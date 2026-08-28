//! D4 save-sidecar hooks.
//!
//! Vanilla saves the per-horse genome at `horse + 0x2b8` via
//! `FUN_14006d470` packed into 240 bytes inside the roster save
//! record written by `FUN_14006ee10`. The roster is iterated by
//! `FUN_14006d610` in `gamestate[+0x130]` vector order. We sidecar
//! the EXTENDED alleles in a separate file `save<N>.dat.ext`
//! co-located with the vanilla save, in the same per-horse order.
//!
//! Hook design:
//! - **D4.1a**: pre-hook the top-level save writer (`FUN_14006dc80`).
//!   On entry: derive the sidecar path from the save channel
//!   (`param_2`), open for write, emit magic + version + horse count
//!   header (count is patched up at end). Set a thread-local-ish
//!   "current sidecar writer" state.
//! - **D4.1b**: post-hook the per-horse save writer
//!   (`FUN_14006ee10`). Append the horse's ext genome (`2 *
//!   EXT_GENE_COUNT` dense bytes) to the open sidecar.
//! - **D4.1c**: post-hook the top-level save writer. On exit: close
//!   the sidecar writer, finalize horse count + CRC.
//!
//! Same shape for D4.2 on the load side (top-level `LOAD_GAME` +
//! per-horse `FUN_14006f150`).
//!
//! Sidecar format (locked 2026-05-15):
//!   magic[8]      "BXSAVEXT"
//!   version: u32  1
//!   ext_count: u32  EXT_GENE_COUNT (240)
//!   horse_count: u32
//!   per_horse_record[horse_count]:
//!       ext_alleles: u8[2 * EXT_GENE_COUNT]
//!   payload_crc: u32 (CRC32 of bytes from magic through last
//!       per_horse_record)
//!
//! Handler discipline: atomics for state machine, parking_lot Mutex
//! only AFTER vanilla trampoline returns (small writes only; never
//! blocks game thread under contention because we're the sole
//! writer).

use crate::genes::{self, EXT_GENE_COUNT};
use crate::targets::{self, fn_addr};
use parking_lot::Mutex;
use retour::GenericDetour;
use std::ffi::c_void;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

/// File magic. 8 bytes.
const MAGIC: &[u8; 8] = b"BXSAVEXT";
/// File format version. Bump on layout-breaking changes.
const VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Signatures
// ---------------------------------------------------------------------------

/// `FUN_14006dc80(gamestate, param_2)`. Top-level save writer.
/// `param_2` is the save channel/slot id (e.g. `save1.dat` -> 1).
type SaveWriterFn = unsafe extern "system" fn(*mut c_void, u32);

/// `FUN_14006e480(gamestate, param_2)`. Top-level loader. Mirror
/// shape per `LOAD_GAME` const.
type LoadGameFn = unsafe extern "system" fn(*mut c_void, u32);

/// `FUN_14006ee10(horse)`. Per-horse save writer.
type HorseSaveWriterFn = unsafe extern "system" fn(*mut c_void);

/// `FUN_14006f150(horse)`. Per-horse save loader.
type HorseSaveLoaderFn = unsafe extern "system" fn(*mut c_void);

// ---------------------------------------------------------------------------
// Detour storage
// ---------------------------------------------------------------------------

static SAVE_WRITER_DETOUR: AtomicPtr<GenericDetour<SaveWriterFn>> =
    AtomicPtr::new(std::ptr::null_mut());
static LOAD_GAME_DETOUR: AtomicPtr<GenericDetour<LoadGameFn>> =
    AtomicPtr::new(std::ptr::null_mut());
static HORSE_WRITER_DETOUR: AtomicPtr<GenericDetour<HorseSaveWriterFn>> =
    AtomicPtr::new(std::ptr::null_mut());
static HORSE_LOADER_DETOUR: AtomicPtr<GenericDetour<HorseSaveLoaderFn>> =
    AtomicPtr::new(std::ptr::null_mut());

// ---------------------------------------------------------------------------
// Sidecar I/O state
// ---------------------------------------------------------------------------

/// Current open sidecar writer state. None when no save is in flight.
struct WriterState {
    file: File,
    path: PathBuf,
    /// Number of per-horse records appended so far.
    horse_count: u32,
}

/// Current open sidecar reader state. None when no load is in flight.
struct ReaderState {
    file: File,
    path: PathBuf,
    /// Horse count parsed from the sidecar header.
    horse_count: u32,
    /// Records consumed so far. Should equal vanilla's iteration order.
    horses_read: u32,
    /// True if the sidecar was missing or invalid; the per-horse
    /// loader detour should leave entries at defaults.
    degraded: bool,
}

fn writer_slot() -> &'static Mutex<Option<WriterState>> {
    static S: OnceLock<Mutex<Option<WriterState>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn reader_slot() -> &'static Mutex<Option<ReaderState>> {
    static S: OnceLock<Mutex<Option<ReaderState>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

static SAVE_CALLS: AtomicU64 = AtomicU64::new(0);
static LOAD_CALLS: AtomicU64 = AtomicU64::new(0);
static HORSE_WRITES: AtomicU64 = AtomicU64::new(0);
static HORSE_READS: AtomicU64 = AtomicU64::new(0);
static FILES_WRITTEN: AtomicU64 = AtomicU64::new(0);
static FILES_READ: AtomicU64 = AtomicU64::new(0);

pub struct Stats {
    pub save_calls: u64,
    pub load_calls: u64,
    pub horse_writes: u64,
    pub horse_reads: u64,
    pub files_written: u64,
    pub files_read: u64,
}

pub fn stats() -> Stats {
    Stats {
        save_calls: SAVE_CALLS.load(Ordering::Relaxed),
        load_calls: LOAD_CALLS.load(Ordering::Relaxed),
        horse_writes: HORSE_WRITES.load(Ordering::Relaxed),
        horse_reads: HORSE_READS.load(Ordering::Relaxed),
        files_written: FILES_WRITTEN.load(Ordering::Relaxed),
        files_read: FILES_READ.load(Ordering::Relaxed),
    }
}

fn reset_stats() {
    SAVE_CALLS.store(0, Ordering::Relaxed);
    LOAD_CALLS.store(0, Ordering::Relaxed);
    HORSE_WRITES.store(0, Ordering::Relaxed);
    HORSE_READS.store(0, Ordering::Relaxed);
    FILES_WRITTEN.store(0, Ordering::Relaxed);
    FILES_READ.store(0, Ordering::Relaxed);
}

pub fn is_armed() -> bool {
    !SAVE_WRITER_DETOUR.load(Ordering::Acquire).is_null()
        && !LOAD_GAME_DETOUR.load(Ordering::Acquire).is_null()
        && !HORSE_WRITER_DETOUR.load(Ordering::Acquire).is_null()
        && !HORSE_LOADER_DETOUR.load(Ordering::Acquire).is_null()
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Sidecar path next to the vanilla save. The game writes
/// `<install_dir>/save/save<N>.dat`. We co-locate as
/// `save<N>.dat.ext`. Falls back to the DLL dir if the save dir
/// can't be resolved.
///
/// Locked decision (2026-05-14 user direction):
///   `C:\Games\Steam\steamapps\common\Horsey Game\save\`.
/// We try CWD-relative `save/save<N>.dat.ext` first (matches vanilla's
/// behavior of writing relative to game install), then fall back to
/// the DLL dir if that's not writable.
pub fn sidecar_path(channel: u32) -> PathBuf {
    let filename = format!("save{channel}.dat.ext");
    // First choice: install_dir/save/. Game CWD is the install dir
    // when launched normally. If CWD isn't writable (rare), fall back
    // to DLL dir which is also under install_dir.
    let cwd_path = std::env::current_dir()
        .ok()
        .map(|p| p.join("save").join(&filename));
    if let Some(p) = cwd_path {
        if p.parent().map(|d| d.is_dir()).unwrap_or(false) {
            return p;
        }
    }
    let dll_dir = modforge::log::dll_dir_wait(std::time::Duration::from_secs(1))
        .unwrap_or_else(|| PathBuf::from("."));
    dll_dir.join(filename)
}

// ---------------------------------------------------------------------------
// CRC32 (IEEE polynomial, table-free implementation)
// ---------------------------------------------------------------------------

fn crc32_update(crc: u32, bytes: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xEDB88320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}

// ---------------------------------------------------------------------------
// Pure codec functions (testable via Cursor<Vec<u8>>; reused by the
// live File-based writer/reader).
// ---------------------------------------------------------------------------

/// Write magic + version + ext_count + horse_count.
pub fn write_header<W: Write>(w: &mut W, ext_count: u32, horse_count: u32) -> std::io::Result<()> {
    w.write_all(MAGIC)?;
    w.write_all(&VERSION.to_le_bytes())?;
    w.write_all(&ext_count.to_le_bytes())?;
    w.write_all(&horse_count.to_le_bytes())?;
    Ok(())
}

/// Header parsed from disk. Returned by [`read_header`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedHeader {
    pub version: u32,
    pub ext_count: u32,
    pub horse_count: u32,
}

/// Parse + validate sidecar header. Bails on bad magic or unsupported
/// version. Caller checks `ext_count == EXT_GENE_COUNT` if it cares
/// about strict match.
pub fn read_header<R: Read>(r: &mut R) -> std::io::Result<ParsedHeader> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("bad magic: {magic:?}"),
        ));
    }
    let mut buf4 = [0u8; 4];
    r.read_exact(&mut buf4)?;
    let version = u32::from_le_bytes(buf4);
    if version != VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported version {version} (expected {VERSION})"),
        ));
    }
    r.read_exact(&mut buf4)?;
    let ext_count = u32::from_le_bytes(buf4);
    r.read_exact(&mut buf4)?;
    let horse_count = u32::from_le_bytes(buf4);
    Ok(ParsedHeader {
        version,
        ext_count,
        horse_count,
    })
}

/// Write one horse's `2 * EXT_GENE_COUNT` dense allele bytes.
pub fn write_horse_record<W: Write>(
    w: &mut W,
    genome: &genes::ExtHorseGenome,
) -> std::io::Result<()> {
    w.write_all(&genome.alleles)
}

/// Read one horse's dense alleles into a fresh `ExtHorseGenome`,
/// masking each byte to 0..3 to defend against tampered files.
pub fn read_horse_record<R: Read>(r: &mut R) -> std::io::Result<genes::ExtHorseGenome> {
    let mut bytes = vec![0u8; 2 * EXT_GENE_COUNT];
    r.read_exact(&mut bytes)?;
    let mut g = genes::ExtHorseGenome::empty();
    for (dst, src) in g.alleles.iter_mut().zip(bytes.iter()) {
        *dst = *src & 0x3;
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn make_genome(seed: u8) -> genes::ExtHorseGenome {
        let mut g = genes::ExtHorseGenome::empty();
        for i in 0..EXT_GENE_COUNT {
            g.alleles[i] = ((seed.wrapping_add(i as u8)) & 0x3) as u8;
            g.alleles[i + EXT_GENE_COUNT] =
                ((seed.wrapping_mul(3).wrapping_add(i as u8)) & 0x3) as u8;
        }
        g
    }

    #[test]
    fn header_roundtrip_preserves_fields() {
        let mut buf = Vec::new();
        write_header(&mut buf, EXT_GENE_COUNT as u32, 42).unwrap();
        let mut r = Cursor::new(&buf);
        let h = read_header(&mut r).unwrap();
        assert_eq!(h.version, VERSION);
        assert_eq!(h.ext_count, EXT_GENE_COUNT as u32);
        assert_eq!(h.horse_count, 42);
    }

    #[test]
    fn header_rejects_bad_magic() {
        let buf = vec![0u8; 20];
        // Leave magic zeroed; rest of header is fine bytes but we
        // shouldn't even reach it.
        let mut r = Cursor::new(&buf[..]);
        let err = read_header(&mut r).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn header_rejects_future_version() {
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&99u32.to_le_bytes()); // version 99
        buf.extend_from_slice(&(EXT_GENE_COUNT as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        let mut r = Cursor::new(&buf);
        let err = read_header(&mut r).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn horse_record_roundtrip_preserves_alleles() {
        let g = make_genome(7);
        let mut buf = Vec::new();
        write_horse_record(&mut buf, &g).unwrap();
        assert_eq!(buf.len(), 2 * EXT_GENE_COUNT);

        let mut r = Cursor::new(&buf);
        let back = read_horse_record(&mut r).unwrap();
        assert_eq!(g.alleles, back.alleles, "round-trip changed alleles");
    }

    #[test]
    fn horse_record_masks_corrupted_bytes_to_0_3() {
        // Disk had bad bytes (someone hand-edited the sidecar).
        // Reader must mask to 0..3 and not propagate corruption.
        let mut bad = vec![0xffu8; 2 * EXT_GENE_COUNT];
        bad[10] = 0xaa;
        let mut r = Cursor::new(&bad);
        let g = read_horse_record(&mut r).unwrap();
        for &a in &g.alleles {
            assert!(a < 4, "byte 0x{a:02x} not masked to 0..3");
        }
    }

    #[test]
    fn full_sidecar_roundtrip_three_horses() {
        // Header + 3 horse records, round-tripped through a Vec<u8>.
        let parents = [make_genome(1), make_genome(2), make_genome(3)];
        let mut buf = Vec::new();
        write_header(&mut buf, EXT_GENE_COUNT as u32, parents.len() as u32).unwrap();
        for p in &parents {
            write_horse_record(&mut buf, p).unwrap();
        }

        // Expected size: 8 magic + 4 version + 4 ext_count + 4 horse_count
        // + 3 horses * 2 * EXT_GENE_COUNT bytes.
        let expected_size = 8 + 4 + 4 + 4 + parents.len() * 2 * EXT_GENE_COUNT;
        assert_eq!(buf.len(), expected_size);

        let mut r = Cursor::new(&buf);
        let h = read_header(&mut r).unwrap();
        assert_eq!(h.horse_count as usize, parents.len());
        for (i, expected) in parents.iter().enumerate() {
            let back = read_horse_record(&mut r).unwrap();
            assert_eq!(expected.alleles, back.alleles, "horse {i} drifted");
        }
    }

    #[test]
    fn crc32_known_vector() {
        // Standard test vector: CRC32 of "123456789" is 0xCBF43926.
        let crc = crc32_update(0, b"123456789");
        assert_eq!(crc, 0xCBF43926, "CRC32 IEEE failed standard test vector");
    }
}

// ---------------------------------------------------------------------------
// D4.1: save-write path
// ---------------------------------------------------------------------------

unsafe extern "system" fn save_writer_handler(gamestate: *mut c_void, channel: u32) {
    SAVE_CALLS.fetch_add(1, Ordering::Relaxed);

    // Open sidecar BEFORE vanilla runs (so per-horse hook has somewhere
    // to append).
    open_writer_or_log(channel);

    let p = SAVE_WRITER_DETOUR.load(Ordering::Acquire);
    if !p.is_null() {
        // SAFETY: pointer published by arm() via Box::into_raw + Release.
        unsafe { (*p).call(gamestate, channel) };
    }

    // Finalize sidecar AFTER vanilla returns.
    close_writer_or_log();
}

fn open_writer_or_log(channel: u32) {
    let path = sidecar_path(channel);
    match open_writer_inner(&path) {
        Ok(state) => {
            *writer_slot().lock() = Some(state);
        }
        Err(e) => {
            modforge::log!(
                "save_sidecar: open writer at {} failed: {e}",
                path.display()
            );
            *writer_slot().lock() = None;
        }
    }
}

fn open_writer_inner(path: &std::path::Path) -> std::io::Result<WriterState> {
    let mut f = File::create(path)?;
    // Header: magic + version + ext_count + horse_count placeholder.
    f.write_all(MAGIC)?;
    f.write_all(&VERSION.to_le_bytes())?;
    f.write_all(&(EXT_GENE_COUNT as u32).to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?; // horse_count placeholder
    Ok(WriterState {
        file: f,
        path: path.to_path_buf(),
        horse_count: 0,
    })
}

fn close_writer_or_log() {
    let Some(state) = writer_slot().lock().take() else {
        return;
    };
    if let Err(e) = close_writer_inner(state) {
        modforge::log!("save_sidecar: close writer failed: {e}");
    } else {
        FILES_WRITTEN.fetch_add(1, Ordering::Relaxed);
    }
}

fn close_writer_inner(mut state: WriterState) -> std::io::Result<()> {
    // Patch horse_count into the header.
    state.file.seek(SeekFrom::Start(8 + 4 + 4))?;
    state.file.write_all(&state.horse_count.to_le_bytes())?;
    state.file.seek(SeekFrom::End(0))?;

    // Compute CRC over everything written so far. Cheap re-read; the
    // sidecar is small (< 64KB typical).
    state.file.flush()?;
    let mut f2 = File::open(&state.path)?;
    let mut buf = Vec::new();
    f2.read_to_end(&mut buf)?;
    let crc = crc32_update(0, &buf);
    state.file.write_all(&crc.to_le_bytes())?;
    modforge::log!(
        "save_sidecar: wrote {} ({} horses, crc=0x{crc:08x})",
        state.path.display(),
        state.horse_count
    );
    Ok(())
}

unsafe extern "system" fn horse_writer_handler(horse: *mut c_void) {
    HORSE_WRITES.fetch_add(1, Ordering::Relaxed);

    let p = HORSE_WRITER_DETOUR.load(Ordering::Acquire);
    if !p.is_null() {
        // SAFETY: detour published; trampoline points at vanilla writer.
        unsafe { (*p).call(horse) };
    }

    if !horse.is_null() {
        append_horse_record(horse as usize as u64);
    }
}

#[inline(never)]
fn append_horse_record(horse_id: u64) {
    let genome =
        genes::get_horse_ext_genome(horse_id).unwrap_or_else(|| genes::ExtHorseGenome::empty());
    let mut slot = writer_slot().lock();
    let Some(state) = slot.as_mut() else {
        // No open writer (vanilla called per-horse writer outside a
        // save-write top-level call? unexpected but ignore).
        return;
    };
    if let Err(e) = state.file.write_all(&genome.alleles) {
        modforge::log!("save_sidecar: append for horse 0x{horse_id:x} failed: {e}");
        return;
    }
    state.horse_count = state.horse_count.saturating_add(1);
}

// ---------------------------------------------------------------------------
// D4.2: save-load path
// ---------------------------------------------------------------------------

unsafe extern "system" fn load_game_handler(gamestate: *mut c_void, channel: u32) {
    LOAD_CALLS.fetch_add(1, Ordering::Relaxed);

    // Open sidecar reader BEFORE vanilla load runs, so per-horse load
    // hook can consume records in order.
    open_reader_or_log(channel);

    let p = LOAD_GAME_DETOUR.load(Ordering::Acquire);
    if !p.is_null() {
        // SAFETY: detour published; trampoline points at vanilla loader.
        unsafe { (*p).call(gamestate, channel) };
    }

    close_reader();
}

fn open_reader_or_log(channel: u32) {
    let path = sidecar_path(channel);
    if !path.exists() {
        modforge::log!(
            "save_sidecar: no sidecar at {} (degraded load; ext alleles default)",
            path.display()
        );
        *reader_slot().lock() = None;
        return;
    }
    match open_reader_inner(&path) {
        Ok(state) => {
            modforge::log!(
                "save_sidecar: opened {} ({} horses expected)",
                state.path.display(),
                state.horse_count
            );
            *reader_slot().lock() = Some(state);
        }
        Err(e) => {
            modforge::log!(
                "save_sidecar: open reader at {} failed: {e} (degraded)",
                path.display()
            );
            *reader_slot().lock() = Some(ReaderState {
                file: File::open(&path).unwrap_or_else(|_| {
                    // Should never reach this since we matched Err above
                    // and exists() was true; placeholder to avoid panic.
                    File::open("nul").expect("placeholder")
                }),
                path,
                horse_count: 0,
                horses_read: 0,
                degraded: true,
            });
        }
    }
}

fn open_reader_inner(path: &std::path::Path) -> std::io::Result<ReaderState> {
    let mut f = File::open(path)?;
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("bad magic: {magic:?}"),
        ));
    }
    let mut buf4 = [0u8; 4];
    f.read_exact(&mut buf4)?;
    let version = u32::from_le_bytes(buf4);
    if version != VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported version: {version} (expected {VERSION})"),
        ));
    }
    f.read_exact(&mut buf4)?;
    let ext_count = u32::from_le_bytes(buf4);
    if ext_count as usize != EXT_GENE_COUNT {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("ext_count mismatch: file={ext_count} build={EXT_GENE_COUNT}"),
        ));
    }
    f.read_exact(&mut buf4)?;
    let horse_count = u32::from_le_bytes(buf4);
    Ok(ReaderState {
        file: f,
        path: path.to_path_buf(),
        horse_count,
        horses_read: 0,
        degraded: false,
    })
}

fn close_reader() {
    let Some(state) = reader_slot().lock().take() else {
        return;
    };
    if !state.degraded {
        FILES_READ.fetch_add(1, Ordering::Relaxed);
    }
    modforge::log!(
        "save_sidecar: closed {} (read {}/{} records, degraded={})",
        state.path.display(),
        state.horses_read,
        state.horse_count,
        state.degraded
    );
}

unsafe extern "system" fn horse_loader_handler(horse: *mut c_void) {
    HORSE_READS.fetch_add(1, Ordering::Relaxed);

    let p = HORSE_LOADER_DETOUR.load(Ordering::Acquire);
    if !p.is_null() {
        // Vanilla loader runs first, populating horse + 0x2b8 and
        // other fields. It then calls FUN_1400b3070 to regen render
        // fields. To make our ext alleles available DURING that regen,
        // we need to populate EXT_HORSE_GENOMES BEFORE vanilla calls
        // FUN_1400b3070. Since FUN_1400b3070 runs at the END of the
        // vanilla loader, populating just after the trampoline returns
        // is too late.
        //
        // Workaround: populate ext alleles BEFORE calling vanilla.
        // Vanilla's FUN_14006f150 doesn't depend on EXT_HORSE_GENOMES;
        // it only writes vanilla state. So this ordering is safe.
        if !horse.is_null() {
            consume_horse_record(horse as usize as u64);
        }
        // SAFETY: detour published; trampoline points at vanilla loader.
        unsafe { (*p).call(horse) };
    }
}

#[inline(never)]
fn consume_horse_record(horse_id: u64) {
    let mut slot = reader_slot().lock();
    let Some(state) = slot.as_mut() else {
        // No sidecar; ensure the entry exists at default.
        drop(slot);
        let _ = genes::ensure_horse_ext_genome(horse_id);
        return;
    };
    if state.degraded || state.horses_read >= state.horse_count {
        drop(slot);
        let _ = genes::ensure_horse_ext_genome(horse_id);
        return;
    }
    let mut bytes = vec![0u8; 2 * EXT_GENE_COUNT];
    if let Err(e) = state.file.read_exact(&mut bytes) {
        modforge::log!(
            "save_sidecar: read horse record failed at idx {}: {e}",
            state.horses_read
        );
        state.degraded = true;
        drop(slot);
        let _ = genes::ensure_horse_ext_genome(horse_id);
        return;
    }
    state.horses_read += 1;
    drop(slot);

    // Mask alleles to 0..3 in case of bad data.
    let mut g = genes::ExtHorseGenome::empty();
    for (dst, src) in g.alleles.iter_mut().zip(bytes.iter()) {
        *dst = *src & 0x3;
    }
    genes::set_horse_ext_genome(horse_id, g);
}

// ---------------------------------------------------------------------------
// Dryrun
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TargetReport {
    pub name: &'static str,
    pub rva: usize,
    pub runtime_addr: usize,
    pub prologue_bytes: [u8; 16],
}

fn dryrun_at(name: &'static str, rva: usize) -> TargetReport {
    let runtime_addr = targets::rebase(rva);
    let mut prologue = [0u8; 16];
    // SAFETY: function entry inside our running image; read-only.
    let view = unsafe { std::slice::from_raw_parts(runtime_addr as *const u8, 16) };
    prologue.copy_from_slice(view);
    TargetReport {
        name,
        rva,
        runtime_addr,
        prologue_bytes: prologue,
    }
}

pub fn dryrun_all() -> [TargetReport; 4] {
    [
        dryrun_at("SAVE_WRITER", fn_addr::SAVE_WRITER),
        dryrun_at("LOAD_GAME", fn_addr::LOAD_GAME),
        dryrun_at("HORSE_SAVE_WRITER", fn_addr::HORSE_SAVE_WRITER),
        dryrun_at("HORSE_SAVE_LOADER", fn_addr::HORSE_SAVE_LOADER),
    ]
}

// ---------------------------------------------------------------------------
// Arm / revert
// ---------------------------------------------------------------------------

pub fn arm() -> anyhow::Result<()> {
    let save_writer = crate::targets_registry::resolve::save_writer()
        .unwrap_or_else(|| targets::rebase(fn_addr::SAVE_WRITER));
    let load_game = crate::targets_registry::resolve::load_game()
        .unwrap_or_else(|| targets::rebase(fn_addr::LOAD_GAME));
    let horse_save_writer = crate::targets_registry::resolve::horse_save_writer()
        .unwrap_or_else(|| targets::rebase(fn_addr::HORSE_SAVE_WRITER));
    let horse_save_loader = crate::targets_registry::resolve::horse_save_loader()
        .unwrap_or_else(|| targets::rebase(fn_addr::HORSE_SAVE_LOADER));

    arm_one(
        "SAVE_WRITER",
        save_writer,
        &SAVE_WRITER_DETOUR,
        save_writer_handler,
    )
    .map_err(|e| anyhow::anyhow!("save_sidecar: arm SAVE_WRITER: {e}"))?;
    if let Err(e) = arm_one("LOAD_GAME", load_game, &LOAD_GAME_DETOUR, load_game_handler) {
        revert();
        return Err(anyhow::anyhow!("save_sidecar: arm LOAD_GAME: {e}"));
    }
    if let Err(e) = arm_one(
        "HORSE_SAVE_WRITER",
        horse_save_writer,
        &HORSE_WRITER_DETOUR,
        horse_writer_handler,
    ) {
        revert();
        return Err(anyhow::anyhow!("save_sidecar: arm HORSE_SAVE_WRITER: {e}"));
    }
    if let Err(e) = arm_one(
        "HORSE_SAVE_LOADER",
        horse_save_loader,
        &HORSE_LOADER_DETOUR,
        horse_loader_handler,
    ) {
        revert();
        return Err(anyhow::anyhow!("save_sidecar: arm HORSE_SAVE_LOADER: {e}"));
    }
    reset_stats();
    Ok(())
}

fn arm_one<F: Copy>(
    name: &'static str,
    runtime_addr: usize,
    slot: &AtomicPtr<GenericDetour<F>>,
    handler: F,
) -> anyhow::Result<()>
where
    F: retour::Function,
{
    if !slot.load(Ordering::Acquire).is_null() {
        anyhow::bail!("{name}: already armed");
    }
    // SAFETY: target inside our loaded image; 8 bytes readable.
    let prologue = unsafe { std::slice::from_raw_parts(runtime_addr as *const u8, 8) };
    let prologue_hex: String = prologue
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    modforge::log!("save_sidecar: {name} prologue=[{prologue_hex}]");

    // SAFETY: address is the true entry of the named function;
    // F's signature matches the decomp prototype.
    let target: F = unsafe { std::mem::transmute_copy(&runtime_addr) };
    // SAFETY: signatures match.
    let detour = unsafe { GenericDetour::new(target, handler) }
        .map_err(|e| anyhow::anyhow!("{name}: GenericDetour::new failed: {e}"))?;
    // SAFETY: enable installs JMP + trampoline.
    unsafe { detour.enable() }.map_err(|e| anyhow::anyhow!("{name}: enable failed: {e}"))?;
    let leaked: *mut GenericDetour<F> = Box::into_raw(Box::new(detour));
    slot.store(leaked, Ordering::Release);
    modforge::log!("save_sidecar: armed {name} at 0x{runtime_addr:x}");
    Ok(())
}

pub fn revert() {
    revert_one("HORSE_SAVE_LOADER", &HORSE_LOADER_DETOUR);
    revert_one("HORSE_SAVE_WRITER", &HORSE_WRITER_DETOUR);
    revert_one("LOAD_GAME", &LOAD_GAME_DETOUR);
    revert_one("SAVE_WRITER", &SAVE_WRITER_DETOUR);

    // Drop any in-flight sidecar state.
    *writer_slot().lock() = None;
    *reader_slot().lock() = None;
}

fn revert_one<F>(name: &'static str, slot: &AtomicPtr<GenericDetour<F>>)
where
    F: retour::Function,
{
    let p = slot.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if p.is_null() {
        return;
    }
    // SAFETY: pointer published by arm_one; sole owner now.
    let det = unsafe { Box::from_raw(p) };
    // SAFETY: disable restores the prologue.
    let res = unsafe { det.disable() };
    match res {
        Ok(()) => modforge::log!("save_sidecar: reverted {name}"),
        Err(e) => modforge::log!("save_sidecar: revert {name} FAILED: {e}"),
    }
}

//! In-memory binary patches. Each patch is applied at DLL attach
//! and reverted at detach.
//!
//! All patches go through [`patch_bytes`] which takes care of
//! VirtualProtect, byte write, and protection restore. Original
//! bytes are saved in [`APPLIED`] so revert() puts them back.
//!
//! Submodules:
//! - [`sleep_safe_no_tire`]: NOPs the +0x206 zero-store inside the
//!   no_tire loop so the day-cycle sleep gate keeps working.
//! - [`ext_genes`]: DI-A detours (3 of 5 v1 wired). Arms via HTTP.
//! - [`render_trampoline`]: D5 post-hook on FUN_14009f680 that
//!   layers extended-gene render mappings onto vanilla's buf.
//!   Arms via HTTP.
//! - [`combinator`]: D3.4 post-hook on FUN_1400a2d80 that runs an
//!   independent Mendelian combinator over EXT_HORSE_GENOMES so
//!   children inherit extended alleles from parents. Arms via HTTP.

pub mod combinator;
pub mod ext_genes;
pub mod lifecycle;
pub mod render_trampoline;
pub mod save_sidecar;

use parking_lot::Mutex;
use windows_sys::Win32::System::Memory::{
    PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect,
};

/// Record of one applied patch.
#[derive(Clone)]
struct Patch {
    /// Symbolic name for diagnostics.
    name: &'static str,
    /// Runtime address.
    addr: usize,
    /// Original bytes overwritten by the patch.
    original: Vec<u8>,
}

static APPLIED: Mutex<Vec<Patch>> = Mutex::new(Vec::new());

/// Write `new_bytes` at `addr`, recording the originals for revert.
/// `addr` must be inside the running Horsey.exe image.
pub fn patch_bytes(name: &'static str, addr: usize, new_bytes: &[u8]) -> anyhow::Result<()> {
    let len = new_bytes.len();
    // SAFETY: We trust the caller to pass a valid in-process address.
    // The slice points to code-segment memory; VirtualProtect below
    // makes the page writable, we copy, then we restore.
    let view = unsafe { std::slice::from_raw_parts(addr as *const u8, len) };
    let original: Vec<u8> = view.to_vec();

    let mut old_protect: PAGE_PROTECTION_FLAGS = 0;
    // SAFETY: addr+len is in our own process; VirtualProtect on our
    // own image's code pages is always safe.
    let ok = unsafe {
        VirtualProtect(
            addr as *mut _,
            len,
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        )
    };
    if ok == 0 {
        anyhow::bail!("VirtualProtect RW failed at 0x{addr:x}");
    }

    // SAFETY: page is RW; write within bounds; we own the slice.
    unsafe {
        std::ptr::copy_nonoverlapping(new_bytes.as_ptr(), addr as *mut u8, len);
    }

    // Restore original protection.
    let mut tmp: PAGE_PROTECTION_FLAGS = 0;
    // SAFETY: same address + length as the prior VirtualProtect call.
    unsafe { VirtualProtect(addr as *mut _, len, old_protect, &mut tmp) };

    // Flush instruction cache. On x64 this is a no-op for correctness
    // (the CPU coheres icache with stores) but we call it for
    // form and forward-compat.
    // SAFETY: Win32 API call with valid handle and address.
    unsafe {
        let h_proc = windows_sys::Win32::System::Threading::GetCurrentProcess();
        windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache(
            h_proc,
            addr as *const _,
            len,
        );
    }

    APPLIED.lock().push(Patch {
        name,
        addr,
        original,
    });
    modforge::log!(
        "patch[{name}]: applied at 0x{addr:x} ({} bytes)",
        new_bytes.len()
    );
    Ok(())
}

/// Revert every applied patch, in reverse order. Also reverts the
/// ext_genes detours and the D5 render trampoline (no-op if never
/// armed).
pub fn revert_all() {
    // LIFO order: most recently armed reverts first.
    save_sidecar::revert();
    lifecycle::revert();
    combinator::revert();
    render_trampoline::revert();
    ext_genes::revert();
    let mut g = APPLIED.lock();
    while let Some(p) = g.pop() {
        let len = p.original.len();
        let mut old_protect: PAGE_PROTECTION_FLAGS = 0;
        // SAFETY: addr was a valid in-process code page when applied.
        unsafe {
            VirtualProtect(
                p.addr as *mut _,
                len,
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            );
            std::ptr::copy_nonoverlapping(p.original.as_ptr(), p.addr as *mut u8, len);
            let mut tmp: PAGE_PROTECTION_FLAGS = 0;
            VirtualProtect(p.addr as *mut _, len, old_protect, &mut tmp);
        }
        modforge::log!("patch[{}]: reverted at 0x{:x}", p.name, p.addr);
    }
}

pub fn applied_names() -> Vec<&'static str> {
    APPLIED.lock().iter().map(|p| p.name).collect()
}

// =============================================================================
// Patch: sleep_safe_no_tire
// =============================================================================

pub mod sleep_safe_no_tire {
    //! The game's `no_tire` cheat at FUN_1400ceb60 zeroes BOTH +0x205
    //! AND +0x206 on every horse every frame. +0x206 is what the sleep
    //! gate at FUN_1400e0aa0 line 131551 counts, so when no_tire is on
    //! the player can't advance the day cycle.
    //!
    //! We NOP the instruction that zeroes +0x206, leaving the +0x205
    //! zero intact. Race eligibility (which reads +0x205) is bypassed;
    //! sleep eligibility (which reads +0x206) keeps working normally.
    //!
    //! The instruction to find:
    //!   `mov byte ptr [<reg> + 0x206], 0`
    //! encoded as 7 bytes: `C6 <modrm> 06 02 00 00 00`
    //! where <modrm> is `10xxx000` for [reg+disp32].
    //!
    //! There's a sibling instruction at +0x205 (the one we want to KEEP).
    //! We match on `06 02` to disambiguate.

    use super::patch_bytes;
    use crate::targets;

    pub fn apply() -> anyhow::Result<()> {
        // Pattern-resolved function bounds via the no_tire byte-zero
        // pair anchor + int3-padding walkout. Falls back to
        // hardcoded `NO_TIRE_LOOP_FN_RVA / BODY_SIZE` if anchor
        // misses.
        let fn_start = targets::horse_offset::no_tire_loop_entry();
        let fn_size = targets::horse_offset::no_tire_loop_size();
        // SAFETY: fn_start..fn_start+fn_size is inside the running
        // Horsey.exe image (resolver returned bounds within .text).
        // Bytes are static for the lifetime of the process.
        let bytes = unsafe { std::slice::from_raw_parts(fn_start as *const u8, fn_size) };

        let (offset, length) = find_patch_site(bytes)?;
        let patch_addr = fn_start + offset;
        // NOP fill of whatever length the instruction was.
        let nops = vec![0x90u8; length];
        patch_bytes("sleep_safe_no_tire", patch_addr, &nops)?;
        Ok(())
    }

    /// Match the no_tire loop body: a `mov byte ptr [reg+0x206], 0`
    /// IMMEDIATELY followed by a `mov byte ptr [reg+0x205], 0` (the
    /// only place in the binary that does both back-to-back is the
    /// no_tire per-frame loop). Returns
    /// `(offset_of_0x206_instr, length_of_0x206_instr)`.
    ///
    /// We accept either 7-byte (no REX) or 8-byte (REX-prefixed)
    /// instruction encoding for each of the two stores.
    fn find_patch_site(bytes: &[u8]) -> anyhow::Result<(usize, usize)> {
        // Collect every +TIRED_FLAG_B=0 store with its instruction
        // length. Also collect every +TIRED_FLAG_A=0 store (for
        // proximity matching). Offsets resolved via R4 so they
        // track on game updates.
        let disp_b = targets::horse_offset::tired_flag_b() as u32;
        let disp_a = targets::horse_offset::tired_flag_a() as u32;
        let mut sites_206: Vec<(usize, usize)> = Vec::new();
        let mut sites_205: Vec<(usize, usize)> = Vec::new();
        for i in 0..bytes.len() {
            let (m206, l206) = parse_zero_store(&bytes[i..], disp_b);
            if m206 {
                sites_206.push((i, l206));
            }
            let (m205, l205) = parse_zero_store(&bytes[i..], disp_a);
            if m205 {
                sites_205.push((i, l205));
            }
        }

        // The no_tire body in source is:
        //   *(...+0x206) = 0;
        //   *(...+0x205) = 0;
        // The compiler may interleave loop bookkeeping between them.
        // Heuristic: pick the +0x206 store that has a +0x205 store
        // within the next ~64 bytes.
        let mut found: Option<(usize, usize)> = None;
        for (i_206, l_206) in &sites_206 {
            let near = sites_205
                .iter()
                .any(|(i_205, _)| *i_205 > *i_206 && *i_205 - *i_206 < 64);
            if near {
                if found.is_some() {
                    anyhow::bail!(
                        "sleep_safe_no_tire: multiple +0x206 stores have a \
                         nearby +0x205 sibling; can't disambiguate. \
                         206 sites: {:?}, 205 sites: {:?}",
                        sites_206,
                        sites_205,
                    );
                }
                found = Some((*i_206, *l_206));
            }
        }
        if let Some(site) = found {
            return Ok(site);
        }
        anyhow::bail!(
            "sleep_safe_no_tire: no +0x206 store has a +0x205 sibling within \
             64 bytes. 206 sites: {:?}, 205 sites: {:?}",
            sites_206,
            sites_205,
        )
    }

    /// If `bytes` starts with a `mov byte ptr [reg + disp], 0` whose
    /// disp32 equals `expected_disp`, return `(true, instr_len)` where
    /// instr_len is 7 or 8 depending on REX prefix. Otherwise
    /// `(false, 0)`.
    fn parse_zero_store(bytes: &[u8], expected_disp: u32) -> (bool, usize) {
        let exp = expected_disp.to_le_bytes();
        // 7-byte: C6 modrm <disp32> 00
        if bytes.len() >= 7
            && bytes[0] == 0xc6
            && (bytes[1] & 0xF8) == 0x80
            && bytes[2] == exp[0]
            && bytes[3] == exp[1]
            && bytes[4] == exp[2]
            && bytes[5] == exp[3]
            && bytes[6] == 0x00
        {
            return (true, 7);
        }
        // 8-byte: REX (0x40..0x4F) C6 modrm <disp32> 00
        if bytes.len() >= 8
            && (bytes[0] & 0xF0) == 0x40
            && bytes[1] == 0xc6
            && (bytes[2] & 0xF8) == 0x80
            && bytes[3] == exp[0]
            && bytes[4] == exp[1]
            && bytes[5] == exp[2]
            && bytes[6] == exp[3]
            && bytes[7] == 0x00
        {
            return (true, 8);
        }
        (false, 0)
    }
}

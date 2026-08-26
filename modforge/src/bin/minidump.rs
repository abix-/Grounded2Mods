//! Read a Windows minidump without a debugger.
//!
//! When a game dies it leaves a `.dmp` and nothing else useful.
//! This answers the first question that matters: what was the
//! exception, where did it happen, and WHOSE code is at that
//! address, the game's or the mod's.
//!
//! ```text
//! minidump <path-to-dmp> [pdb ...]
//! ```
//!
//! Built by `cargo build -p modforge --bin minidump`.
//!
//! Deliberately small. It reads the header, the module list and
//! the exception record. It does NOT unwind a call stack, which
//! needs unwind tables; the faulting address plus its owning
//! module is usually enough to say whether a crash is ours.

use std::env;
use std::fs;

/// `MDMP`, little-endian.
const SIGNATURE: u32 = 0x504D_444D;

const STREAM_MODULE_LIST: u32 = 4;
const STREAM_EXCEPTION: u32 = 6;

/// `MINIDUMP_MODULE` is 108 bytes.
const MODULE_ENTRY_SIZE: usize = 108;

struct Dump {
    bytes: Vec<u8>,
}

impl Dump {
    fn u32_at(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap_or_default())
    }

    fn u64_at(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.bytes[at..at + 8].try_into().unwrap_or_default())
    }

    /// `MINIDUMP_STRING`: a byte length then UTF-16 characters.
    fn string_at(&self, rva: usize) -> String {
        if rva + 4 > self.bytes.len() {
            return String::new();
        }
        let len = self.u32_at(rva) as usize;
        let start = rva + 4;
        let end = (start + len).min(self.bytes.len());
        let units: Vec<u16> = self.bytes[start..end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    }

    /// (stream type, size, rva) for every stream in the directory.
    fn streams(&self) -> Vec<(u32, usize, usize)> {
        let count = self.u32_at(0x08) as usize;
        let dir = self.u32_at(0x0C) as usize;
        (0..count)
            .map(|i| {
                let at = dir + i * 12;
                (
                    self.u32_at(at),
                    self.u32_at(at + 4) as usize,
                    self.u32_at(at + 8) as usize,
                )
            })
            .collect()
    }

    fn stream(&self, kind: u32) -> Option<(usize, usize)> {
        self.streams()
            .into_iter()
            .find(|(t, _, _)| *t == kind)
            .map(|(_, size, rva)| (size, rva))
    }
}

struct Module {
    base: u64,
    size: u64,
    name: String,
}

fn modules(d: &Dump) -> Vec<Module> {
    let Some((_, rva)) = d.stream(STREAM_MODULE_LIST) else {
        return Vec::new();
    };
    let count = d.u32_at(rva) as usize;
    (0..count)
        .map(|i| {
            let at = rva + 4 + i * MODULE_ENTRY_SIZE;
            Module {
                base: d.u64_at(at),
                size: d.u32_at(at + 8) as u64,
                name: d.string_at(d.u32_at(at + 20) as usize),
            }
        })
        .collect()
}

/// The module an address falls inside, if any.
fn owner<'a>(mods: &'a [Module], addr: u64) -> Option<&'a Module> {
    mods.iter()
        .find(|m| addr >= m.base && addr < m.base + m.size)
}

fn short(name: &str) -> &str {
    name.rsplit(['\\', '/']).next().unwrap_or(name)
}

/// The common Windows exception codes, named. Anything else is
/// printed as a bare code rather than guessed at.
fn exception_name(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "ACCESS_VIOLATION",
        0xC000_001D => "ILLEGAL_INSTRUCTION",
        0xC000_0025 => "NONCONTINUABLE_EXCEPTION",
        0xC000_008C => "ARRAY_BOUNDS_EXCEEDED",
        0xC000_0094 => "INT_DIVIDE_BY_ZERO",
        0xC000_00FD => "STACK_OVERFLOW",
        0xC000_0409 => "STACK_BUFFER_OVERRUN / __fastfail",
        0x8000_0003 => "BREAKPOINT",
        0xE060_6573 => "unhandled C++ exception",
        _ => "",
    }
}

/// The function containing `rva` in a PDB, as (name, start rva).
///
/// Release builds strip symbols from the DLL, but the PDB beside
/// it keeps them, so a stripped crash address is still
/// answerable. Public symbols give the start of each function;
/// the one with the greatest start not past `rva` contains it.
fn symbolize(pdb_path: &str, rva: u32) -> Option<(String, u32)> {
    use pdb::FallibleIterator;
    let file = fs::File::open(pdb_path).ok()?;
    let mut pdb = pdb::PDB::open(file).ok()?;
    let symbols = pdb.global_symbols().ok()?;
    let addresses = pdb.address_map().ok()?;

    let mut best: Option<(String, u32)> = None;
    let mut iter = symbols.iter();
    while let Ok(Some(sym)) = iter.next() {
        let pdb::SymbolData::Public(data) = sym.parse().ok()? else {
            continue;
        };
        if !data.function {
            continue;
        }
        let Some(start) = data.offset.to_rva(&addresses) else {
            continue;
        };
        let start = start.0;
        if start > rva {
            continue;
        }
        if best.as_ref().is_none_or(|(_, b)| start > *b) {
            best = Some((data.name.to_string().into_owned(), start));
        }
    }
    best
}

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: minidump <path-to-dmp> [pdb ...]");
        eprintln!("  pdb files are matched to modules by file name");
        std::process::exit(2);
    };
    let pdbs: Vec<String> = env::args().skip(2).collect();
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(1);
        }
    };
    let d = Dump { bytes };
    if d.bytes.len() < 32 || d.u32_at(0) != SIGNATURE {
        eprintln!("{path} is not a minidump");
        std::process::exit(1);
    }

    let mods = modules(&d);
    println!("{path}");
    println!("{} module(s)", mods.len());

    let Some((_, rva)) = d.stream(STREAM_EXCEPTION) else {
        println!("no exception stream: the dump was not taken at a crash");
        return;
    };
    // MINIDUMP_EXCEPTION_STREAM: ThreadId, alignment, then the
    // MINIDUMP_EXCEPTION record.
    let thread_id = d.u32_at(rva);
    let rec = rva + 8;
    let code = d.u32_at(rec);
    let flags = d.u32_at(rec + 4);
    let addr = d.u64_at(rec + 16);
    let nparams = d.u32_at(rec + 24) as usize;

    let named = exception_name(code);
    println!("\nthread      {thread_id}");
    println!(
        "exception   {code:#010x}{}{named}",
        if named.is_empty() { "" } else { "  " }
    );
    println!("flags       {flags:#x}");
    match owner(&mods, addr) {
        Some(m) => {
            let rva = (addr - m.base) as u32;
            println!("address     {addr:#x}  {} + {rva:#x}", short(&m.name));
            // Any PDB given on the command line is tried; a
            // mismatched one simply finds nothing.
            for p in &pdbs {
                if let Some((name, start)) = symbolize(p, rva) {
                    println!(
                        "function    {name}  (+{:#x} into it, from {})",
                        rva - start,
                        short(p)
                    );
                }
            }
        }
        None => println!("address     {addr:#x}  (no module owns this address)"),
    }

    // An access violation puts the operation and the target
    // address in its parameters, which says read vs write and
    // WHAT was touched.
    if nparams >= 2 && code == 0xC000_0005 {
        let op = d.u64_at(rec + 32);
        let target = d.u64_at(rec + 40);
        let what = match op {
            0 => "reading",
            1 => "writing",
            8 => "executing",
            _ => "accessing",
        };
        match owner(&mods, target) {
            Some(m) => println!(
                "fault       {what} {target:#x}  ({} + {:#x})",
                short(&m.name),
                target - m.base
            ),
            None => println!("fault       {what} {target:#x}"),
        }
    }

    println!("\nloaded modules around the fault:");
    let mut sorted: Vec<&Module> = mods.iter().collect();
    sorted.sort_by_key(|m| m.base);
    for m in sorted.iter().filter(|m| {
        let n = short(&m.name).to_ascii_lowercase();
        n.contains("main") || n.contains("ue4ss") || n.contains("shipping") || n.ends_with(".exe")
    }) {
        println!(
            "  {:#018x} .. {:#018x}  {}",
            m.base,
            m.base + m.size,
            short(&m.name)
        );
    }
}

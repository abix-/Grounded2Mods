//! Windows process introspection helpers. Pure `windows-sys` calls
//! against the current process. No UE / game state needed. Useful
//! in any Windows mod for diagnosing CPU and memory issues:
//!
//! - `process_threads_json`. Per-thread CPU times (UE5 uses
//!   `GameThread`, `RenderThread`, `RHIThread`,
//!   `TaskGraphThreadHP*`); biggest delta = cycle thief.
//! - `process_cpu_json`. Total user/kernel CPU time
//!   (FILETIME -> nanoseconds).
//! - `process_regions_json`. VirtualQuery walk, bucket committed
//!   bytes by `(image|mapped|private)` x `(rwx|rx|rw|r)`, plus
//!   private-region size histogram, AllocationBase count, top-N
//!   regions with mapped file name. Diff two snapshots = where the
//!   leak is.
//! - `sample_thread_modules_json`. SuspendThread + GetThreadContext
//!   per-thread RIP-by-module sampler. Statistical "who is burning
//!   CPU" without DbgHelp.
//! - `process_memory_json`. PSAPI working set, private usage,
//!   pagefile, page faults.
//!
//! ## Universal SAFETY contract (all `unsafe` blocks in this module)
//!
//! Every `unsafe { ... }` block in this file calls a `windows-sys`
//! FFI on the current process. Three shape families cover them
//! all:
//!
//! 1. **Toolhelp32 / PSAPI snapshot iteration**: opens an OS
//!    snapshot handle, walks entries via `Thread32First /
//!    ToolHelp32...Next` or `GetMappedFileNameW`, closes the
//!    handle on exit. Each entry is a `#[repr(C)]` Windows struct
//!    whose first field is the byte size; we zero-init via
//!    `mem::zeroed`, set `dwSize`, then let the API populate the
//!    rest. Snapshots are per-call, no aliasing.
//!
//! 2. **VirtualQuery / region walking**: reads
//!    `MEMORY_BASIC_INFORMATION` for a given address, advancing
//!    through the process address space. Bounded by the kernel's
//!    own region boundary returns; we always advance by
//!    `RegionSize` so the loop terminates. The Windows kernel
//!    handles invalid input safely (returns 0 written) so the
//!    function-side null/junk checks are conservative.
//!
//! 3. **Thread sampling**: `SuspendThread` + `GetThreadContext`
//!    + `ResumeThread`. The TID being sampled is one we just
//!    enumerated from a snapshot; if it died between enumeration
//!    and sample, `OpenThread` returns null + we skip cleanly.
//!    Suspend/Resume calls are paired; we always Resume on the
//!    same code path that Suspends.
//!
//! All three shapes are categorically safe given current-process
//! semantics (we never sample another process, never share OS
//! handles across threads). The cost of repeating the same
//! SAFETY justification 45 times across this file outweighs the
//! lint discipline payoff; we allow the workspace lint at module
//! level with the documented contract above.
#![allow(clippy::too_many_lines)]
#![allow(clippy::undocumented_unsafe_blocks)]

/// Cheap "would dereferencing this address segfault?" check. Uses
/// `VirtualQuery` to confirm the page is committed and not
/// PAGE_NOACCESS / PAGE_GUARD. Returns `false` for null + low
/// addresses without syscalling.
///
/// Use this to guard pointer reads on hot UE traversal paths
/// where a corrupt back-pointer would otherwise crash the game
/// process. The check is microsecond-scale but not free; only
/// apply to reads that have actually shown up in crash dumps.
pub fn is_addr_readable(addr: usize) -> bool {
    if addr < 0x1_0000 {
        return false;
    }
    // `region::query` accepts any address; it returns Err for
    // unmapped regions. Safe wrapper, no unsafe block needed.
    let region = match region::query(addr as *const std::ffi::c_void) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if !region.is_committed() {
        return false;
    }
    if region.is_guarded() {
        return false;
    }
    // PAGE_NOACCESS: every Protection flag bit is clear.
    if region.protection() == region::Protection::NONE {
        return false;
    }
    true
}

pub fn process_threads_json() -> serde_json::Value {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, GetThreadDescription, GetThreadTimes, OpenThread,
        THREAD_QUERY_LIMITED_INFORMATION,
    };

    let pid = unsafe { GetCurrentProcessId() };
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snap == INVALID_HANDLE_VALUE || snap.is_null() {
        return serde_json::json!({"error": "CreateToolhelp32Snapshot failed"});
    }

    let mut entries: Vec<serde_json::Value> = Vec::new();
    let mut te: THREADENTRY32 = unsafe { std::mem::zeroed() };
    te.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

    let to_ns = |ft: FILETIME| -> u64 {
        let combined = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
        combined.saturating_mul(100)
    };

    if unsafe { Thread32First(snap, &mut te) } != 0 {
        loop {
            if te.th32OwnerProcessID == pid {
                let h = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, te.th32ThreadID) };
                if !h.is_null() {
                    let mut creation = FILETIME {
                        dwLowDateTime: 0,
                        dwHighDateTime: 0,
                    };
                    let mut exit = FILETIME {
                        dwLowDateTime: 0,
                        dwHighDateTime: 0,
                    };
                    let mut kernel = FILETIME {
                        dwLowDateTime: 0,
                        dwHighDateTime: 0,
                    };
                    let mut user = FILETIME {
                        dwLowDateTime: 0,
                        dwHighDateTime: 0,
                    };
                    let ok = unsafe {
                        GetThreadTimes(h, &mut creation, &mut exit, &mut kernel, &mut user)
                    };

                    // GetThreadDescription returns a wide-char
                    // string allocated via LocalAlloc; we must
                    // LocalFree it.
                    // GetThreadDescription returns a buffer that
                    // must be freed with LocalFree. windows-sys's
                    // re-export path varies; declare directly.
                    unsafe extern "system" {
                        fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
                    }
                    let mut name_ptr: *mut u16 = std::ptr::null_mut();
                    let name = unsafe {
                        let _ = GetThreadDescription(h, &mut name_ptr);
                        if !name_ptr.is_null() {
                            let mut len = 0usize;
                            while *name_ptr.add(len) != 0 {
                                len += 1;
                            }
                            let slice = std::slice::from_raw_parts(name_ptr, len);
                            let s = String::from_utf16_lossy(slice);
                            LocalFree(name_ptr as *mut _);
                            s
                        } else {
                            format!("tid_{}", te.th32ThreadID)
                        }
                    };

                    if ok != 0 {
                        let user_ns = to_ns(user);
                        let kernel_ns = to_ns(kernel);
                        entries.push(serde_json::json!({
                            "tid": te.th32ThreadID,
                            "name": name,
                            "cpu_user_ns": user_ns,
                            "cpu_kernel_ns": kernel_ns,
                            "cpu_total_ns": user_ns + kernel_ns,
                        }));
                    }
                    unsafe { CloseHandle(h) };
                }
            }
            if unsafe { Thread32Next(snap, &mut te) } == 0 {
                break;
            }
        }
    }
    unsafe { CloseHandle(snap) };

    entries.sort_by_key(|v| {
        std::cmp::Reverse(v.get("cpu_total_ns").and_then(|x| x.as_u64()).unwrap_or(0))
    });
    serde_json::json!({"threads": entries})
}

/// Process CPU time readings (user + kernel, in nanoseconds).
/// `GetProcessTimes` returns FILETIME (100-ns ticks); we
/// convert to nanoseconds. Diff over a window gives the
/// process's total CPU consumption. Compare against our
/// `TIME_NS_*` counters to compute our fractional load.
pub fn process_cpu_json() -> serde_json::Value {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut kernel = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut user = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return serde_json::json!({"error": "GetProcessTimes failed"});
    }
    let to_ns = |ft: FILETIME| -> u64 {
        let combined = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
        // FILETIME is 100-ns intervals.
        combined.saturating_mul(100)
    };
    serde_json::json!({
        "process_cpu_user_ns":   to_ns(user),
        "process_cpu_kernel_ns": to_ns(kernel),
        "process_cpu_total_ns":  to_ns(user) + to_ns(kernel),
    })
}

/// Walk the entire user-mode address space via VirtualQueryEx,
/// grouping committed regions by (state, protection, type).
/// Diffing two snapshots reveals which memory category grew.
///
/// Categories:
///   - Image: pages mapped from a loaded module (.exe/.dll). Code
///     and read-only data. Growth here means more DLLs loaded.
///   - Mapped: pages backed by a file or pagefile (e.g. memory-
///     mapped asset packs, shared sections). Growth = file/asset
///     buffers.
///   - Private: anonymous pages (heap, stacks, DXGI/RHI resources
///     that allocate via VirtualAlloc). Growth = native heap or
///     resource allocations.
///
/// Protection buckets we expose:
///   - rwx: read+write+execute (JIT'd code regions, shaders, etc.)
///   - rx:  read+execute (code)
///   - rw:  read+write (data, heap)
///   - r:   read only (constants, mapped read-only sections)
///   - other: anything else (no-access guards, etc.)
///
/// Plus top N largest individual committed regions, with module
/// name where applicable. The fastest-growing region in a diff
/// is the leak.
///
/// Cost: O(N) over committed regions (~thousands). Allocates;
/// snapshot-only path, not hot path.
pub fn process_regions_json() -> serde_json::Value {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_FREE, MEM_IMAGE, MEM_MAPPED, MEM_PRIVATE, MEM_RESERVE,
        MEMORY_BASIC_INFORMATION, PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
        PAGE_EXECUTE_WRITECOPY, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY,
        VirtualQuery,
    };
    use windows_sys::Win32::System::ProcessStatus::GetMappedFileNameW;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut total_committed: u64 = 0;
    let mut total_reserved: u64 = 0;
    let mut total_free: u64 = 0;
    let mut by_type_image: u64 = 0;
    let mut by_type_mapped: u64 = 0;
    let mut by_type_private: u64 = 0;
    let mut by_prot_rwx: u64 = 0;
    let mut by_prot_rx: u64 = 0;
    let mut by_prot_rw: u64 = 0;
    let mut by_prot_r: u64 = 0;
    let mut by_prot_other: u64 = 0;
    let mut region_count: u64 = 0;
    let mut committed_count: u64 = 0;

    // Histogram of committed-region sizes (private only.
    // the leak we are chasing). Buckets cover the UE binned
    // allocator page sizes plus the larger D3D resource heaps.
    let mut hist_priv_4k: u64 = 0;
    let mut hist_priv_64k: u64 = 0;
    let mut hist_priv_128k: u64 = 0;
    let mut hist_priv_256k: u64 = 0;
    let mut hist_priv_1m: u64 = 0;
    let mut hist_priv_4m: u64 = 0;
    let mut hist_priv_16m: u64 = 0;
    let mut hist_priv_64m_plus: u64 = 0;
    let mut hist_priv_4k_bytes: u64 = 0;
    let mut hist_priv_64k_bytes: u64 = 0;
    let mut hist_priv_128k_bytes: u64 = 0;
    let mut hist_priv_256k_bytes: u64 = 0;
    let mut hist_priv_1m_bytes: u64 = 0;
    let mut hist_priv_4m_bytes: u64 = 0;
    let mut hist_priv_16m_bytes: u64 = 0;
    let mut hist_priv_64m_plus_bytes: u64 = 0;

    // Track unique AllocationBases for committed-private regions.
    // Each unique AllocationBase = one VirtualAlloc'd zone.
    // A growing count = the game is calling VirtualAlloc more.
    // A stable count with growing committed bytes = the game is
    // committing pages within already-reserved zones.
    let mut alloc_bases_private: std::collections::HashSet<usize> =
        std::collections::HashSet::new();

    // Track top-N largest committed regions for the report.
    // Region key collapses adjacent same-type/same-protection
    // pages. We don't merge here. VirtualQuery already returns
    // "regions" of contiguous identical pages.
    let mut top_regions: Vec<(u64, u64, u32, u32, String)> = Vec::new(); // (size, base, type, prot, mapped_name)

    let mut addr: usize = 0;
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let info_size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();

    // VirtualQuery on our own process. Pass null as the process
    // handle equivalent uses VirtualQuery (not Ex). VirtualQuery
    // takes (lpAddress, lpBuffer, dwLength) and returns bytes
    // written.
    loop {
        let written = unsafe { VirtualQuery(addr as *const _, &mut info, info_size) };
        if written == 0 {
            break;
        }
        region_count += 1;
        let size = info.RegionSize as u64;
        let state = info.State;
        let prot = info.Protect;
        let mtype = info.Type;

        // Track unique AllocationBases for commits (counts
        // distinct VirtualAlloc'd zones).
        // Track region size histogram (small heap pages vs
        // big resource heaps).

        if state == MEM_FREE {
            total_free += size;
        } else if state == MEM_RESERVE {
            total_reserved += size;
        } else if state == MEM_COMMIT {
            total_committed += size;
            committed_count += 1;

            if mtype == MEM_IMAGE {
                by_type_image += size;
            } else if mtype == MEM_MAPPED {
                by_type_mapped += size;
            } else if mtype == MEM_PRIVATE {
                by_type_private += size;
                alloc_bases_private.insert(info.AllocationBase as usize);
                if size < 64 * 1024 {
                    hist_priv_4k += 1;
                    hist_priv_4k_bytes += size;
                } else if size < 128 * 1024 {
                    hist_priv_64k += 1;
                    hist_priv_64k_bytes += size;
                } else if size < 256 * 1024 {
                    hist_priv_128k += 1;
                    hist_priv_128k_bytes += size;
                } else if size < 1024 * 1024 {
                    hist_priv_256k += 1;
                    hist_priv_256k_bytes += size;
                } else if size < 4 * 1024 * 1024 {
                    hist_priv_1m += 1;
                    hist_priv_1m_bytes += size;
                } else if size < 16 * 1024 * 1024 {
                    hist_priv_4m += 1;
                    hist_priv_4m_bytes += size;
                } else if size < 64 * 1024 * 1024 {
                    hist_priv_16m += 1;
                    hist_priv_16m_bytes += size;
                } else {
                    hist_priv_64m_plus += 1;
                    hist_priv_64m_plus_bytes += size;
                }
            }

            // Strip out the modifier flags (PAGE_GUARD,
            // PAGE_NOCACHE, PAGE_WRITECOMBINE) by masking to the
            // low 8 bits of the access protection.
            let base_prot = prot & 0xFF;
            if base_prot == PAGE_EXECUTE_READWRITE || base_prot == PAGE_EXECUTE_WRITECOPY {
                by_prot_rwx += size;
            } else if base_prot == PAGE_EXECUTE || base_prot == PAGE_EXECUTE_READ {
                by_prot_rx += size;
            } else if base_prot == PAGE_READWRITE || base_prot == PAGE_WRITECOPY {
                by_prot_rw += size;
            } else if base_prot == PAGE_READONLY {
                by_prot_r += size;
            } else if base_prot == PAGE_NOACCESS {
                by_prot_other += size;
            } else {
                by_prot_other += size;
            }

            // Capture top regions. For mapped/image, look up
            // the file name so we know what is loaded.
            if size >= 1024 * 1024 {
                let name = if mtype == MEM_IMAGE || mtype == MEM_MAPPED {
                    let mut buf = [0u16; 260];
                    let len = unsafe {
                        GetMappedFileNameW(
                            GetCurrentProcess(),
                            info.BaseAddress,
                            buf.as_mut_ptr(),
                            buf.len() as u32,
                        )
                    };
                    if len > 0 {
                        String::from_utf16_lossy(&buf[..len as usize])
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                top_regions.push((size, info.BaseAddress as u64, mtype, prot, name));
            }
        }

        // Advance past this region. VirtualQuery returns
        // contiguous regions; just add RegionSize to the base.
        let next = info.BaseAddress as usize + info.RegionSize;
        if next <= addr {
            // Wrap-around guard.
            break;
        }
        addr = next;
    }

    top_regions.sort_by(|a, b| b.0.cmp(&a.0));
    let top: Vec<serde_json::Value> = top_regions
        .into_iter()
        .take(40)
        .map(|(size, base, t, prot, name)| {
            let type_str = match t {
                MEM_IMAGE => "image",
                MEM_MAPPED => "mapped",
                MEM_PRIVATE => "private",
                _ => "other",
            };
            serde_json::json!({
                "size_bytes": size,
                "size_mb": size as f64 / (1024.0 * 1024.0),
                "base": format!("0x{base:X}"),
                "type": type_str,
                "protect": format!("0x{prot:X}"),
                "mapped_name": name,
            })
        })
        .collect();

    serde_json::json!({
        "region_count":            region_count,
        "committed_region_count":  committed_count,
        "total_committed_bytes":   total_committed,
        "total_reserved_bytes":    total_reserved,
        "total_free_bytes":        total_free,
        "by_type_image_bytes":     by_type_image,
        "by_type_mapped_bytes":    by_type_mapped,
        "by_type_private_bytes":   by_type_private,
        "by_prot_rwx_bytes":       by_prot_rwx,
        "by_prot_rx_bytes":        by_prot_rx,
        "by_prot_rw_bytes":        by_prot_rw,
        "by_prot_r_bytes":         by_prot_r,
        "by_prot_other_bytes":     by_prot_other,
        "top_committed_regions":   top,
        "private_alloc_bases":     alloc_bases_private.len() as u64,
        "private_hist": {
            "lt_64k_count":   hist_priv_4k,
            "lt_64k_bytes":   hist_priv_4k_bytes,
            "lt_128k_count":  hist_priv_64k,
            "lt_128k_bytes":  hist_priv_64k_bytes,
            "lt_256k_count":  hist_priv_128k,
            "lt_256k_bytes":  hist_priv_128k_bytes,
            "lt_1m_count":    hist_priv_256k,
            "lt_1m_bytes":    hist_priv_256k_bytes,
            "lt_4m_count":    hist_priv_1m,
            "lt_4m_bytes":    hist_priv_1m_bytes,
            "lt_16m_count":   hist_priv_4m,
            "lt_16m_bytes":   hist_priv_4m_bytes,
            "lt_64m_count":   hist_priv_16m,
            "lt_64m_bytes":   hist_priv_16m_bytes,
            "ge_64m_count":   hist_priv_64m_plus,
            "ge_64m_bytes":   hist_priv_64m_plus_bytes,
        },
    })
}

/// Sample every thread's instruction pointer N times over a
/// window. For each sample, look up which loaded module the
/// RIP belongs to. Per-thread histogram of "which module is
/// each thread spending time in". Statistical caller
/// attribution without needing DbgHelp / StackWalk64.
///
/// This answers the "WHO is burning CPU / who is allocating"
/// question one frame deep. If a thread is mostly in
/// `Grounded2-WinGRTS-Shipping.exe`, that thread is running
/// game code. Mostly in `ntdll.dll`/`kernelbase.dll` =
/// allocator/syscall heavy. Mostly in `nvrtum64.dll` = NVIDIA
/// shader compiler / driver work. Etc.
///
/// Caveat: only the topmost frame (the actual RIP). A thread
/// inside the allocator shows ntdll. To know who CALLED the
/// allocator we'd need StackWalk64. This is good enough as a
/// first cut.
///
/// Cost: SuspendThread + GetThreadContext + ResumeThread per
/// thread per sample. ~100us per sample. With 50 threads and
/// 10 Hz sampling: 50 ms / sec = 5% overhead while sampling.
/// Acceptable for an on-demand probe.
///
/// args: duration_ms, interval_ms.
pub fn sample_thread_modules_json(duration_ms: u32, interval_ms: u32) -> serde_json::Value {
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::Debug::CONTEXT;
    unsafe extern "system" {
        // GetThreadContext lives in kernel32 / kernelbase but
        // windows-sys exposes it under Win32::System::Threading
        // for x86_64. Re-declare locally to avoid the feature
        // flag dance.
        fn GetThreadContext(hThread: *mut std::ffi::c_void, lpContext: *mut CONTEXT) -> i32;
    }
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, GetCurrentThreadId, GetThreadDescription, OpenThread, ResumeThread,
        SuspendThread, THREAD_GET_CONTEXT, THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME,
    };

    // Snapshot the loaded module map once: list of (base, size, short_name).
    // Looking up RIP -> module is a linear scan; with ~100 modules that's
    // fine for 10 Hz sampling.
    let modules: Vec<(u64, u64, String)> = loaded_modules();

    let module_for = |rip: u64| -> String {
        for (base, size, name) in &modules {
            if rip >= *base && rip < *base + *size {
                return name.clone();
            }
        }
        String::new() // empty = unmapped (likely stack data, not code)
    };

    // System-DLL filter. When RIP is in one of these, the thread
    // is inside the OS / allocator / driver. Walking up the stack
    // finds the user-code caller. Which is the actual "WHO."
    let is_system = |m: &str| -> bool {
        let lower = m.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "ntdll.dll"
                | "kernelbase.dll"
                | "kernel32.dll"
                | "win32u.dll"
                | "user32.dll"
                | "gdi32.dll"
                | "gdi32full.dll"
                | "msvcp140.dll"
                | "msvcrt.dll"
                | "vcruntime140.dll"
                | "vcruntime140_1.dll"
                | "ucrtbase.dll"
                | "rpcrt4.dll"
                | "advapi32.dll"
                | "sechost.dll"
        )
    };

    // Read 8 bytes at addr from our process, safely. Returns
    // None if the address is unreadable. Used to walk stacks.
    let read_qword = |addr: u64| -> Option<u64> {
        use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
        let proc = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
        let mut buf: u64 = 0;
        let mut read: usize = 0;
        let ok = unsafe {
            ReadProcessMemory(
                proc,
                addr as *const _,
                &mut buf as *mut _ as *mut _,
                8,
                &mut read,
            )
        };
        if ok != 0 && read == 8 {
            Some(buf)
        } else {
            None
        }
    };

    // Walk stack starting at RSP looking for the first non-system,
    // non-empty (= mapped) module's code address. Cap at STACK_PROBE
    // qwords to keep sampling cheap. Returns (module_name, depth)
    // where depth is the qword index that hit (0 = top).
    const STACK_PROBE: u64 = 64;
    let resolve_caller = |rip: u64, rsp: u64| -> (String, u32) {
        let m = module_for(rip);
        if !m.is_empty() && !is_system(&m) {
            return (m, 0);
        }
        for i in 1..STACK_PROBE {
            let addr = rsp.saturating_add(i.saturating_mul(8));
            if let Some(v) = read_qword(addr) {
                let mm = module_for(v);
                if !mm.is_empty() && !is_system(&mm) {
                    return (mm, i as u32);
                }
            }
        }
        // All system modules deep. Report where the topmost
        // non-empty module sits.
        if !m.is_empty() {
            return (m, 0);
        }
        ("<unmapped>".to_string(), 0)
    };

    // Per-thread state: tid -> (name, total_samples, by_module)
    let mut per_thread: HashMap<u32, (String, u64, HashMap<String, u64>)> = HashMap::new();
    let mut grand_by_module: HashMap<String, u64> = HashMap::new();
    let mut total_samples: u64 = 0;

    let start = std::time::Instant::now();
    let our_tid = unsafe { GetCurrentThreadId() };
    let our_pid = unsafe { GetCurrentProcessId() };

    while start.elapsed().as_millis() < duration_ms as u128 {
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            break;
        }
        let mut te: THREADENTRY32 = unsafe { std::mem::zeroed() };
        te.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        if unsafe { Thread32First(snap, &mut te) } != 0 {
            loop {
                if te.th32OwnerProcessID == our_pid && te.th32ThreadID != our_tid {
                    let h = unsafe {
                        OpenThread(
                            THREAD_SUSPEND_RESUME
                                | THREAD_GET_CONTEXT
                                | THREAD_QUERY_LIMITED_INFORMATION,
                            0,
                            te.th32ThreadID,
                        )
                    };
                    if !h.is_null() {
                        let prev = unsafe { SuspendThread(h) };
                        if prev != u32::MAX {
                            let mut ctx = std::mem::MaybeUninit::<CONTEXT>::zeroed();
                            unsafe {
                                // CONTEXT_CONTROL on x64 is 0x100001.
                                // We only need RIP so this is enough.
                                (*ctx.as_mut_ptr()).ContextFlags = 0x00100001;
                            }
                            let ok = unsafe { GetThreadContext(h, ctx.as_mut_ptr()) };
                            if ok != 0 {
                                let ctx = unsafe { ctx.assume_init() };
                                let (m, _depth) = resolve_caller(ctx.Rip, ctx.Rsp);
                                let entry =
                                    per_thread.entry(te.th32ThreadID).or_insert_with(|| {
                                        // Resolve thread name once.
                                        let mut nptr: *mut u16 = std::ptr::null_mut();
                                        unsafe extern "system" {
                                            fn LocalFree(
                                                hMem: *mut std::ffi::c_void,
                                            ) -> *mut std::ffi::c_void;
                                        }
                                        let nm = unsafe {
                                            let _ = GetThreadDescription(h, &mut nptr);
                                            if !nptr.is_null() {
                                                let mut len = 0usize;
                                                while *nptr.add(len) != 0 {
                                                    len += 1;
                                                }
                                                let s = String::from_utf16_lossy(
                                                    std::slice::from_raw_parts(nptr, len),
                                                );
                                                LocalFree(nptr as *mut _);
                                                s
                                            } else {
                                                format!("tid_{}", te.th32ThreadID)
                                            }
                                        };
                                        (nm, 0, HashMap::new())
                                    });
                                entry.1 += 1;
                                *entry.2.entry(m.clone()).or_insert(0) += 1;
                                *grand_by_module.entry(m).or_insert(0) += 1;
                                total_samples += 1;
                            }
                            unsafe { ResumeThread(h) };
                        }
                        unsafe { CloseHandle(h) };
                    }
                }
                if unsafe { Thread32Next(snap, &mut te) } == 0 {
                    break;
                }
            }
        }
        unsafe { CloseHandle(snap) };
        std::thread::sleep(std::time::Duration::from_millis(interval_ms as u64));
    }

    // Format per-thread output. Sort threads by total samples desc.
    let mut threads_out: Vec<(u32, String, u64, Vec<(String, u64)>)> = per_thread
        .into_iter()
        .map(|(tid, (name, count, by_mod))| {
            let mut by: Vec<(String, u64)> = by_mod.into_iter().collect();
            by.sort_by(|a, b| b.1.cmp(&a.1));
            (tid, name, count, by)
        })
        .collect();
    threads_out.sort_by(|a, b| b.2.cmp(&a.2));

    let threads_json: Vec<serde_json::Value> = threads_out
        .into_iter()
        .map(|(tid, name, count, by_mod)| {
            let by: Vec<serde_json::Value> = by_mod
                .into_iter()
                .take(10)
                .map(|(m, c)| {
                    serde_json::json!({
                        "module": m,
                        "samples": c,
                        "pct": if count > 0 { (c as f64 * 100.0) / count as f64 } else { 0.0 },
                    })
                })
                .collect();
            serde_json::json!({
                "tid": tid,
                "name": name,
                "samples": count,
                "by_module": by,
            })
        })
        .collect();

    let mut grand: Vec<(String, u64)> = grand_by_module.into_iter().collect();
    grand.sort_by(|a, b| b.1.cmp(&a.1));
    let grand_json: Vec<serde_json::Value> = grand
        .into_iter()
        .take(20)
        .map(|(m, c)| {
            serde_json::json!({
                "module": m,
                "samples": c,
                "pct": if total_samples > 0 { (c as f64 * 100.0) / total_samples as f64 } else { 0.0 },
            })
        })
        .collect();

    serde_json::json!({
        "duration_ms": duration_ms,
        "interval_ms": interval_ms,
        "total_samples": total_samples,
        "by_module_grand_total": grand_json,
        "by_thread": threads_json,
    })
}

/// Process memory readings (Windows). Captured in the snapshot
/// so the perf test can compute deltas and detect leaks beyond
pub fn process_memory_json() -> serde_json::Value {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut info = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS_EX>::zeroed();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let ok =
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), info.as_mut_ptr() as *mut _, size) };
    if ok == 0 {
        return serde_json::json!({"error": "GetProcessMemoryInfo failed"});
    }
    let info = unsafe { info.assume_init() };
    serde_json::json!({
        "working_set_size":              info.WorkingSetSize          as u64,
        "peak_working_set_size":         info.PeakWorkingSetSize      as u64,
        "private_usage":                 info.PrivateUsage            as u64,
        "pagefile_usage":                info.PagefileUsage           as u64,
        "peak_pagefile_usage":           info.PeakPagefileUsage       as u64,
        "page_fault_count":              info.PageFaultCount          as u64,
        "quota_paged_pool_usage":        info.QuotaPagedPoolUsage     as u64,
        "quota_peak_paged_pool_usage":   info.QuotaPeakPagedPoolUsage as u64,
        "quota_non_paged_pool_usage":    info.QuotaNonPagedPoolUsage  as u64,
        "quota_peak_non_paged_pool_usage": info.QuotaPeakNonPagedPoolUsage as u64,
    })
}

/// Enumerate loaded modules as `(base, size, short_name)`. A linear
/// scan of this list maps a RIP or return address to the module it
/// lives in. Used by the thread sampler and the write watchpoint.
pub fn loaded_modules() -> Vec<(u64, u64, String)> {
    use windows_sys::Win32::System::ProcessStatus::{
        EnumProcessModulesEx, GetModuleFileNameExW, GetModuleInformation, LIST_MODULES_ALL,
        MODULEINFO,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let proc = GetCurrentProcess();
        let mut h_modules: Vec<*mut std::ffi::c_void> = vec![std::ptr::null_mut(); 1024];
        let cb_needed = (h_modules.len() * std::mem::size_of::<*mut std::ffi::c_void>()) as u32;
        let mut written: u32 = 0;
        let ok = EnumProcessModulesEx(
            proc,
            h_modules.as_mut_ptr(),
            cb_needed,
            &mut written,
            LIST_MODULES_ALL,
        );
        if ok == 0 {
            return Vec::new();
        }
        let n = (written as usize) / std::mem::size_of::<*mut std::ffi::c_void>();
        h_modules.truncate(n);
        let mut out = Vec::with_capacity(n);
        for &h in &h_modules {
            let mut info = std::mem::MaybeUninit::<MODULEINFO>::zeroed();
            let info_ok = GetModuleInformation(
                proc,
                h,
                info.as_mut_ptr(),
                std::mem::size_of::<MODULEINFO>() as u32,
            );
            if info_ok == 0 {
                continue;
            }
            let info = info.assume_init();
            let mut name_buf = [0u16; 260];
            let nlen = GetModuleFileNameExW(proc, h, name_buf.as_mut_ptr(), name_buf.len() as u32);
            let name = if nlen > 0 {
                let s = String::from_utf16_lossy(&name_buf[..nlen as usize]);
                s.rsplit(['\\', '/']).next().unwrap_or(&s).to_string()
            } else {
                format!("module_{:p}", h)
            };
            out.push((info.lpBaseOfDll as u64, info.SizeOfImage as u64, name));
        }
        out
    }
}

// --- hardware write watchpoint --------------------------------------
//
// Watch one memory address for writes and record which instruction
// (and its caller chain) performed each write. This is the "find out
// what writes to this address" technique: a CPU debug-register data
// breakpoint plus a vectored exception handler. The game keeps
// running; when the watched bytes are written, the CPU traps, the
// handler records the faulting RIP and every code return address on
// the stack, then lets execution continue.
//
// Debug registers are per thread, so the address is armed on every
// thread in the process. Whichever thread does the write is caught.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const WATCH_MAX_RECORDS: usize = 24;
const WATCH_MAX_FRAMES: usize = 40;
const WATCH_STACK_BYTES: usize = 1024;

#[derive(Clone, Copy)]
struct WatchRecord {
    rip: usize,
    tid: u32,
    // Integer argument registers at the hit. For an exec breakpoint on
    // a function entry these are the incoming Windows x64 arguments
    // (rcx, rdx, r8, r9), which name the `this` pointer and reveal a
    // function's parameters.
    rcx: u64,
    rdx: u64,
    r8: u64,
    r9: u64,
    frame_count: u32,
    frames: [usize; WATCH_MAX_FRAMES],
}

struct WatchBuf {
    slots: [UnsafeCell<WatchRecord>; WATCH_MAX_RECORDS],
}
// SAFETY: each slot is written by exactly one thread, the one whose
// `WATCH_COUNT.fetch_add` returned that index; readers run only after
// the watchpoint is disarmed and all handler activity has stopped.
unsafe impl Sync for WatchBuf {}

static WATCH_BUF: WatchBuf = WatchBuf {
    slots: [const {
        UnsafeCell::new(WatchRecord {
            rip: 0,
            tid: 0,
            rcx: 0,
            rdx: 0,
            r8: 0,
            r9: 0,
            frame_count: 0,
            frames: [0; WATCH_MAX_FRAMES],
        })
    }; WATCH_MAX_RECORDS],
};
static WATCH_COUNT: AtomicUsize = AtomicUsize::new(0);
static WATCH_ACTIVE: AtomicBool = AtomicBool::new(false);
static WATCH_EXE_BASE: AtomicUsize = AtomicUsize::new(0);
static WATCH_EXE_END: AtomicUsize = AtomicUsize::new(0);
// An execution breakpoint traps BEFORE the instruction runs (a fault,
// not a trap), so the handler must clear this thread's breakpoint on
// every hit or the same instruction re-traps forever.
static WATCH_MODE_EXEC: AtomicBool = AtomicBool::new(false);

/// Vectored exception handler for the write watchpoint. Fires as a
/// STATUS_SINGLE_STEP once the armed data breakpoint (DR0) trips.
unsafe extern "system" fn watch_handler(
    info: *mut windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
) -> i32 {
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    const EXCEPTION_CONTINUE_EXECUTION: i32 = -1;
    const STATUS_SINGLE_STEP: u32 = 0x8000_0004;

    if !WATCH_ACTIVE.load(Ordering::Acquire) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    // SAFETY: Windows guarantees `info` and its records are valid for
    // the duration of this callback.
    let rec = unsafe { &*(*info).ExceptionRecord };
    if rec.ExceptionCode as u32 != STATUS_SINGLE_STEP {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    // SAFETY: same validity guarantee; we take a mutable view to clear
    // the debug status and optionally silence the breakpoint.
    let ctx = unsafe { &mut *(*info).ContextRecord };
    // DR0 is the only breakpoint we arm; its trip is Dr6 bit 0.
    if ctx.Dr6 & 0x1 == 0 {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let idx = WATCH_COUNT.fetch_add(1, Ordering::AcqRel);
    if idx < WATCH_MAX_RECORDS {
        let base = WATCH_EXE_BASE.load(Ordering::Relaxed);
        let end = WATCH_EXE_END.load(Ordering::Relaxed);
        // SAFETY: `idx` is owned exclusively by this handler invocation.
        let slot = unsafe { &mut *WATCH_BUF.slots[idx].get() };
        slot.rip = ctx.Rip as usize;
        slot.tid = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
        slot.rcx = ctx.Rcx;
        slot.rdx = ctx.Rdx;
        slot.r8 = ctx.R8;
        slot.r9 = ctx.R9;
        slot.frame_count = 0;

        // Copy a window of the interrupted thread's stack via a
        // fault-safe read, then keep the qwords that point into the
        // main exe's code. Those are the return addresses of the call
        // chain that reached this write.
        let mut buf = [0u8; WATCH_STACK_BYTES];
        let mut got: usize = 0;
        let ok = unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory(
                windows_sys::Win32::System::Threading::GetCurrentProcess(),
                ctx.Rsp as *const _,
                buf.as_mut_ptr() as *mut _,
                buf.len(),
                &mut got,
            )
        };
        if ok != 0 {
            let mut fc = 0usize;
            let mut i = 0usize;
            while i + 8 <= got && fc < WATCH_MAX_FRAMES {
                let v = usize::from_le_bytes(buf[i..i + 8].try_into().unwrap());
                if v >= base && v < end {
                    slot.frames[fc] = v;
                    fc += 1;
                }
                i += 8;
            }
            slot.frame_count = fc as u32;
        }
    }

    // Clear the debug status so the trap does not re-report.
    ctx.Dr6 = 0;
    // Silence this thread's breakpoint once enough hits are captured,
    // or on every hit in exec mode (where the fault would otherwise
    // re-trap the same instruction forever).
    if WATCH_MODE_EXEC.load(Ordering::Relaxed)
        || WATCH_COUNT.load(Ordering::Acquire) >= WATCH_MAX_RECORDS
    {
        ctx.Dr0 = 0;
        ctx.Dr7 = 0;
    }
    EXCEPTION_CONTINUE_EXECUTION
}

/// DR7 value that arms DR0 as a local breakpoint. Encoding: L0 enable
/// (bit 0), R/W0 (bits 16..17): 00 = execute, 01 = write, 11 = read or
/// write. LEN0 (bits 18..19): 1 byte=00, 2=01, 8=10, 4=11 (execute
/// requires LEN 00).
fn dr7_watch(mode_exec: bool, read_write: bool, len: u8) -> u64 {
    if mode_exec {
        return 1 << 0; // R/W=00 execute, LEN0=00
    }
    let rw: u64 = if read_write { 0b11 } else { 0b01 };
    let len_bits: u64 = match len {
        1 => 0b00,
        2 => 0b01,
        8 => 0b10,
        _ => 0b11, // 4 bytes
    };
    (1 << 0) | (rw << 16) | (len_bits << 18)
}

/// Per-stage counts from one `set_dr_all_threads` pass, so a failure
/// to arm points at the exact syscall that refused.
#[derive(Default, Clone, Copy)]
struct DrSetDiag {
    seen: usize,
    opened: usize,
    suspended: usize,
    got: usize,
    set: usize,
    last_err: u32,
}

/// Write `dr0` / `dr7` into every thread except the caller, so a data
/// breakpoint is armed (or cleared, with zeros) process-wide.
fn set_dr_all_threads(dr0: u64, dr7: u64) -> DrSetDiag {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::Debug::{
        CONTEXT, GetThreadContext, SetThreadContext,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread, SuspendThread,
        THREAD_GET_CONTEXT, THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME,
    };

    // CONTEXT_DEBUG_REGISTERS on x86_64 = CONTEXT_AMD64 | 0x10.
    const CONTEXT_DEBUG_REGISTERS: u32 = 0x0010_0010;

    let mut diag = DrSetDiag::default();
    let our_pid = unsafe { GetCurrentProcessId() };
    let our_tid = unsafe { GetCurrentThreadId() };
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snap == INVALID_HANDLE_VALUE || snap.is_null() {
        diag.last_err = unsafe { GetLastError() };
        return diag;
    }
    let mut te: THREADENTRY32 = unsafe { std::mem::zeroed() };
    te.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

    if unsafe { Thread32First(snap, &mut te) } != 0 {
        loop {
            if te.th32OwnerProcessID == our_pid && te.th32ThreadID != our_tid {
                diag.seen += 1;
                let h = unsafe {
                    OpenThread(
                        THREAD_GET_CONTEXT | THREAD_SET_CONTEXT | THREAD_SUSPEND_RESUME,
                        0,
                        te.th32ThreadID,
                    )
                };
                if h.is_null() {
                    diag.last_err = unsafe { GetLastError() };
                } else {
                    diag.opened += 1;
                    if unsafe { SuspendThread(h) } != u32::MAX {
                        diag.suspended += 1;
                        let mut ctx = std::mem::MaybeUninit::<CONTEXT>::zeroed();
                        // SAFETY: ContextFlags is a transparent u32; we
                        // request only the debug-register block.
                        unsafe {
                            (*ctx.as_mut_ptr()).ContextFlags = CONTEXT_DEBUG_REGISTERS;
                        }
                        if unsafe { GetThreadContext(h, ctx.as_mut_ptr()) } != 0 {
                            diag.got += 1;
                            // SAFETY: populated by GetThreadContext.
                            unsafe {
                                let c = ctx.as_mut_ptr();
                                (*c).ContextFlags = CONTEXT_DEBUG_REGISTERS;
                                (*c).Dr0 = dr0;
                                (*c).Dr6 = 0;
                                (*c).Dr7 = dr7;
                            }
                            if unsafe { SetThreadContext(h, ctx.as_ptr()) } != 0 {
                                diag.set += 1;
                            } else {
                                diag.last_err = unsafe { GetLastError() };
                            }
                        } else {
                            diag.last_err = unsafe { GetLastError() };
                        }
                        unsafe { ResumeThread(h) };
                    } else {
                        diag.last_err = unsafe { GetLastError() };
                    }
                    unsafe { CloseHandle(h) };
                }
            }
            if unsafe { Thread32Next(snap, &mut te) } == 0 {
                break;
            }
        }
    }
    unsafe { CloseHandle(snap) };
    diag
}

/// Watch `addr` for `mode` access (`"write"`, `"readwrite"`, or
/// `"exec"`) of `len` bytes (1/2/4/8; execute forces 1) for
/// `duration_ms`, then report the instruction and stack-return-address
/// chain of each hit, resolved to module + RVA. This is the
/// non-guessing way to find which code writes a piece of state, or
/// whether a candidate function actually runs on a physical action:
/// arm the address, take the action, read back the exact code.
pub fn capture_write_watchpoint(
    addr: usize,
    len: u8,
    duration_ms: u32,
    mode: &str,
) -> serde_json::Value {
    use windows_sys::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, RemoveVectoredExceptionHandler,
    };

    let mode_exec = mode == "exec";
    let read_write = mode == "readwrite";
    if !mode_exec && !matches!(len, 1 | 2 | 4 | 8) {
        return serde_json::json!({"error": "len must be 1, 2, 4, or 8"});
    }
    if !mode_exec && addr % len as usize != 0 {
        return serde_json::json!({
            "error": format!("addr 0x{addr:x} is not aligned to len {len}"),
        });
    }
    if WATCH_ACTIVE.load(Ordering::Acquire) {
        return serde_json::json!({"error": "a watchpoint is already active"});
    }

    let modules = loaded_modules();
    // The main exe is the first module EnumProcessModules returns.
    let (exe_base, exe_size, exe_name) = modules
        .first()
        .cloned()
        .unwrap_or((0, 0, String::from("<unknown>")));
    WATCH_EXE_BASE.store(exe_base as usize, Ordering::Relaxed);
    WATCH_EXE_END.store((exe_base + exe_size) as usize, Ordering::Relaxed);
    WATCH_COUNT.store(0, Ordering::Release);

    // SAFETY: the callback signature matches PVECTORED_EXCEPTION_HANDLER.
    let veh = unsafe { AddVectoredExceptionHandler(1, Some(watch_handler)) };
    if veh.is_null() {
        return serde_json::json!({"error": "AddVectoredExceptionHandler returned NULL"});
    }

    WATCH_MODE_EXEC.store(mode_exec, Ordering::Relaxed);
    WATCH_ACTIVE.store(true, Ordering::Release);
    let arm = set_dr_all_threads(addr as u64, dr7_watch(mode_exec, read_write, len));

    std::thread::sleep(std::time::Duration::from_millis(duration_ms as u64));

    let clear = set_dr_all_threads(0, 0);
    WATCH_ACTIVE.store(false, Ordering::Release);
    // SAFETY: `veh` is the handle just returned by the matching add.
    unsafe { RemoveVectoredExceptionHandler(veh) };

    let resolve = |a: u64| -> (String, u64) {
        for (b, s, name) in &modules {
            if a >= *b && a < *b + *s {
                return (name.clone(), a - *b);
            }
        }
        (String::new(), a)
    };

    let hit_total = WATCH_COUNT.load(Ordering::Acquire);
    let recorded = hit_total.min(WATCH_MAX_RECORDS);
    let mut records = Vec::with_capacity(recorded);
    for i in 0..recorded {
        // SAFETY: the watchpoint is disarmed; no handler can be writing
        // this slot now, and indices below `recorded` were populated.
        let slot = unsafe { *WATCH_BUF.slots[i].get() };
        let (rip_mod, rip_rva) = resolve(slot.rip as u64);
        let frames: Vec<serde_json::Value> = (0..slot.frame_count as usize)
            .map(|f| {
                let a = slot.frames[f] as u64;
                let (m, rva) = resolve(a);
                serde_json::json!({
                    "addr": format!("0x{a:x}"),
                    "module": m,
                    "rva": format!("0x{rva:x}"),
                })
            })
            .collect();
        records.push(serde_json::json!({
            "tid": slot.tid,
            "rip_after_write": format!("0x{:x}", slot.rip),
            "rip_module": rip_mod,
            "rip_rva": format!("0x{rip_rva:x}"),
            "rcx": format!("0x{:x}", slot.rcx),
            "rdx": format!("0x{:x}", slot.rdx),
            "r8": format!("0x{:x}", slot.r8),
            "r9": format!("0x{:x}", slot.r9),
            "stack_return_addrs": frames,
        }));
    }

    serde_json::json!({
        "watch_addr": format!("0x{addr:x}"),
        "len": len,
        "mode": mode,
        "duration_ms": duration_ms,
        "exe_name": exe_name,
        "exe_base": format!("0x{exe_base:x}"),
        "exe_size": format!("0x{exe_size:x}"),
        "threads_armed": arm.set,
        "threads_cleared": clear.set,
        "arm_diag": {
            "seen": arm.seen,
            "opened": arm.opened,
            "suspended": arm.suspended,
            "got_context": arm.got,
            "set_context": arm.set,
            "last_err": arm.last_err,
        },
        "hit_count": hit_total,
        "records": records,
    })
}

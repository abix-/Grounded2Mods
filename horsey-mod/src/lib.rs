//! horsey-mod: native-PE binding of modforge for Horsey Game.
//!
//! Sibling to ueforge (UE5) and unityforge (Unity). Whereas those
//! frameworks ride on a managed plugin loader, horsey-mod injects
//! via the proxy-DLL pattern: the cdylib produced here ships as
//! `steam_api64.dll`, the user renames the real one to
//! `steam_api64_real.dll`, and Windows loads us first because the
//! game asks for `steam_api64.dll` by name.
//!
//! The build script enumerates `steam_api64.dll`'s exports and
//! emits forwarders in a .DEF file so every existing Steam API
//! call goes through to the real DLL transparently.
//!
//! Our `DllMain` runs the moment the DLL is loaded. It spawns a
//! worker thread (NEVER do work in DllMain itself; the loader
//! holds a lock that almost everything in the Win32 API
//! eventually tries to take) which:
//!   1. Initializes modforge's logger.
//!   2. Generates a random auth token, writes it next to the DLL.
//!   3. Registers Horsey-specific ops on modforge's registry.
//!   4. Spawns modforge's HTTP server with auth on 127.0.0.1:33077.
//!
//! Function-hook installation (MinHook trampolines on the 18
//! game functions we identified during decompilation) is staged
//! for a follow-up phase.

pub mod chromosomes;
pub mod fatigue;
pub mod gamestate;
pub mod gene_names;
pub mod genes;
pub mod genes_xml;
pub mod hk1;
pub mod horse;
pub mod input_surface;
pub mod ops;
pub mod overlay;
pub mod patches;
pub mod scene;
pub mod snapshot;
pub mod targets;
pub mod targets_registry;
// `crate::ui` (Phase A C++-shim tab content) was deleted with the
// `ui.native.*` retirement. Step 2 ports the equivalent tab content
// into `crate::overlay` using imgui-rs.

// modforge::server::spawn() pushes its SpawnHandle into a global
// SERVER_REGISTRY and doesn't return it. To stop the server (e.g.
// during a hot-reload), call modforge::server::shutdown_all().

/// One-shot startup. Spawned from a worker thread by DllMain to
/// avoid the DllMain-loader-lock trap.
fn worker_main() {
    // 1. Initialize the modforge logger. It writes to a file in the
    //    same directory as the DLL (resolved via `set_dll_module`
    //    which DllMain called before spawning us).
    modforge::log::init(modforge::log::Config {
        file_name: "horsey.log",
        console_title: "horsey-mod",
        console: false,
    });
    modforge::log!("horsey-mod worker thread started");
    install_panic_hook();
    install_seh_logger();
    modforge::log!("horsey-mod panic + SEH log hooks installed");

    // 2. Resolve the DLL directory for our auth-file write.
    let dll_dir = modforge::log::dll_dir_wait(std::time::Duration::from_secs(5))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    // 3. Auth disabled. The HTTP server binds 127.0.0.1 only, so it
    //    is not reachable off-machine; the per-launch token added no
    //    real security and made every curl invocation friction. If we
    //    ever expose the port off localhost this needs to come back.

    // 4. Register Horsey-specific ops on the modforge global registry.
    ops::register_all();

    // 4b. Register modforge::vanilla cmdlets (vanilla.invoke /
    //     vanilla.list) bound to the horsey target registry. Lets
    //     external clients invoke any registered vanilla function
    //     by name via HTTP.
    targets_registry::register_vanilla_ops();

    // 4c. Register the L3 input surface for Horsey. After this any
    //     `input.*` cmdlet with `backend: "l3"` routes through
    //     `HorseyInputSurface` (writes LOC cursor floats directly,
    //     bypasses the engine's per-frame Win32 input pump).
    input_surface::register();

    // 4a. Arm the in-game ImGui overlay via hudhook. Hooks the
    //     game's IDXGISwapChain::Present so our panel renders
    //     INSIDE the game window. Step 1 spike: draws a single
    //     frame-counter panel. Phase A's separate window is retired
    //     because hudhook bundles its own ImGui via imgui-sys and
    //     cannot link alongside modforge's vendored C++ ImGui shim.
    match overlay::arm() {
        Ok(()) => modforge::log!("horsey-mod: overlay armed"),
        Err(e) => modforge::log!("horsey-mod: overlay arm FAILED: {e}"),
    }

    // 4b. Auto-load `genes-extended.xml` if it exists next to the DLL.
    //     Failure to find or parse the file is non-fatal: the
    //     extended-gene table just stays at defaults and authors
    //     can populate it via HTTP ops or the `genes.ext.reload` op.
    let genes_xml_path = dll_dir.join("genes-extended.xml");
    if genes_xml_path.exists() {
        match crate::genes_xml::load_from_file(&genes_xml_path) {
            Ok(stats) => modforge::log!(
                "horsey-mod: loaded genes-extended.xml -- parsed={} placed={} render={} errors={}",
                stats.parsed,
                stats.placed,
                stats.render_mappings,
                stats.errors.len()
            ),
            Err(e) => modforge::log!(
                "horsey-mod: failed to load {}: {e}",
                genes_xml_path.display()
            ),
        }
    } else {
        modforge::log!(
            "horsey-mod: no genes-extended.xml found at {} (extended-gene table stays at defaults)",
            genes_xml_path.display()
        );
    }

    // 5. Install the sleep_safe_no_tire patch.
    //
    //   The game's `no_tire` cheat (DAT_1403d95c5) drives a per-frame
    //   loop in FUN_1400ceb60 that zeroes BOTH +0x205 and +0x206 on
    //   every horse. +0x206 is the sleep-gate's "is any horse tired"
    //   counter. When it's always 0, the player can't sleep.
    //
    //   This patch NOPs the +0x206 zero, leaving the +0x205 zero in
    //   place. Net effect: enabling no_tire bypasses race-eligibility
    //   fatigue without breaking the day cycle.
    match patches::sleep_safe_no_tire::apply() {
        Ok(()) => {
            // Now we can safely enable the game's own no_tire.
            gamestate::set_no_tire(true);
            modforge::log!("horsey-mod: sleep_safe_no_tire patched + game no_tire enabled");
        }
        Err(e) => {
            modforge::log!(
                "horsey-mod: sleep_safe_no_tire FAILED ({e}); leaving no_tire off so sleep still works"
            );
        }
    }

    // 5. Start the HTTP server. Single endpoint /op, auth via header.
    let server_cfg = modforge::server::Config {
        port: 33077,
        endpoint: "/op",
        thread_name: "horsey-http",
        auth_token: None,
    };
    modforge::server::spawn(
        server_cfg,
        |body: &str| -> Vec<u8> {
            // Parse + dispatch via the modforge op envelope. This
            // mirrors what ueforge / unityforge do.
            let (op, args) = match modforge::envelope::parse_request(body) {
                Ok(v) => v,
                Err(e) => {
                    let resp = modforge::envelope::OpResponse::err(
                        "<parse-error>",
                        e,
                        snapshot::HorseyState::placeholder(),
                    );
                    return serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec());
                }
            };
            let result = modforge::ops::OP_REGISTRY
                .dispatch(&op, &args)
                .unwrap_or_else(|| Err(format!("unknown op: {op}")));
            // Snapshot AFTER dispatch so write ops show the post-write
            // game state in the `state` sub-object. Fixes the cosmetic
            // pre-write-snapshot issue tracked in
            // horsey-mod/docs/todo.md "Known control-plane issues" #1.
            let state = snapshot::HorseyState::capture();
            let resp = modforge::envelope::OpResponse::from_result(&op, result, state);
            serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec())
        },
        |msg| modforge::log!("{msg}"),
    );

    modforge::log!("horsey-mod: listening on 127.0.0.1:33077 (auth disabled)");
}

/// Vectored exception handler. Logs the exception code, fault
/// address, and (for access violations) the address that was being
/// read/written, then returns EXCEPTION_CONTINUE_SEARCH so the
/// game's own crash machinery still produces a WER report.
///
/// Catches SEH (e.g. 0xc0000005 access violation) that the Rust
/// panic hook cannot see. Critical for diagnosing crashes inside
/// our detour handlers or anywhere else our DLL touches game memory.
unsafe extern "system" fn seh_logger(
    info: *mut windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
) -> i32 {
    // SAFETY: Windows guarantees `info` and `info.ExceptionRecord`
    // are valid pointers for the duration of this callback.
    let rec = unsafe { &*(*info).ExceptionRecord };
    let code = rec.ExceptionCode;
    // Only log the codes that matter; ignore noise like C++ EH
    // (0xe06d7363) and DBG_PRINTEXCEPTION_C (0x40010006) which fire
    // routinely in normal Windows process operation.
    const EXCEPTION_ACCESS_VIOLATION: u32 = 0xc000_0005;
    const EXCEPTION_ILLEGAL_INSTRUCTION: u32 = 0xc000_001d;
    const EXCEPTION_STACK_OVERFLOW: u32 = 0xc000_00fd;
    const EXCEPTION_INT_DIVIDE_BY_ZERO: u32 = 0xc000_0094;
    const EXCEPTION_PRIV_INSTRUCTION: u32 = 0xc000_0096;
    let code_u = code as u32;
    let interesting = matches!(
        code_u,
        EXCEPTION_ACCESS_VIOLATION
            | EXCEPTION_ILLEGAL_INSTRUCTION
            | EXCEPTION_STACK_OVERFLOW
            | EXCEPTION_INT_DIVIDE_BY_ZERO
            | EXCEPTION_PRIV_INSTRUCTION
    );
    if !interesting {
        const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let fault_rip = rec.ExceptionAddress as usize;
    let thread = std::thread::current()
        .name()
        .unwrap_or("<unnamed>")
        .to_string();
    // For access violation, ExceptionInformation[0] = 0(read)/1(write)/8(execute)
    // ExceptionInformation[1] = the bad address.
    if code_u == EXCEPTION_ACCESS_VIOLATION {
        let op = rec.ExceptionInformation[0];
        let bad_addr = rec.ExceptionInformation[1];
        let kind = match op {
            0 => "READ",
            1 => "WRITE",
            8 => "EXEC",
            _ => "?",
        };
        modforge::log!(
            "SEH thread={thread:?} code=0xc0000005 ACCESS_VIOLATION \
             kind={kind} bad_addr=0x{bad_addr:x} at rip=0x{fault_rip:x}"
        );
    } else {
        modforge::log!("SEH thread={thread:?} code=0x{code_u:08x} at rip=0x{fault_rip:x}");
    }
    // Best-effort: log the runtime image base so we can compute the
    // RVA inside our DLL by subtracting.
    let our_base = crate::targets::image_base();
    modforge::log!(
        "SEH context: horsey.exe image_base=0x{our_base:x} (rva = rip - dll_base; \
         dll_base != image_base since we're a loaded DLL)"
    );
    // EXCEPTION_CONTINUE_SEARCH: let the next handler (default
    // unhandled-exception filter / WER) deal with the crash. We've
    // logged what we need.
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    EXCEPTION_CONTINUE_SEARCH
}

fn install_seh_logger() {
    use windows_sys::Win32::System::Diagnostics::Debug::AddVectoredExceptionHandler;
    // First parameter = 1 means "call this handler FIRST", before any
    // other vectored handlers and before per-thread SEH frames. We
    // want to log even if the game catches the exception elsewhere.
    // SAFETY: the callback signature matches PVECTORED_EXCEPTION_HANDLER.
    let h = unsafe { AddVectoredExceptionHandler(1, Some(seh_logger)) };
    if h.is_null() {
        modforge::log!("SEH logger: AddVectoredExceptionHandler returned NULL");
    } else {
        modforge::log!("SEH logger: installed (handle 0x{:x})", h as usize);
    }
}

/// Install a Rust panic hook that writes the panic message, location,
/// and backtrace to `horsey.log`. Without this, a panic in any
/// horsey-mod thread (HTTP worker, future detour handlers) under
/// `panic = "abort"` (our release profile) terminates Horsey.exe
/// silently with a 0xc0000409 status code and no log breadcrumb.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let payload = if let Some(s) = info.payload().downcast_ref::<&'static str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "<non-string panic payload>".to_string()
        };
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        modforge::log!("PANIC thread={thread:?} at {location}: {payload}");
        modforge::log!(
            "PANIC backtrace:\n{}",
            std::backtrace::Backtrace::force_capture()
        );
    }));
}

// =============================================================================
// DllMain
// =============================================================================

use windows_sys::Win32::Foundation::HINSTANCE;

/// Windows BOOL: `i32` underneath. windows-sys 0.61 doesn't
/// re-export the alias.
type BOOL = i32;

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_THREAD_ATTACH: u32 = 2;
const DLL_THREAD_DETACH: u32 = 3;
const DLL_PROCESS_DETACH: u32 = 0;

/// PE entry. The MSVC linker recognizes the literal name `DllMain`
/// and generates the CRT bridge automatically.
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(
    hinst: HINSTANCE,
    reason: u32,
    _reserved: *mut std::ffi::c_void,
) -> BOOL {
    match reason {
        DLL_PROCESS_ATTACH => {
            // Hand the HMODULE to modforge's logger so dll_dir()
            // can resolve our path. This call only touches loaded-DLL
            // metadata; safe inside DllMain.
            modforge::log::set_dll_module(hinst as _);
            // Spawn the worker. NEVER block here.
            std::thread::spawn(worker_main);
        }
        DLL_PROCESS_DETACH => {
            // Stop every server modforge has running so the listener
            // thread doesn't keep our DLL pinned past unload.
            modforge::server::shutdown_all();
            // Restore the bytes we overwrote in Horsey.exe so the
            // game runs vanilla again after we detach. CRITICAL.
            // without this, a hot-reload would leave NOPs in the
            // exe and the next-gen DLL would re-patch on top of
            // already-NOP'd code.
            patches::revert_all();
            // Best-effort: also turn off no_tire so the game's own
            // per-frame loop (with our NOPs reverted) doesn't keep
            // zeroing the tired flags.
            gamestate::set_no_tire(false);
        }
        DLL_THREAD_ATTACH | DLL_THREAD_DETACH => {}
        _ => {}
    }
    1 // TRUE
}

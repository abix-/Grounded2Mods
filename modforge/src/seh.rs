//! SEH guard: run a closure inside an MSVC `__try`/`__except` block
//! so a structured exception (access violation, illegal instruction,
//! stack overflow) inside foreign code is CAUGHT and reported instead
//! of killing the host process.
//!
//! Use this around every call into vanilla / detoured / vtable-derived
//! game functions. Without it, a single bad pointer in any one of those
//! calls takes the entire host process down. With it, the call returns
//! `Err(SehError)` and the host stays alive, ready to retry with
//! different inputs.
//!
//! The actual `__try`/`__except` block lives in `src/seh.c` (MSVC C
//! extension). On non-MSVC targets, [`guard`] degrades to a direct
//! call (no protection). The `cfg` gate is `target_env = "msvc"`.
//!
//! # Example
//!
//! ```ignore
//! use modforge::seh;
//!
//! type DropCommitFn = unsafe extern "system" fn(*mut u8) -> u8;
//! let f: DropCommitFn = unsafe { core::mem::transmute(fn_addr) };
//!
//! match seh::guard(|| unsafe { f(loc as *mut u8) }) {
//!     Ok(result) => log::info!("dropped, result={result}"),
//!     Err(e)     => log::warn!("drop crashed: {e:?}"),
//! }
//! ```

use std::cell::Cell;
use std::ffi::c_void;

/// Information captured from a caught structured exception.
#[derive(Debug, Clone, Copy)]
pub struct SehError {
    /// Win32 EXCEPTION_RECORD::ExceptionCode (e.g. 0xC0000005 = AV).
    pub code: u32,
    /// Faulting instruction address.
    pub fault_address: usize,
}

impl SehError {
    pub fn is_access_violation(&self) -> bool {
        self.code == 0xc000_0005
    }
    pub fn is_illegal_instruction(&self) -> bool {
        self.code == 0xc000_001d
    }
}

impl std::fmt::Display for SehError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.code {
            0xc000_0005 => "ACCESS_VIOLATION",
            0xc000_001d => "ILLEGAL_INSTRUCTION",
            0xc000_0094 => "INT_DIVIDE_BY_ZERO",
            0xc000_0096 => "PRIV_INSTRUCTION",
            0xc000_00fd => "STACK_OVERFLOW",
            _ => "UNKNOWN",
        };
        write!(
            f,
            "SEH {name} (code=0x{:08x}) at rip=0x{:x}",
            self.code, self.fault_address
        )
    }
}

impl std::error::Error for SehError {}

#[cfg(all(windows, target_env = "msvc"))]
unsafe extern "C" {
    fn modforge_seh_guard(
        thunk: unsafe extern "C" fn(*mut c_void),
        ctx: *mut c_void,
        out_code: *mut u32,
        out_addr: *mut usize,
    ) -> i32;
}

/// Per-call payload passed through the C shim.
struct Payload<F: FnOnce() -> R, R> {
    f: Option<F>,
    result: Option<R>,
}

unsafe extern "C" fn thunk_trampoline<F: FnOnce() -> R, R>(ctx: *mut c_void) {
    // SAFETY: ctx was just a &mut Payload<F, R>; we cast back.
    let payload = unsafe { &mut *(ctx as *mut Payload<F, R>) };
    if let Some(f) = payload.f.take() {
        payload.result = Some(f());
    }
}

thread_local! {
    /// Set while we're inside a `guard`. The vectored handler in
    /// `horsey-mod::lib::install_seh_logger` reads this to decide
    /// whether to log a verbose entry. (Logging from a vectored
    /// handler while we're recovering can cause re-entry storms.)
    static INSIDE_GUARD: Cell<bool> = const { Cell::new(false) };
}

/// Returns true while the current thread is executing inside a
/// [`guard`] call. Vectored exception handlers can use this to
/// suppress noisy logging when they know recovery is imminent.
pub fn currently_guarding() -> bool {
    INSIDE_GUARD.with(|c| c.get())
}

/// Run `f` inside a `__try`/`__except` block. Returns `Ok(result)` if
/// `f` finished normally, or `Err(SehError)` if a structured exception
/// was raised (access violation, illegal instruction, etc.).
///
/// On non-MSVC targets this just calls `f()` directly.
#[cfg(all(windows, target_env = "msvc"))]
pub fn guard<F, R>(f: F) -> Result<R, SehError>
where
    F: FnOnce() -> R,
{
    let mut payload: Payload<F, R> = Payload {
        f: Some(f),
        result: None,
    };
    let mut code: u32 = 0;
    let mut addr: usize = 0;
    INSIDE_GUARD.with(|c| c.set(true));
    // SAFETY: thunk_trampoline matches the expected C signature
    // (`void (*)(void*)`). payload outlives the call; the thunk only
    // touches it through the same pointer we hand to the C shim.
    let raised = unsafe {
        modforge_seh_guard(
            thunk_trampoline::<F, R>,
            &mut payload as *mut _ as *mut c_void,
            &mut code,
            &mut addr,
        )
    };
    INSIDE_GUARD.with(|c| c.set(false));
    if raised == 0 {
        Ok(payload.result.expect("thunk succeeded but result missing"))
    } else {
        Err(SehError {
            code,
            fault_address: addr,
        })
    }
}

#[cfg(not(all(windows, target_env = "msvc")))]
pub fn guard<F, R>(f: F) -> Result<R, SehError>
where
    F: FnOnce() -> R,
{
    Ok(f())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_success_returns_value() {
        let r = guard(|| 42i32);
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn guard_success_runs_side_effects() {
        let mut buf = String::new();
        let r = guard(|| {
            buf.push_str("hello");
            buf.len()
        });
        assert_eq!(r.unwrap(), 5);
        assert_eq!(buf, "hello");
    }

    #[cfg(all(windows, target_env = "msvc"))]
    #[test]
    fn guard_catches_access_violation() {
        // Read a u8 from a non-mapped address. u8 has no alignment
        // requirement so we don't trip the read_volatile precondition.
        let bad: *const u8 = 0xdead_beef as *const u8;
        let r = guard(|| unsafe { bad.read_volatile() });
        let e = r.expect_err("should have caught AV");
        assert!(e.is_access_violation(), "got {e}");
    }

    #[cfg(all(windows, target_env = "msvc"))]
    #[test]
    fn currently_guarding_is_set_inside() {
        assert!(!currently_guarding());
        let _ = guard(|| {
            assert!(currently_guarding());
        });
        assert!(!currently_guarding());
    }
}

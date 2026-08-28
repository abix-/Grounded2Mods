//! Win32 worker-thread spawn with a panic guard and a thread name.
//!
//! Every mod ends up doing this: spawn a worker off the engine init
//! thread (so a panic in our init doesn't propagate to UE4SS),
//! catch_unwind the body, name the thread for crash dumps. Hand-
//! rolling it has produced two real bugs in this workspace:
//!   - silent panic -> dead worker -> no signal to the user
//!   - unnamed thread -> crash dumps showed `Thread N: ?? @ 0x...`
//!
//! ```ignore
//! ueforge::worker::spawn("grounded2_mod/init", || {
//!     // ... heavy init: settings load, GObjects walks, hook installs ...
//! });
//! ```
//!
//! Backed by `std::thread::Builder::name(...)`. Which on Windows
//! sets the thread description visible to debuggers.

/// Spawn a named worker thread. Panics inside `work` are caught and
/// logged; the thread exits cleanly without taking the host process
/// down.
pub fn spawn<F>(name: &'static str, work: F)
where
    F: FnOnce() + Send + 'static,
{
    let result = std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
            if let Err(payload) = r {
                let msg = panic_msg(&payload);
                crate::log!("worker [{name}]: panicked: {msg}");
            }
        });
    if let Err(e) = result {
        crate::log!("worker [{name}]: spawn failed: {e}");
    }
}

fn panic_msg(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "<non-string panic payload>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn body_runs() {
        let flag = Arc::new(AtomicBool::new(false));
        let f2 = flag.clone();
        spawn("test/body_runs", move || {
            f2.store(true, Ordering::SeqCst);
        });
        // Poll briefly; thread::Builder::spawn may schedule late.
        for _ in 0..50 {
            if flag.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("worker body did not run within 500ms");
    }

    #[test]
    fn panic_is_caught() {
        // We can't reliably observe the log line in unit tests, so
        // just confirm the call returns and doesn't unwind into us.
        spawn("test/panics", || panic!("intentional"));
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

//! Windows process helpers: tasklist polling + taskkill.

use std::process::Command;
use std::time::{Duration, Instant};

/// Is a process by name currently running?
pub fn is_running(process_name: &str) -> bool {
    let out = Command::new("tasklist")
        .args([
            "/fi",
            &format!("imagename eq {process_name}"),
            "/nh",
            "/fo",
            "csv",
        ])
        .output();
    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains(process_name)
        }
        Err(_) => false,
    }
}

/// Wait until a process appears in `tasklist`. Returns `Ok(())` when
/// found within `timeout`, `Err(...)` if it never shows up.
pub fn wait_for_start(process_name: &str, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if is_running(process_name) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!(
        "process {process_name} did not start within {}s",
        timeout.as_secs()
    )
}

/// Wait until a process is gone from `tasklist`.
pub fn wait_for_exit(process_name: &str, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_running(process_name) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!(
        "process {process_name} did not exit within {}s",
        timeout.as_secs()
    )
}

/// Force-kill a process by name. Returns Ok even if the process
/// wasn't running. Safe to call before launching to ensure a clean
/// slate.
pub fn kill(process_name: &str) -> anyhow::Result<()> {
    let _ = Command::new("taskkill")
        .args(["/f", "/im", process_name])
        .output()?;
    // `taskkill` exits non-zero if the process isn't running; we
    // don't care, the post-condition we want is "process is gone".
    Ok(())
}

/// Kill a process and wait until it's actually gone.
pub fn kill_and_wait(process_name: &str, timeout: Duration) -> anyhow::Result<()> {
    kill(process_name)?;
    // If the process was never running, wait_for_exit returns immediately.
    wait_for_exit(process_name, timeout)
}

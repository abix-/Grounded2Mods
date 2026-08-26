//! Run the per-game injector binary after the game process is up.

use super::InjectorDef;
use std::process::Command;

/// Invoke the injector binary. Inherits stdio so the test runner
/// shows the injector's progress. Bails on non-zero exit.
pub fn run(spec: &InjectorDef) -> anyhow::Result<()> {
    if !spec.injector_exe.is_file() {
        anyhow::bail!(
            "injector exe not found at {}",
            spec.injector_exe.display()
        );
    }
    let mut cmd = Command::new(&spec.injector_exe);
    cmd.args(&spec.args);
    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!(
            "injector {} failed with status {status}",
            spec.injector_exe.display()
        );
    }
    std::thread::sleep(spec.post_inject_settle);
    Ok(())
}

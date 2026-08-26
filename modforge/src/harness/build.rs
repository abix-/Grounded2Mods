//! Wrap `cargo build -p <package>` for the autobuild step.

use super::BuildDef;
use std::process::Command;

/// Run `cargo build -p <package> [--release]` from
/// `spec.workspace_dir`. Stdout/stderr are inherited so the test
/// runner shows compile errors. Bails on non-zero exit.
pub fn run(spec: &BuildDef) -> anyhow::Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&spec.workspace_dir);
    cmd.args(["build", "-p", spec.package]);
    if spec.release {
        cmd.arg("--release");
    }
    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!(
            "cargo build -p {} {} failed with status {status}",
            spec.package,
            if spec.release { "--release" } else { "" }
        );
    }
    Ok(())
}

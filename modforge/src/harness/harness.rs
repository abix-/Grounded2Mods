//! `GameHarness` + `RunningGame`: the composite "launch and own a
//! game process" type that tests use.

use super::log::TestLog;
use super::{
    GameDef, build, http_probe, injector, process, should_skip_build, steam,
};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

pub struct GameHarness;

impl GameHarness {
    /// Full launch cycle: build (optional), kill any running
    /// instance, Steam launch, wait for process, run injector
    /// (optional), poll HTTP plane until ready. Returns a
    /// `RunningGame` whose Drop tears down the process. Writes a
    /// timestamped log to `target/test-runs/<test_name>-<ts>.log`.
    pub fn launch(spec: &GameDef, test_name: &str) -> anyhow::Result<RunningGame> {
        let log = TestLog::open(&workspace_root(spec), test_name)?;
        log.event("SPEC", &format!("{spec:?}"));

        if let Some(b) = &spec.build {
            if !should_skip_build() {
                log.event("BUILD", &format!("cargo build -p {} (release={})", b.package, b.release));
                let t = Instant::now();
                build::run(b)?;
                log.event("BUILD", &format!("done in {:.1}s", t.elapsed().as_secs_f32()));
            } else {
                log.event("BUILD", "skipped (MODFORGE_SKIP_BUILD=1)");
            }
        }

        log.event("KILL", &format!("kill any running {}", spec.process_name));
        process::kill_and_wait(spec.process_name, Duration::from_secs(30))?;
        log.event("KILL", "process gone (or never running)");

        log.event("STEAM", &format!("launching appid {}", spec.app_id));
        steam::launch(spec.app_id)?;

        log.event("WAIT", &format!("waiting for {} to appear (timeout {}s)", spec.process_name, spec.launch_timeout.as_secs()));
        let t = Instant::now();
        process::wait_for_start(spec.process_name, spec.launch_timeout)?;
        log.event("WAIT", &format!("{} appeared in {:.1}s", spec.process_name, t.elapsed().as_secs_f32()));

        if let Some(inj) = &spec.injector {
            log.event("INJECT", &format!("running {} {:?}", inj.injector_exe.display(), inj.args));
            let t = Instant::now();
            injector::run(inj)?;
            log.event("INJECT", &format!("injector exited cleanly in {:.1}s", t.elapsed().as_secs_f32()));
        } else {
            log.event("INJECT", "skipped (no injector configured)");
        }

        log.event("HTTP", &format!("waiting for HTTP plane on port {} (timeout {}s)", spec.http.port, spec.launch_timeout.as_secs()));
        let t = Instant::now();
        http_probe::wait_for_ready(&spec.http, spec.launch_timeout)?;
        log.event("HTTP", &format!("ping ok in {:.1}s", t.elapsed().as_secs_f32()));

        log.event("READY", "game running, mod injected, HTTP ready");
        Ok(RunningGame { spec: spec.clone(), log })
    }

    /// Skip launch; assume the game is already running and the mod
    /// is injected. Verify HTTP plane and hand back a `RunningGame`
    /// whose Drop is a no-op (so we don't kill a session the user
    /// wanted to keep).
    pub fn attach_existing(spec: &GameDef, test_name: &str) -> anyhow::Result<RunningGame> {
        let log = TestLog::open(&workspace_root(spec), test_name)?;
        log.event("ATTACH", &format!("verifying HTTP plane on port {}", spec.http.port));
        http_probe::wait_for_ready(&spec.http, Duration::from_secs(5))?;
        log.event("ATTACH", "HTTP ready; attached without launch");
        Ok(RunningGame {
            spec: GameDef {
                process_name: "",
                ..spec.clone()
            },
            log,
        })
    }
}

fn workspace_root(spec: &GameDef) -> std::path::PathBuf {
    spec.build
        .as_ref()
        .map(|b| b.workspace_dir.clone())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()))
}

/// Live, modded game instance. Drop kills the process.
pub struct RunningGame {
    spec: GameDef,
    log: TestLog,
}

impl RunningGame {
    /// POST an op, return raw response body. Logs the call + result.
    pub fn op_raw(&self, op: &str, body: &str) -> anyhow::Result<String> {
        let t = Instant::now();
        let result = http_probe::post(&self.spec.http, op, body);
        match result {
            Ok(resp) => {
                self.log.event(
                    "OP",
                    &format!(
                        "{op} -> ok in {:.0}ms ({} bytes)",
                        t.elapsed().as_secs_f32() * 1000.0,
                        resp.len()
                    ),
                );
                Ok(resp)
            }
            Err(e) => {
                let msg = format!("{op} -> ERR in {:.0}ms: {e}", t.elapsed().as_secs_f32() * 1000.0);
                self.log.event("OP", &msg);
                Err(anyhow::anyhow!(msg))
            }
        }
    }

    /// POST an op with a JSON body, parse response as JSON.
    pub fn op_json(&self, op: &str, body: &Value) -> anyhow::Result<Value> {
        let body_str = body.to_string();
        let resp = self.op_raw(op, &body_str)?;
        let v: Value = serde_json::from_str(&resp)
            .map_err(|e| anyhow::anyhow!("op {op} returned non-JSON ({e}): {resp}"))?;
        Ok(v)
    }

    pub fn spec(&self) -> &GameDef {
        &self.spec
    }

    pub fn log(&self) -> &TestLog {
        &self.log
    }

    /// Record a test PASS in the log.
    pub fn pass(&self, msg: &str) {
        self.log.result("pass", msg);
    }

    /// Record a test FAIL in the log.
    pub fn fail(&self, msg: &str) {
        self.log.result("fail", msg);
    }

    /// Graceful shutdown; Drop does the actual work.
    pub fn stop(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl Drop for RunningGame {
    fn drop(&mut self) {
        if self.spec.process_name.is_empty() {
            self.log.event("TEARDOWN", "attached session; no kill");
            return;
        }
        self.log.event(
            "TEARDOWN",
            &format!(
                "grace {}s, then taskkill {}",
                self.spec.shutdown_grace.as_secs(),
                self.spec.process_name
            ),
        );
        std::thread::sleep(self.spec.shutdown_grace);
        let _ = process::kill_and_wait(
            self.spec.process_name,
            Duration::from_secs(30),
        );
        self.log.event("TEARDOWN", "done");
    }
}

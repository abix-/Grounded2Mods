//! HTTP control plane. Same shape as outworld-station-mod's, and
//! starts just as empty: ops and snapshot fields grow as the
//! emission research lands.

use serde::Serialize;

use ueforge::envelope::handle_request;

/// Register every misery-mod op + selector into the workspace
/// registries. Called once before the HTTP listener starts.
fn register_ops() {
    ueforge::selector::register_builtins();
    ueforge::ops::register_builtins();
    ueforge::ops::register_with_resolver(ueforge::selector::resolve);
}

pub fn spawn(port: u16) {
    register_ops();
    ueforge::spawn(
        ueforge::Config {
            port,
            endpoint: "/debug",
            thread_name: "misery-mod-debug",
            auth_token: None,
        },
        |body| {
            let resp = handle_request(body, &ueforge::ops::OP_REGISTRY, build_snapshot);
            serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec())
        },
        |msg| ueforge::log::log(format_args!("{msg}")),
    );
}

/// Game-specific snapshot fields. `offsets_known` is the one that
/// matters right now: until the platform offsets are filled in,
/// every object-walking op will fail and this says why.
#[derive(Serialize, Default)]
pub struct Snapshot {
    pub offsets_known: bool,
}

fn build_snapshot() -> Snapshot {
    Snapshot {
        offsets_known: ueforge::ue::try_runtime().is_some(),
    }
}

//! HTTP control plane. Mirrors grounded2-mod's pattern but
//! starts empty. Fields and ops grow as research lands.

use serde::Serialize;

use ueforge::envelope::handle_request;

/// Register every ows-tweaks op + selector into the workspace
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
            thread_name: "ows-tweaks-debug",
            auth_token: None,
        },
        |body| {
            let resp = handle_request(body, &ueforge::ops::OP_REGISTRY, build_snapshot);
            serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec())
        },
        |msg| ueforge::log::log(format_args!("{msg}")),
    );
}

/// Game-specific snapshot fields. Empty for now; tests grow this
/// as they need observables.
#[derive(Serialize, Default)]
pub struct Snapshot {
    pub offsets_known: bool,
}

fn build_snapshot() -> Snapshot {
    Snapshot {
        offsets_known: ueforge::ue::try_runtime().is_some(),
    }
}


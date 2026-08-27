/// Starts the local MISERY debug server used to inspect and control the running mod.
/// Stays here because the endpoint name and process startup belong to this mod; Ueforge owns the server.
pub fn spawn(port: u16) {
    ueforge::start_debug_server(ueforge::Config {
        port,
        endpoint: "/debug",
        thread_name: "misery-mod-debug",
        auth_token: None,
    });
}

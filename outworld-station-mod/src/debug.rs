pub fn spawn(port: u16) {
    ueforge::start_debug_server(ueforge::Config {
        port,
        endpoint: "/debug",
        thread_name: "ows-tweaks-debug",
        auth_token: None,
    });
}

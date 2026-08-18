pub fn spawn(port: u16) {
    ueforge::start_debug_server(ueforge::Config {
        port,
        endpoint: "/debug",
        thread_name: "misery-mod-debug",
        auth_token: None,
    });
}

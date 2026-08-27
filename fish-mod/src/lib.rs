use unityforge::ModDef;

static MOD_INFO: ModDef = ModDef {
    name: "FishMod",
    version: "0.1.0",
    http_port: 17174,
    on_init: Some(on_init),
    on_tick: None,
    on_shutdown: None,
    tabs: &[],
};

unityforge::unityforge_mod!(MOD_INFO);

fn on_init() {
    unityforge::ops::register_builtins();
    unityforge::selector::register_builtins();
    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        "fish-mod: ready (ops + selectors installed)",
    );
}

//! Research: read S_GameplaySettings values from the live
//! BP_GlobalManager_C instance.
//!
//! GameplaySettings is an embedded struct at +0x218 on the
//! GlobalManager. Field offsets within the struct are from the
//! UE4SS object dump.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_gameplay_settings -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;

#[test]
fn read_gameplay_settings_live() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let Some(inst) = client::find_live_instance(&api, "BP_GlobalManager_C") else {
        println!("no live BP_GlobalManager_C");
        return;
    };
    let addr = inst.addr;

    let fields: &[(&str, u64, &str)] = &[
        ("ShiningsTimer",           0x218 + 0x00, "f64"),
        ("HungerSpeed",             0x218 + 0x28, "f64"),
        ("ThirstSpeed",             0x218 + 0x30, "f64"),
        ("StaminaDrainRate",        0x218 + 0x38, "f64"),
        ("HeadshotDamageMultiplier", 0x218 + 0x40, "f64"),
        ("DamageMultiplier",        0x218 + 0x48, "f64"),
        ("EnemySpawnRate",          0x218 + 0x68, "f64"),
        ("EnemyDamageToPlayer",     0x218 + 0x70, "f64"),
        ("EnemySpeed",              0x218 + 0x78, "f64"),
        ("RespawnHealthMultiplier", 0x218 + 0xA8, "f64"),
        ("WeightLimitMultiplier",   0x218 + 0xB0, "f64"),
        ("RespawnOnEmission",       0x218 + 0xBB, "bool"),
    ];
    for (name, offset, ty) in fields {
        match *ty {
            "f64" => {
                let val = client::read_f64(&api, addr, *offset);
                println!("{name} @ +0x{offset:x}: {val}");
            }
            "bool" => {
                let val = client::read_u8(&api, addr, *offset) != 0;
                println!("{name} @ +0x{offset:x}: {val}");
            }
            _ => {}
        }
    }
}

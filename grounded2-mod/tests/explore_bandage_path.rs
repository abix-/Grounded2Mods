// Exploratory research: does AddHealth bypass the
// impact_resistance mask in practice, or only in our isolated
// simulate path?
//
// The user reported bandages don't heal when impact_resistance
// is enabled. Earlier rounds proved:
//   - status_effects array unchanged when bandage used (no HoT
//     row added on either mask-on or mask-off case)
//   - damage_ring empty during bandage on either case
//   - HP DOES grow with mask off (~57 over HoT)
//   - HP does NOT grow with mask on
// So the heal path is invisible to our current trampoline.
// Hypothesis: bandages call UHealthComponent::AddHealth (or
// ServerAddHealth) directly. AddHealth parm layout has been
// captured in the SDK; we call it via the generic `call`
// primitive and compare HP delta on/off.
//
// All test logic is here. The mod stays generic.
//
// Run:
//   set BBP_DEBUG_PORT=17171
//   cargo test --release --target x86_64-pc-windows-msvc \
//     --test explore_bandage_path. --nocapture --test-threads=1

mod common;

/// SDK: `Maine.HealthComponent.AddHealth(float Amount, UBaseLODActor* Causer)`
/// 16-byte parm buffer. Causer is a pointer; we pass null since
/// AddHealth tolerates a null causer (it's just for kill-attribution
/// in the receiver path).
#[repr(C)]
#[derive(
    Default,
    Copy,
    Clone,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::Immutable,
    zerocopy::KnownLayout,
)]
struct AddHealthParms {
    amount: f32,
    _pad: [u8; 4],
    causer: u64,
}

fn call_add_health(api: &common::Api, amount: f32) -> Result<common::Snapshot, String> {
    let parms = AddHealthParms {
        amount,
        _pad: [0; 4],
        causer: 0,
    };
    let bytes = common::parms_as_bytes(&parms);
    let (_after, state) =
        api.call_ufunction("HealthComponent", "AddHealth", "live_player_hc", bytes)?;
    Ok(state)
}

#[test]
fn addhealth_with_mask_on_vs_off() {
    let api = common::Api::require();

    let baseline = api.snapshot();
    if baseline.live_player.is_none() {
        panic!("no live player; load a save and spawn into the world");
    }
    if baseline.live_hc().max_health - baseline.live_hc().current_damage
        > baseline.live_hc().max_health - 25.0
    {
        eprintln!(
            "WARNING: player near full HP (current_damage={}); take some damage first to leave headroom for the heal to be observable",
            baseline.live_hc().current_damage
        );
    }

    eprintln!("\n========================================");
    eprintln!("=== AddHealth(20) with impact_resistance OFF ===");
    eprintln!("========================================");
    api.skill_toggle("impact_resistance", false);
    let pre_off = api.snapshot();
    let mask_off = pre_off.live_hc().required_damage_type_flags.clone();
    let cd_off_pre = pre_off.live_hc().current_damage;

    let post_off = call_add_health(&api, 20.0).expect("AddHealth(off) failed");
    let cd_off_post = post_off.live_hc().current_damage;
    let delta_off = cd_off_pre - cd_off_post;
    eprintln!("  mask:           {}", mask_off);
    eprintln!(
        "  CurrentDamage:  {} -> {}  (delta = {} healed)",
        cd_off_pre, cd_off_post, delta_off
    );
    eprintln!(
        "  damage_ring entries during call: {}",
        post_off.damage_ring.len() - pre_off.damage_ring.len()
    );
    for ev in &post_off.damage_ring[pre_off.damage_ring.len()..] {
        eprintln!(
            "    fn={} damage={} flags=0x{:08x} type_flags=0x{:08x}",
            ev.function, ev.damage, ev.damage_flags, ev.type_flags
        );
    }

    eprintln!("\n========================================");
    eprintln!("=== AddHealth(20) with impact_resistance ON ===");
    eprintln!("========================================");
    api.skill_toggle("impact_resistance", true);
    let pre_on = api.snapshot();
    let mask_on = pre_on.live_hc().required_damage_type_flags.clone();
    let cd_on_pre = pre_on.live_hc().current_damage;

    let post_on = call_add_health(&api, 20.0).expect("AddHealth(on) failed");
    let cd_on_post = post_on.live_hc().current_damage;
    let delta_on = cd_on_pre - cd_on_post;
    eprintln!("  mask:           {}", mask_on);
    eprintln!(
        "  CurrentDamage:  {} -> {}  (delta = {} healed)",
        cd_on_pre, cd_on_post, delta_on
    );
    eprintln!(
        "  damage_ring entries during call: {}",
        post_on.damage_ring.len() - pre_on.damage_ring.len()
    );
    for ev in &post_on.damage_ring[pre_on.damage_ring.len()..] {
        eprintln!(
            "    fn={} damage={} flags=0x{:08x} type_flags=0x{:08x}",
            ev.function, ev.damage, ev.damage_flags, ev.type_flags
        );
    }

    eprintln!("\n========================================");
    eprintln!("=== VERDICT ===");
    eprintln!("========================================");
    eprintln!("  off delta: {}", delta_off);
    eprintln!("  on  delta: {}", delta_on);
    if (delta_off - delta_on).abs() < 0.5 {
        eprintln!("  AddHealth behaves IDENTICALLY with mask on vs off.");
        eprintln!("  Bandages must use a different path (not AddHealth directly),");
        eprintln!("  OR there is a mod state that changes between the two snapshots");
        eprintln!("  that gates the bandage upstream of AddHealth.");
    } else if delta_on < 1.0 && delta_off > 1.0 {
        eprintln!("  AddHealth IS BLOCKED with mask on. The mask intercepts");
        eprintln!("  AddHealth (contradicting the earlier simulate_add_health");
        eprintln!("  result -- worth re-running simulate_add_health to confirm");
        eprintln!("  consistency).");
    } else {
        eprintln!("  Asymmetric or partial heal -- not a clean blocked/unblocked.");
        eprintln!("  Possibly clamping at MaxHealth, or the mask scales rather");
        eprintln!("  than blocks. Inspect raw values above.");
    }

    // Hard assertion: regardless of HOW the off path behaves, with
    // mask off AddHealth(20) should heal noticeably (not zero).
    assert!(
        delta_off > 1.0,
        "AddHealth with mask OFF did not heal; expected delta > 1.0, got {}",
        delta_off
    );
}

/// Watch for what bandages actually fire on the player HC.
/// Interactive: user uses a bandage in-game between baseline and
/// observation snapshots; the test prints the diff in damage_ring
/// (which now captures EVERY UFunction call on the player HC, not
/// just damage-shaped ones).
#[test]
#[ignore] // run manually with `--ignored` after using a bandage
fn observe_bandage_path() {
    let api = common::Api::require();
    let s = api.snapshot();
    let hc = s.live_hc();
    eprintln!(
        "HP: {:.2} / {:.2}  mask: {}  ring entries: {}",
        hc.max_health - hc.current_damage,
        hc.max_health,
        hc.required_damage_type_flags,
        s.damage_ring.len()
    );
    eprintln!("\nLast 30 entries in damage_ring:");
    for ev in s
        .damage_ring
        .iter()
        .rev()
        .take(30)
        .collect::<Vec<_>>()
        .iter()
        .rev()
    {
        eprintln!(
            "  t={}  fn={}  dmg={}  flags=0x{:08x}  tflags=0x{:08x}  cd_before={:?}",
            ev.at_secs,
            ev.function,
            ev.damage,
            ev.damage_flags,
            ev.type_flags,
            ev.current_damage_before
        );
    }
}

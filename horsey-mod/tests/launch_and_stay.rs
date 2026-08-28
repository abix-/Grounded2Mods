//! Launch Horsey + inject the current horsey.dll + wait for a save to
//! load, then exit the test process WITHOUT killing the game. The
//! game keeps running so the operator can interact with the overlay
//! (e.g. click HK1 transfer buttons) and let results accumulate in
//! `<dll_dir>/hk1_overlay.log`.
//!
//! The harness's `RunningGame::drop` taskkills the game. We bypass
//! that by calling `std::process::exit(0)` after a successful save
//! gate; the destructor doesn't fire.

mod common;

#[test]
fn launch_and_stay() {
    let Some(game) = common::launch("launch_and_stay") else {
        return;
    };
    eprintln!("[GATE] waiting for any owned horse to appear (up to 180s) - load your save");
    let h = common::wait_for_target_horse(&game, std::time::Duration::from_secs(180));
    eprintln!("[READY] save loaded; horse '{}' at {}", h.name, h.ptr_s);
    eprintln!("[STAYING] game left running - close it yourself when done");
    eprintln!("[LOG] overlay HK1 actions append to <dll_dir>/hk1_overlay.log");
    // Skip RunningGame::drop so the harness doesn't taskkill the game.
    std::process::exit(0);
}

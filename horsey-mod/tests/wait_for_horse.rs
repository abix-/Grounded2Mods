//! GENERIC: block until the target horse appears in the owned list.
//!
//! `HORSEY_HORSE=<name>` -> wait for that specific horse.
//! unset           -> wait until any horse is owned.
//!
//! Pre-test gate for any other live test that needs a loaded save.

mod common;

#[test]
fn wait_for_horse() {
    let Some(game) = common::launch("wait_for_horse") else {
        return;
    };
    let h = common::wait_for_target_horse(&game, std::time::Duration::from_secs(120));
    eprintln!("[READY] horse '{}' at {} (id {})", h.name, h.ptr_s, h.id);
}

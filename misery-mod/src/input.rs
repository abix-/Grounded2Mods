//! MISERY connection to Ueforge player input.

pub fn register() {
    ueforge::input::register("misery", &crate::speed::PLAYER);
}

//! Is the mod alive inside The Boss Gangsters Nightlife?
//!
//! First contact: BepInEx loads Unityforge.Shim.Mono, the shim
//! loads bossgangsters_mod.unityforge.dll, and the control
//! plane answers ping on port 17176.
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_ping -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};

#[test]
fn control_plane_answers_ping() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    println!("control plane answered ping on port 17176");
}

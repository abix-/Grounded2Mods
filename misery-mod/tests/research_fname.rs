//! Can we turn a string into an `FName`?
//!
//! Everything else the framework does with names READS them off
//! something that already exists: a class, an actor, an asset
//! entry. Nothing built one from a string we chose, which put
//! every engine call taking a name we pick out of reach
//! (research.md 28). It blocked reading the asset registry's
//! cooked tags: `AssetRegistryHelpers::GetTagValue` is right
//! there and callable, and its tag-name argument could not be
//! supplied.
//!
//! The engine has its own constructor,
//! `FName::FName(wchar_t const*, EFindName)`, and patternsleuth
//! already ships a resolver for it (`FNameCtorWchar`). This
//! proves calling it works.
//!
//! **Find, not Add.** A name the game cooked in already exists,
//! so finding it is enough. Adding one would turn "this build has
//! no such name" into a silent yes, which is the answer we most
//! need to trust.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_fname -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

/// A name the game certainly has, because the engine itself uses
/// it, round-tripped back to the same text.
#[test]
fn a_name_the_game_has_comes_back() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("string_to_fname", json!({ "text": "StaticMesh" }));
    assert!(r.ok, "string_to_fname failed: {:?}", r.error);
    println!(
        "{}",
        serde_json::to_string_pretty(&r.result).unwrap_or_default()
    );

    assert_eq!(
        r.result["found"],
        json!(true),
        "the engine has no name 'StaticMesh'?"
    );
    // The round trip is the proof. A non-zero FName only says
    // something came back; the same TEXT says it is the right one.
    assert_eq!(
        r.result["round_trip"],
        json!("StaticMesh"),
        "the name did not survive the round trip"
    );
}

/// A name nothing could have cooked in comes back MISSING, not
/// invented.
///
/// This is the half that matters. If find-mode silently added
/// names, every question of the form "does this build have X"
/// would answer yes.
#[test]
fn a_name_the_game_lacks_stays_missing() {
    let Some(api) = api_or_skip() else { return };
    let text = "ThisNameCannotExist_ModforgeResearch_9f3a";
    let r = api.op("string_to_fname", json!({ "text": text }));
    assert!(r.ok, "string_to_fname failed: {:?}", r.error);
    println!(
        "{}",
        serde_json::to_string_pretty(&r.result).unwrap_or_default()
    );
    assert_eq!(
        r.result["found"],
        json!(false),
        "find-mode invented a name that should not exist"
    );
}

/// The names the asset registry's tags are keyed by.
///
/// Which of these the game has tells us what `GetTagValue` can be
/// asked for, and whether a static mesh's SIZE is among them.
/// That decides whether the parts list needs to load 1,500 meshes
/// or none (parts.md).
#[test]
fn which_asset_tag_names_this_build_has() {
    let Some(api) = api_or_skip() else { return };
    // Unreal's own static-mesh registry tags, plus the general
    // ones. Present means the engine cooked that tag name in.
    let candidates = [
        "ApproxSize",
        "Bounds",
        "BoundsExtent",
        "Triangles",
        "Vertices",
        "Materials",
        "LODs",
        "MinLOD",
        "CollisionPrims",
        "NaniteEnabled",
        "UVChannels",
        "PhysicsAsset",
    ];
    println!("{:<18} {}", "tag", "in this build");
    let mut present = Vec::new();
    for text in candidates {
        let r = api.op("string_to_fname", json!({ "text": text }));
        let found = r.ok && r.result["found"] == json!(true);
        println!("{text:<18} {}", if found { "yes" } else { "no" });
        if found {
            present.push(text);
        }
    }
    println!("\npresent: {present:?}");
    // Not an assertion about WHICH are present: this test exists
    // to find that out. It asserts the mechanism worked at all,
    // because zero hits would mean the lookup is broken rather
    // than that the game has no tags.
    assert!(
        !present.is_empty(),
        "not one candidate name exists, which means the lookup is broken"
    );
}

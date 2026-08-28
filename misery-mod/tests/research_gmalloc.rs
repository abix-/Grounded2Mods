//! Which vtable slot is `FMalloc::Malloc`?
//!
//! Anything the engine will later grow or free must be allocated
//! by the ENGINE. Hand it a Rust buffer and it works until the
//! engine reallocs that array, at which point `FMallocBinned2`
//! looks for its marker in the bytes before the block, finds
//! Rust's data, and kills the process:
//!
//! ```text
//! FMallocBinned2 Attempt to realloc an unrecognized block
//! canary == 0x65 != 0xe3
//! ```
//!
//! That is a real crash from 2026-08-26, and it fired on
//! disconnect, long after the write that caused it, because
//! disconnect is when the engine tears down the arrays a mod
//! grew.
//!
//! Calling the engine's allocator means calling a virtual method,
//! which means knowing its slot. **Slot 2 was GUESSED once** from
//! patternsleuth's pattern bytes and tried live the same day: the
//! call returned null and the process died in the same second.
//! So `ue::gmalloc::MALLOC_SLOT` is `None` and every vendor grow
//! logs `grow failed: engine allocator (GMalloc) unavailable`
//! instead.
//!
//! This measures it instead of guessing, by reading the running
//! image:
//!
//! ```text
//! 48 8B 0D | ?? ?? ?? ??   mov rcx, [GMalloc]   <- anchored on the xref
//! ...                      argument shuffling
//! 48 8B 01                 mov rax, [rcx]       <- load the vtable
//! FF 50 ??                 call [rax+imm8]      <- the byte we want
//! ```
//!
//! Everything here is READ-ONLY: it scans and reads bytes, and
//! calls nothing. Getting this wrong by calling is what killed
//! the game last time.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_gmalloc -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

/// What the image says the slot is.
#[test]
fn the_image_says_which_slot_malloc_is() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("measure_malloc_slot", json!({}));
    if !r.ok {
        println!("measure_malloc_slot failed: {:?}", r.error);
        return;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&r.result).unwrap_or_default()
    );

    let found = r.result["slots_found"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("\nconfigured: {}", r.result["configured"]);
    println!("measured:   {found:?}");

    assert!(!found.is_empty(), "no call site found; the scan needs work");
    // Every site that loads GMalloc and calls through its vtable
    // should call the SAME slot. If they disagree, the scan
    // matched something that is not FMemory::Malloc, and acting
    // on it would be the guess this test exists to replace.
    assert_eq!(
        found.len(),
        1,
        "call sites disagree about the slot: {found:?}. Do not set MALLOC_SLOT from this."
    );
}

/// Is GMalloc reachable at all?
///
/// Separates "the global is not resolved" from "the slot is not
/// known", which produce the same silence otherwise.
#[test]
fn gmalloc_is_reachable() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("inspect_gmalloc", json!({}));
    if !r.ok {
        println!("inspect_gmalloc failed: {:?}", r.error);
        return;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&r.result).unwrap_or_default()
    );
}

/// The bytes at every instruction that references GMalloc.
///
/// For when the expected shapes do not match: read what the
/// binary actually does instead of guessing another encoding.
/// The first attempt guessed `mov r8d,r32` then `mov rdx,r64`
/// before the vtable load, and this build does something else.
#[test]
fn what_the_call_sites_actually_look_like() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("gmalloc_call_sites", json!({ "bytes": 40 }));
    if !r.ok {
        println!("gmalloc_call_sites failed: {:?}", r.error);
        return;
    }
    println!(
        "{} site(s) reference GMalloc
",
        r.result["count"]
    );
    for s in r.result["sites"].as_array().cloned().unwrap_or_default() {
        println!(
            "{}  {}",
            s["at"].as_str().unwrap_or("?"),
            s["bytes"].as_str().unwrap_or("")
        );
    }
}

/// Once the slot is set, this is the proof it works: a vendor
/// pass with no `grow failed` line.
///
/// `#[ignore]`d because it writes to the live game. Run it
/// deliberately, after a vendor pass, and read the mod log.
#[test]
#[ignore = "writes to the live game"]
fn a_grow_actually_grows() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("measure_malloc_slot", json!({}));
    assert!(r.ok, "measure_malloc_slot failed: {:?}", r.error);
    let configured = r.result["configured"].clone();
    assert!(
        !configured.is_null(),
        "MALLOC_SLOT is still None, so nothing will grow. Measure it first."
    );

    let grow = api.op("tarray_grow", json!({}));
    println!(
        "{}",
        serde_json::to_string_pretty(&grow.result).unwrap_or_default()
    );
    assert!(grow.ok, "tarray_grow failed: {:?}", grow.error);
}

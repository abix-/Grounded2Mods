// Layout assertions cross-checked against Dumper-7 SDK headers
// (`C:\Tools\work\sdk\SDK\Basic.hpp` and `CoreUObject_classes.hpp`).
// These run on host, not in-game, so they verify only sizes/offsets of our
// own #[repr(C)] mirrors. Not the live game layout. The live layout is
// validated implicitly the first time we read fields successfully.
//
// FName / FString / TArray live in `ueforge::ue::*`; grounded2-mod
// consumes them via the workspace dependency.

use std::mem::size_of;
use ueforge::ue::tarray::TArray;
use ueforge::ue::{FName, FString};

#[test]
fn fname_is_8_bytes() {
    assert_eq!(size_of::<FName>(), 8);
}

#[test]
fn fstring_is_16_bytes() {
    assert_eq!(size_of::<FString>(), 16);
}

#[test]
fn tarray_ptr_is_16_bytes() {
    assert_eq!(size_of::<TArray<*mut u8>>(), 16);
}

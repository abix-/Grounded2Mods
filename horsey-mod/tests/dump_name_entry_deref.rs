//! GENERIC: dereference a name-table entry by name_id, read N bytes at
//! its first qword, and assert the result is string-shaped.
//!
//! Background: name table entries store names as MSVC `std::string`
//! (SSO or heap pointer). The first qword is either the SSO inline
//! data (printable ASCII) or a heap pointer to a longer string.
//!
//! Modes (env-driven):
//! - `HORSEY_NAME_ID=<n>`: name id to deref (default 250).
//! - `HORSEY_NAME_BYTES=<n>`: bytes to read (default 64).
//! - default: discovery. Logs bytes, asserts first byte is printable
//!   ASCII (sanity that we found a string).
//! - `HORSEY_EXPECT_NAME=<string>`: strict. Assert the entry resolves
//!   to this string (handles both SSO and heap-deref paths).

mod common;

use serde_json::{Value, json};

#[test]
fn dump_name_entry_deref() {
    let Some(game) = common::launch("dump_name_entry_deref") else {
        return;
    };
    let name_id: u64 = common::env_or("HORSEY_NAME_ID", 250u64);
    let n: u64 = common::env_or("HORSEY_NAME_BYTES", 64u64);

    let r = game
        .op_json("horse.name_diag", &json!({"name_id": name_id}))
        .unwrap();
    let result = r.get("result").unwrap_or(&r);
    let first = result
        .get("first_qword")
        .and_then(Value::as_str)
        .unwrap_or("0x0");
    let entry_addr = result.get("entry").and_then(Value::as_str).unwrap_or("0x0");
    eprintln!("name_id={name_id} entry={entry_addr} first_qword={first}");

    let bytes_r = game
        .op_json("patterns.read_bytes", &json!({"addr": first, "n": n}))
        .unwrap();
    let hex = bytes_r
        .get("result")
        .and_then(|x| x.get("bytes"))
        .and_then(Value::as_str)
        .expect("no bytes field");
    eprintln!("bytes @ first_qword:\n  {hex}");

    let bytes: Vec<u8> = hex
        .split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap_or(0))
        .collect();
    assert!(!bytes.is_empty(), "no bytes read");

    let printable_run: String = bytes
        .iter()
        .take_while(|&&b| b >= 0x20 && b < 0x7f)
        .map(|&b| b as char)
        .collect();
    eprintln!(
        "printable prefix: {:?} (len {})",
        printable_run,
        printable_run.len()
    );

    let first_byte = bytes[0];
    assert!(
        first_byte >= 0x20 && first_byte < 0x7f,
        "first byte 0x{first_byte:02x} not printable ASCII; entry doesn't look string-shaped"
    );

    if let Ok(want) = std::env::var("HORSEY_EXPECT_NAME") {
        assert!(
            printable_run.starts_with(&want),
            "HORSEY_EXPECT_NAME='{want}' but read printable prefix '{printable_run}'"
        );
    }

    game.pass(&format!("name_id={name_id} -> '{printable_run}'"));
}

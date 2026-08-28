//! GENERIC probe: verify the CRISPR-revealed paired-genome hypothesis.
//!
//! CRISPR's apply function (`FUN_1400b39b0`, 0x1400b39b0) writes each
//! allele to TWO banks: `*(u8*)(horse + offset) = val` and
//! `*(u8*)(horse + offset + 0xF0) = val`. If the primary bank starts
//! at `horse + 0x2b8`, the paired bank should start at `horse + 0x3a8`.
//!
//! This probe dumps both banks for the target horse and reports
//! agreement / disagreement (a freshly-applied genome should be near
//! identical between banks; long-lived horses may drift via the
//! breeding / regen path).
//!
//! Modes (env-driven):
//! - `HORSEY_HORSE=<name>` (default: first owned).
//! - default: print + log; passes regardless of paired-bank content.
//! - `HORSEY_EXPECT_PAIRED=1`: strict. Assert the paired bank holds
//!   real allele data (not all zeros) and agreement count >= 100/240.
//!   Use to lock the diploid-genome hypothesis as regression coverage.

mod common;

use serde_json::{Value, json};

const PRIMARY_OFF: usize = 0x2b8;
const PAIRED_OFF: usize = 0x2b8 + 0xF0;
const LEN: usize = 240;

#[test]
fn probe_paired_genome() {
    let Some(game) = common::launch("probe_paired_genome") else {
        return;
    };
    let h = common::target_horse(&game);
    let ptr = u64::from_str_radix(h.ptr_s.trim_start_matches("0x"), 16).unwrap();
    eprintln!("target: {} ({})", h.name, h.ptr_s);

    let read = |off: usize| -> Vec<u8> {
        let r = game
            .op_json(
                "patterns.read_bytes",
                &json!({
                    "addr": format!("0x{:x}", ptr + off as u64),
                    "n":    LEN,
                }),
            )
            .unwrap();
        r.get("result")
            .and_then(|x| x.get("bytes"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .split_whitespace()
            .filter_map(|h| u8::from_str_radix(h, 16).ok())
            .collect()
    };

    let primary = read(PRIMARY_OFF);
    let paired = read(PAIRED_OFF);
    assert_eq!(primary.len(), LEN);
    assert_eq!(paired.len(), LEN);

    let p_nz = primary.iter().filter(|&&b| b != 0).count();
    let s_nz = paired.iter().filter(|&&b| b != 0).count();
    let p_max = primary.iter().copied().max().unwrap_or(0);
    let s_max = paired.iter().copied().max().unwrap_or(0);
    eprintln!("PRIMARY  +0x{PRIMARY_OFF:x}: {p_nz}/{LEN} nonzero, max {p_max}");
    eprintln!("PAIRED   +0x{PAIRED_OFF:x}: {s_nz}/{LEN} nonzero, max {s_max}");

    let same: usize = primary
        .iter()
        .zip(paired.iter())
        .filter(|(a, b)| a == b)
        .count();
    let diff = LEN - same;
    eprintln!("agreement: {same}/{LEN} slots equal ({diff} differ)");

    eprintln!("\n=== PRIMARY ===");
    dump(&primary);
    eprintln!("\n=== PAIRED ===");
    dump(&paired);

    if s_nz == 0 && s_max == 0 {
        eprintln!(
            "\n[FAIL?] paired bank is all zeros -- offset may be wrong, or genome is single-bank"
        );
    } else {
        eprintln!("\n[CONFIRMED] paired bank at horse+0x{PAIRED_OFF:x} holds real allele data");
    }

    if std::env::var("HORSEY_EXPECT_PAIRED").ok().as_deref() == Some("1") {
        assert!(
            s_nz > 0,
            "HORSEY_EXPECT_PAIRED=1: paired bank at horse+0x{PAIRED_OFF:x} is all zeros; \
             diploid-genome hypothesis failed for {}",
            h.name
        );
        assert!(
            same >= 100,
            "HORSEY_EXPECT_PAIRED=1: only {same}/{LEN} slots agree between primary and paired; \
             banks have drifted too far. {} differ",
            LEN - same
        );
    }
}

fn dump(bytes: &[u8]) {
    for row in 0..15 {
        eprint!("  ");
        for col in 0..16 {
            eprint!("{:>3} ", bytes[row * 16 + col]);
        }
        eprintln!();
    }
}

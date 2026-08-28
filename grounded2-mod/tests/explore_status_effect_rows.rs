// Discovery test: find rows in Table_StatusEffects matching the
// EStatusEffectType values we want to migrate skills to.
//
// All TMap walking + FName + per-row reads now go through
// `ueforge::client`. The test owns only the per-row
// field offsets (Type at +0x30, Value at +0x34) and the target
// type-id set.
//
// Run:
//   set BBP_DEBUG_PORT=17171
//   cargo test --release --target x86_64-pc-windows-msvc \
//     --test explore_status_effect_rows. --nocapture --test-threads=1

mod common;

use std::collections::BTreeMap;

use ueforge::client;

const TARGET_TYPES: &[(u8, &str)] = &[
    (5, "MaxHealth"),
    (14, "FallDamage"),
    (23, "AttackDamage"),
    (24, "Health"),
    (29, "DamageReduction"),
    (30, "DamageReductionMultiplier"),
    (38, "LifeSteal"),
];

// FStatusEffectData layout (Maine_structs.hpp): Type@+0x30 (u8),
// Value@+0x34 (f32).
const ROW_TYPE_OFFSET: u64 = 0x30;
const ROW_VALUE_OFFSET: u64 = 0x34;

#[derive(Debug, Clone)]
struct RowFind {
    type_id: u8,
    fname_u64: u64,
    addr: u64,
    value: f32,
}

#[test]
fn discover_rows_by_type() {
    let api = common::Api::require();

    // 1. Find Table_StatusEffects (path-disambiguate. There's
    // both a CDO and the live instance).
    let Some((dt_sel, dt_addr)) =
        client::find_data_table_by_path(api.inner(), "Table_StatusEffects.Table_StatusEffects")
    else {
        panic!("Table_StatusEffects not found");
    };
    eprintln!("Table_StatusEffects @ 0x{dt_addr:016x} (selector={dt_sel})");

    // 2. Read all rows in one batched walk.
    let rows = client::read_data_table_rows(api.inner(), &dt_sel).expect("read_data_table_rows");
    eprintln!("=== {} live rows ===", rows.len());

    // 3. For each row, read Type+Value, capture matches.
    let target_set: std::collections::HashSet<u8> = TARGET_TYPES.iter().map(|(t, _)| *t).collect();
    let mut found: BTreeMap<u8, RowFind> = BTreeMap::new();
    let mut rows_scanned = 0usize;

    for row in &rows {
        rows_scanned += 1;
        let stat_type = client::read_u8(api.inner(), row.addr, ROW_TYPE_OFFSET);
        let value = client::read_f32(api.inner(), row.addr, ROW_VALUE_OFFSET);

        if target_set.contains(&stat_type) && !found.contains_key(&stat_type) {
            found.insert(
                stat_type,
                RowFind {
                    type_id: stat_type,
                    fname_u64: row.fname,
                    addr: row.addr,
                    value,
                },
            );
            eprintln!(
                "  [{:5}] HIT type={stat_type:>3} value={value:>8.3} fname=0x{:016x} row=0x{:016x}",
                rows_scanned, row.fname, row.addr
            );
            if found.len() == target_set.len() {
                eprintln!("  all target types found after {rows_scanned} row reads; stopping");
                break;
            }
        }
    }

    eprintln!("\n=== Migration target rows ===");
    for &(ty, name) in TARGET_TYPES {
        match found.get(&ty) {
            Some(f) => eprintln!(
                "  Type {ty:>3} ({name:<28}) value={:>8.3} fname=0x{:016x} addr=0x{:016x}",
                f.value, f.fname_u64, f.addr
            ),
            None => {
                eprintln!("  Type {ty:>3} ({name:<28}) NOT FOUND in {rows_scanned} scanned rows")
            }
        }
    }

    eprintln!("\nTable selector for the migration: {}", dt_sel);
}

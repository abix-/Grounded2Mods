//! Research test for item stack limits.
//!
//! The ItemList DataTable has 496 rows (one per item), each using
//! the S_ItemDetails row struct. MaxStack is an Int at offset 0x44;
//! AllowStacking is a Bool at 0x40.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_item_stacks -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live, show};
use serde_json::json;

/// Dump a small sample of ItemList rows (first 5) to verify the
/// field name for max stack count and see the data shape.
#[test]
fn sample_item_list() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op(
        "dump_data_table",
        json!({"table_name": "ItemList", "max_rows": 5}),
    );
    show("ItemList (sample)", &r);
}

/// Print every row's name, MaxStack, and AllowStacking values.
/// This confirms the field names and shows the vanilla baseline.
#[test]
fn all_stack_limits() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("dump_data_table", json!({"table_name": "ItemList"}));
    if !r.ok {
        println!("dump_data_table failed: {:?}", r.error);
        return;
    }
    let rows = r.result["rows"].as_array();
    let Some(rows) = rows else {
        println!("no rows array in response");
        return;
    };
    println!("{} rows total", rows.len());
    println!("{:<50} {:>8} {:>10}", "ITEM", "MaxStack", "Stackable");
    println!("{}", "-".repeat(70));
    for row in rows {
        let name = row["row_name"].as_str().unwrap_or("?");
        let fields = &row["fields"];
        let max_stack = fields["MaxStack"].as_i64().unwrap_or(-1);
        let stackable = fields["AllowStacking"]
            .as_bool()
            .map(|b| if b { "yes" } else { "no" })
            .unwrap_or("?");
        println!("{:<50} {:>8} {:>10}", name, max_stack, stackable);
    }
}

/// Apply a 10x multiplier to MaxStack on all rows in ItemList.
/// Uses tweak_apply with op=multiply, which is idempotent (always
/// re-bases on captured vanilla) and persisted to tweaks.json.
#[test]
fn multiply_all_stacks_10x() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op(
        "tweak_apply",
        json!({
            "table": "ItemList",
            "field": "MaxStack",
            "kind": "i32",
            "op": "multiply",
            "value": 10
        }),
    );
    show("tweak_apply 10x MaxStack", &r);
}

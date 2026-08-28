//! Walk DT_Materials rows. Layout from
//! `OutworldStation\Binaries\Win64\ue4ss\CXXHeaderDump\SpaceSalvageStation.hpp`:
//!
//! ```text
//! struct FSMaterialData : public FTableRowBase {  // size 0x170
//!     EItemMaterial Material;     // 0x0008  (u8)
//!     FText Name;                 // 0x0010
//!     FText Description;          // 0x0020
//!     UTexture2D* Icon;           // 0x0030
//!     FLinearColor Colour;        // 0x0038
//!     int32 MaxCanStack;          // 0x0048  <-- TARGET FOR STACK MOD
//!     ...
//! };
//! ```
//!
//! All TMap walking + FName resolution + per-row reads go through
//! `ueforge::client`.

mod common;

use ueforge::client;

const STACK_OFFSET: u64 = 0x48; // FSMaterialData::MaxCanStack

#[test]
fn list_material_names_and_stacks() {
    let Some(api) = common::try_api() else { return };
    let Some((dt_sel, dt_addr)) = client::find_data_table_by_name(&api, "DT_Materials") else {
        eprintln!("DT_Materials not found");
        return;
    };
    eprintln!("DT_Materials @ 0x{dt_addr:X} (selector={dt_sel})");

    let rows = match client::read_data_table_rows(&api, &dt_sel) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("read_data_table_rows failed: {e}");
            return;
        }
    };
    eprintln!("=== {} live rows ===", rows.len());
    let mut found = 0;
    for row in &rows {
        let name = client::fname_to_string(&api, row.fname)
            .unwrap_or_else(|| format!("fname=0x{:016X}", row.fname));
        let stack = client::read_i32(&api, row.addr, STACK_OFFSET);
        eprintln!("  {name:<40} MaxCanStack={stack}");
        found += 1;
    }
    eprintln!("=== {found} live rows ===");
}

//! Built-in ImGui browser tab for every discovered UDataTable.
//! Every game using ueforge gets this for free. Just include
//! `ueforge::ui_data_table_browser::render` in your `ModDef::tabs`:
//!
//! ```ignore
//! tabs: &[
//!     Tab { name: "Tweaks", render: render_tweaks_tab },
//!     Tab { name: "Tables", render: ueforge::ui_data_table_browser::render },
//! ],
//! ```
//!
//! UI flow:
//! 1. List of every cached UDataTable (from `ueforge::discovery`).
//! 2. Click "Open" on a row -> snapshot loads + the schema +
//!    first N rows render below.
//! 3. "Refresh discovery" re-walks GObjects.
//! 4. "Reload rows" re-snapshots the open table.
//!
//! Cache lives in a process-global `Mutex<DtBrowserUi>` mirroring
//! the scanner tab.

use std::sync::OnceLock;

use parking_lot::Mutex;
use serde_json::Value as Json;

use crate::ui;

struct DtBrowserUi {
    selected: Option<String>,
    snapshot: Option<Json>,
    snapshot_error: Option<String>,
    max_rows: i32,
    filter: [u8; 128],
    row_filter: [u8; 128],
}

impl DtBrowserUi {
    fn new() -> Self {
        Self {
            selected: None,
            snapshot: None,
            snapshot_error: None,
            max_rows: 50,
            filter: [0u8; 128],
            row_filter: [0u8; 128],
        }
    }
}

static BROWSER: OnceLock<Mutex<DtBrowserUi>> = OnceLock::new();

fn state() -> &'static Mutex<DtBrowserUi> {
    BROWSER.get_or_init(|| Mutex::new(DtBrowserUi::new()))
}

pub fn render() {
    let mut s = state().lock();

    ui::text("Data tables (live)");
    ui::text_disabled(
        "Every UDataTable ueforge::discovery found at load time. \
         Click Open to snapshot rows. Refresh re-walks GObjects \
         (use after content streams in).",
    );
    ui::spacing();

    if ui::button("Refresh discovery") {
        let _ = crate::discovery::refresh();
        // Re-snapshot the open table so the right pane stays current.
        if let Some(name) = s.selected.clone() {
            load_snapshot(&mut s, &name);
        }
    }
    ui::same_line();
    ui::slider_i32("max rows", &mut s.max_rows, 1, 500);
    ui::input_text("filter##dt_browser", &mut s.filter);
    let filter_owned = ui::cstr_view(&s.filter).to_ascii_lowercase();
    let filter = filter_owned.as_str();

    let cache = crate::discovery::cached();
    let Some(snap) = cache else {
        ui::text_disabled("Discovery has not run yet.");
        return;
    };

    let tables_found = snap
        .data_tables
        .get("tables_found")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    ui::text(&format!("Discovered: {tables_found} tables"));
    ui::separator();
    ui::spacing();

    // Left: list of tables. Right pane (below): selected detail.
    let empty = Vec::new();
    let tables = snap
        .data_tables
        .get("data_tables")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let mut shown_count = 0usize;
    for t in tables {
        let table_name = t
            .get("table_name")
            .and_then(|v| v.as_str())
            .unwrap_or("<no-name>");
        if !filter.is_empty() && !table_name.to_ascii_lowercase().contains(filter) {
            continue;
        }
        shown_count += 1;
        // Eager cache holds names only now. Schema + row_struct
        // walk happens on Open click via snapshot_table.
        let row_struct = "?";
        let field_count: usize = 0;

        let selected = s.selected.as_deref() == Some(table_name);
        let label = if selected {
            format!("[Close]##{table_name}")
        } else {
            format!("Open##{table_name}")
        };
        if ui::small_button(&label) {
            if selected {
                s.selected = None;
                s.snapshot = None;
                s.snapshot_error = None;
            } else {
                let owned = table_name.to_string();
                load_snapshot(&mut s, &owned);
                s.selected = Some(owned);
            }
        }
        ui::same_line();
        ui::text(&format!(
            "{table_name}  -- {row_struct}  ({field_count} fields)"
        ));

        if selected {
            render_selected(&mut s);
        }
    }
    if !filter.is_empty() {
        ui::text_disabled(&format!("filtered: {shown_count} matching '{filter}'"));
    }
}

fn load_snapshot(s: &mut DtBrowserUi, table_name: &str) {
    let max = s.max_rows.max(1) as usize;
    match crate::data_table::snapshot_table(table_name, Some(max)) {
        Some(j) => {
            s.snapshot = Some(j);
            s.snapshot_error = None;
        }
        None => {
            s.snapshot = None;
            s.snapshot_error = Some(format!(
                "table '{table_name}' not loaded or has no RowStruct"
            ));
        }
    }
}

fn render_selected(s: &mut DtBrowserUi) {
    ui::indent();
    if let Some(err) = &s.snapshot_error {
        ui::text_colored(err, (1.0, 0.4, 0.4, 1.0));
        ui::unindent();
        return;
    }
    if s.snapshot.is_none() {
        ui::unindent();
        return;
    }

    // Row-FName filter input (case-insensitive substring on
    // row_name). Hoisted out of the borrow on s.snapshot so we
    // can pass &mut s.row_filter to input_text.
    ui::input_text("row filter##dt_browser_rows", &mut s.row_filter);
    let row_filter_owned = ui::cstr_view(&s.row_filter).to_ascii_lowercase();
    let row_filter = row_filter_owned.as_str();

    let j = s.snapshot.as_ref().unwrap();
    let total = j.get("rows_total").and_then(|v| v.as_u64()).unwrap_or(0);
    let shown = j.get("rows_returned").and_then(|v| v.as_u64()).unwrap_or(0);
    ui::text(&format!("rows: {shown}/{total} shown"));

    if ui::tree_node("schema") {
        let empty = Vec::new();
        let fields = j
            .get("row_struct")
            .and_then(|v| v.get("fields"))
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        for f in fields {
            let n = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let c = f.get("class").and_then(|v| v.as_str()).unwrap_or("?");
            let off = f.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);
            let sz = f.get("element_size").and_then(|v| v.as_i64()).unwrap_or(0);
            ui::text(&format!("  {n}: {c}  @+0x{off:x}  size={sz}"));
        }
        ui::tree_pop();
    }

    if ui::tree_node("rows") {
        let empty = Vec::new();
        let rows = j.get("rows").and_then(|v| v.as_array()).unwrap_or(&empty);
        let mut matched = 0usize;
        for r in rows {
            let row_name = r.get("row_name").and_then(|v| v.as_str()).unwrap_or("?");
            if !row_filter.is_empty() && !row_name.to_ascii_lowercase().contains(row_filter) {
                continue;
            }
            matched += 1;
            let fields = r
                .get("fields")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            let mut line = String::with_capacity(128);
            line.push_str(row_name);
            line.push_str(":  ");
            for (k, v) in fields.iter() {
                line.push_str(k);
                line.push('=');
                line.push_str(&compact_value(v));
                line.push_str("  ");
            }
            ui::text(&line);
        }
        if !row_filter.is_empty() {
            ui::text_disabled(&format!("filtered: {matched} rows matching '{row_filter}'"));
        }
        ui::tree_pop();
    }
    ui::unindent();
}

fn compact_value(v: &Json) -> String {
    match v {
        Json::Null => String::from("null"),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.to_string(),
        Json::String(s) => {
            if s.len() > 32 {
                format!("\"{}...\"", &s[..32])
            } else {
                format!("\"{s}\"")
            }
        }
        Json::Object(map) => {
            if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                format!("<obj:{name}>")
            } else if let Some(hex) = map.get("raw_bytes_hex").and_then(|v| v.as_str()) {
                let short = if hex.len() > 16 { &hex[..16] } else { hex };
                format!("{short}..")
            } else {
                String::from("{..}")
            }
        }
        Json::Array(_) => String::from("[..]"),
    }
}

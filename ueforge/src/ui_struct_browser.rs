//! Built-in ImGui browser tab for every discovered
//! `UScriptStruct`. Parallel to `ui_class_browser`; UScriptStructs
//! are the row layouts of UDataTables + the fields of any
//! `FStructProperty`. Useful for: looking up an offset on a
//! row struct without snapshotting the table.
//!
//! ```ignore
//! tabs: &[
//!     ueforge::TabDef { name: "Structs",
//!                       render: ueforge::ui_struct_browser::render },
//! ],
//! ```

use std::sync::OnceLock;

use parking_lot::Mutex;

use crate::ui;

struct StructBrowserUi {
    selected: Option<String>,
    detail: Option<serde_json::Value>,
    filter: [u8; 128],
}

impl StructBrowserUi {
    fn new() -> Self {
        Self {
            selected: None,
            detail: None,
            filter: [0u8; 128],
        }
    }
}

static BROWSER: OnceLock<Mutex<StructBrowserUi>> = OnceLock::new();

fn state() -> &'static Mutex<StructBrowserUi> {
    BROWSER.get_or_init(|| Mutex::new(StructBrowserUi::new()))
}

pub fn render() {
    let mut s = state().lock();

    ui::text("Script structs");
    ui::text_disabled(
        "Every UScriptStruct ueforge::discovery found. Click Open \
         to see the field list (name, FProperty class, offset, \
         element size). These are the row layouts of UDataTables \
         plus every standalone struct in the game.",
    );
    ui::spacing();

    if ui::button("Refresh discovery##structs") {
        let _ = crate::discovery::refresh();
    }
    ui::same_line();
    ui::input_text("filter##struct_browser", &mut s.filter);
    let filter_owned = ui::cstr_view(&s.filter).to_ascii_lowercase();
    let filter = filter_owned.as_str();

    let Some(snap) = crate::discovery::cached() else {
        ui::text_disabled("Discovery has not run yet.");
        return;
    };

    let structs_found = snap
        .structs
        .get("structs_found")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    ui::text(&format!("Discovered: {structs_found} structs"));
    ui::separator();
    ui::spacing();

    let empty = Vec::new();
    let structs = snap
        .structs
        .get("structs")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let mut shown_count = 0usize;
    for st in structs {
        let name = st.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        if !filter.is_empty() && !name.to_ascii_lowercase().contains(filter) {
            continue;
        }
        shown_count += 1;
        if shown_count > 500 && s.selected.as_deref() != Some(name) {
            continue;
        }
        let selected = s.selected.as_deref() == Some(name);
        let label = if selected {
            format!("[Close]##st_{name}")
        } else {
            format!("Open##st_{name}")
        };
        if ui::small_button(&label) {
            if selected {
                s.selected = None;
                s.detail = None;
            } else {
                s.detail = Some(crate::discovery::struct_detail_json(name));
                s.selected = Some(name.to_string());
            }
        }
        ui::same_line();
        ui::text(name);

        if selected {
            if let Some(d) = &s.detail {
                render_field_list(d);
            }
        }
    }
    if shown_count > 500 {
        ui::text_disabled(&format!(
            "{} matches; showing first 500. Tighten the filter.",
            shown_count
        ));
    } else if !filter.is_empty() {
        ui::text_disabled(&format!("filtered: {shown_count} matching '{filter}'"));
    }
}

fn render_field_list(st: &serde_json::Value) {
    ui::indent();
    let empty = Vec::new();
    let fields = st
        .get("fields")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    for f in fields {
        let n = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let cls = f.get("class").and_then(|v| v.as_str()).unwrap_or("?");
        let off = f.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);
        let sz = f.get("element_size").and_then(|v| v.as_i64()).unwrap_or(0);
        ui::text(&format!("  {n}: {cls}  @+0x{off:x}  size={sz}"));
    }
    if fields.is_empty() {
        ui::text_disabled("  (no fields)");
    }
    ui::unindent();
}

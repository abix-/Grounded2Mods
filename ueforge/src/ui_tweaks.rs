//! Built-in ImGui tab for the runtime tweak surface.
//!
//! Every game using ueforge can include this in its `ModDef::tabs`
//! to get a hot-iteration UX for data-table field tweaks without
//! curl:
//!
//! ```ignore
//! tabs: &[
//!     TabDef { name: "Tweaks", render: ueforge::ui_tweaks::render },
//!     // ...
//! ],
//! ```
//!
//! Wraps the existing `data_table::tweak_*` ops:
//! - Apply form (`table`, `field`, `kind`, `op`, `value` -> Apply).
//! - Active tweaks list with per-row Revert.
//! - Persisted (on-disk) list with Reload + Reapply.
//! - Revert-all button.

use std::sync::OnceLock;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormKind {
    I32,
    F32,
    U32,
}

impl FormKind {
    fn as_str(self) -> &'static str {
        match self {
            FormKind::I32 => "i32",
            FormKind::F32 => "f32",
            FormKind::U32 => "u32",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormOp {
    Set,
    Multiply,
    Add,
}

impl FormOp {
    fn as_str(self) -> &'static str {
        match self {
            FormOp::Set => "set",
            FormOp::Multiply => "multiply",
            FormOp::Add => "add",
        }
    }
}

struct TweaksUi {
    table_buf: [u8; 128],
    field_buf: [u8; 128],
    kind: FormKind,
    op: FormOp,
    value_int: i32,
    value_float: f32,
    last_result: Option<String>,
    last_error: Option<String>,
}

impl TweaksUi {
    fn new() -> Self {
        Self {
            table_buf: [0u8; 128],
            field_buf: [0u8; 128],
            kind: FormKind::I32,
            op: FormOp::Multiply,
            value_int: 2,
            value_float: 2.0,
            last_result: None,
            last_error: None,
        }
    }
}

static STATE: OnceLock<Mutex<TweaksUi>> = OnceLock::new();

fn state() -> &'static Mutex<TweaksUi> {
    STATE.get_or_init(|| Mutex::new(TweaksUi::new()))
}

pub fn render() {
    let mut s = state().lock();

    ui::text("Runtime tweaks");
    ui::text_disabled(
        "Apply a (table, field) write at runtime. Vanilla per row is \
         snapshotted on first apply; subsequent applies re-base on \
         vanilla (idempotent). Persisted to <DLL_dir>/tweaks.json.",
    );
    ui::spacing();

    if ui::collapsing_header("Apply a tweak") {
        ui::input_text("table##tweaks", &mut s.table_buf);
        ui::input_text("field##tweaks", &mut s.field_buf);

        // Kind radio-ish: three small buttons.
        ui::text("kind:");
        ui::same_line();
        for &k in &[FormKind::I32, FormKind::F32, FormKind::U32] {
            let label = if k == s.kind {
                format!("[{}]##kind", k.as_str())
            } else {
                format!(" {} ##kind", k.as_str())
            };
            if ui::small_button(&label) {
                s.kind = k;
            }
            ui::same_line();
        }
        ui::new_line();

        ui::text("op:");
        ui::same_line();
        for &op in &[FormOp::Set, FormOp::Multiply, FormOp::Add] {
            let label = if op == s.op {
                format!("[{}]##op", op.as_str())
            } else {
                format!(" {} ##op", op.as_str())
            };
            if ui::small_button(&label) {
                s.op = op;
            }
            ui::same_line();
        }
        ui::new_line();

        match s.kind {
            FormKind::I32 | FormKind::U32 => {
                ui::input_i32("value##tweaks_i", &mut s.value_int);
            }
            FormKind::F32 => {
                ui::slider_f32("value##tweaks_f", &mut s.value_float, -1000.0, 1000.0);
            }
        }

        if ui::button("Apply") {
            let table = ui::cstr_view(&s.table_buf).to_string();
            let field = ui::cstr_view(&s.field_buf).to_string();
            if table.is_empty() || field.is_empty() {
                s.last_error = Some("table and field are required".into());
                s.last_result = None;
            } else {
                let value: Json = match s.kind {
                    FormKind::I32 => json!(s.value_int),
                    FormKind::U32 => json!(s.value_int.max(0) as u32),
                    FormKind::F32 => json!(s.value_float),
                };
                let args = json!({
                    "table": table,
                    "field": field,
                    "kind": s.kind.as_str(),
                    "op": s.op.as_str(),
                    "value": value,
                });
                match crate::data_table::tweak_apply_from_args(&args) {
                    Ok(j) => {
                        s.last_result = Some(j.to_string());
                        s.last_error = None;
                    }
                    Err(e) => {
                        s.last_error = Some(e);
                        s.last_result = None;
                    }
                }
            }
        }
        if let Some(err) = &s.last_error {
            ui::text_colored(&format!("error: {err}"), (1.0, 0.4, 0.4, 1.0));
        }
        if let Some(ok) = &s.last_result {
            ui::text_disabled(ok);
        }
    }

    ui::spacing();
    ui::separator();
    ui::spacing();

    if ui::collapsing_header("Active tweaks (in-memory)") {
        let list = crate::data_table::dynamic_list_json();
        let empty = Vec::new();
        let entries = list
            .get("tweaks")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        let count = entries.len();
        ui::text(&format!("count: {count}"));

        if ui::button("Revert all") {
            let n = crate::data_table::dynamic_revert_all();
            let _ = crate::data_table::forget_persisted_all_pub();
            s.last_result = Some(format!("reverted {n} rows; cleared persisted"));
            s.last_error = None;
        }
        ui::spacing();

        for e in entries {
            let kind = e.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let table = e.get("table_name").and_then(|v| v.as_str()).unwrap_or("?");
            let field = e.get("field_name").and_then(|v| v.as_str()).unwrap_or("?");
            let vanilla_count = e.get("vanilla_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let offset = e.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);

            ui::text(&format!(
                "[{kind}] {table}.{field}  @+0x{offset:x}  rows={vanilla_count}"
            ));
            let revert_label = format!("Revert##{kind}_{table}_{field}");
            ui::same_line();
            if ui::small_button(&revert_label) {
                let rows = crate::data_table::dynamic_revert_one(table, field);
                let _ = crate::data_table::forget_persisted_pub(table, field);
                s.last_result = Some(format!(
                    "reverted {table}.{field}: {rows} rows; forgot persisted"
                ));
                s.last_error = None;
            }
        }
    }

    ui::spacing();
    ui::separator();
    ui::spacing();

    if ui::collapsing_header("Persisted on disk") {
        let list = crate::data_table::persisted_list_json();
        let path = list.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let count = list.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        ui::text(&format!("file: {path}"));
        ui::text(&format!("count: {count}"));

        if ui::button("Reload from disk") {
            let res = crate::data_table::load_persisted_from_disk();
            s.last_result = Some(res.to_string());
            s.last_error = None;
        }
        ui::same_line();
        if ui::button("Reapply persisted") {
            let res = crate::data_table::reapply_persisted();
            s.last_result = Some(res.to_string());
            s.last_error = None;
        }
        ui::spacing();

        let empty = Vec::new();
        let entries = list
            .get("tweaks")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        for e in entries {
            let table = e.get("table").and_then(|v| v.as_str()).unwrap_or("?");
            let field = e.get("field").and_then(|v| v.as_str()).unwrap_or("?");
            let kind = e.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let op = e.get("op").and_then(|v| v.as_str()).unwrap_or("?");
            let value = e.get("value").cloned().unwrap_or(Json::Null);
            ui::text(&format!("[{kind}] {op} {value} on {table}.{field}"));
        }
    }
}

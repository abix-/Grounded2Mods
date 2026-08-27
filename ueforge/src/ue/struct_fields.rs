//! Read and write fields on a UE struct at a raw pointer, driven
//! by a static field definition table. Reusable across any mod
//! that needs to inspect or patch a settings/config struct.

use parking_lot::Mutex;

use crate::ue::{read_at, write_at};

#[derive(Debug, Clone, Copy)]
pub enum FieldType {
    Double,
    Bool,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldDef {
    pub name: &'static str,
    pub desc: &'static str,
    pub offset: usize,
    pub ty: FieldType,
}

#[derive(Debug, Clone)]
pub enum FieldValue {
    Double(f64),
    Bool(bool),
}

impl std::fmt::Display for FieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldValue::Double(v) => write!(f, "{v}"),
            FieldValue::Bool(v) => write!(f, "{v}"),
        }
    }
}

/// Read a field from `base_ptr + struct_base + field.offset`.
pub fn read_field(base_ptr: *const u8, struct_base: usize, field: &FieldDef) -> Result<FieldValue, String> {
    let abs = struct_base + field.offset;
    match field.ty {
        FieldType::Double => {
            let v: f64 = unsafe { read_at(base_ptr, abs) };
            Ok(FieldValue::Double(v))
        }
        FieldType::Bool => {
            let v: u8 = unsafe { read_at(base_ptr, abs) };
            Ok(FieldValue::Bool(v != 0))
        }
    }
}

/// Write a f64 to `base_ptr + struct_base + field.offset`.
pub fn write_double(base_ptr: *const u8, struct_base: usize, field: &FieldDef, value: f64, label: &str) {
    unsafe { write_at(base_ptr, struct_base + field.offset, value) };
    crate::log::log(format_args!("{label}: {} = {value}", field.name));
}

/// Write a bool to `base_ptr + struct_base + field.offset`.
pub fn write_bool(base_ptr: *const u8, struct_base: usize, field: &FieldDef, value: bool, label: &str) {
    unsafe { write_at(base_ptr, struct_base + field.offset, value as u8) };
    crate::log::log(format_args!("{label}: {} = {value}", field.name));
}

pub struct FieldAccessor {
    ptr: *const u8,
    base_offset: usize,
    label: &'static str,
}

impl FieldAccessor {
    pub fn new(ptr: *const u8, base_offset: usize, label: &'static str) -> Self {
        Self { ptr, base_offset, label }
    }

    pub fn ptr(&self) -> *const u8 {
        self.ptr
    }

    pub fn base_offset(&self) -> usize {
        self.base_offset
    }

    pub fn read(&self, field: &FieldDef) -> Result<FieldValue, String> {
        read_field(self.ptr, self.base_offset, field)
    }

    pub fn write_double(&self, field: &FieldDef, value: f64) {
        write_double(self.ptr, self.base_offset, field, value, self.label);
    }

    pub fn write_bool(&self, field: &FieldDef, value: bool) {
        write_bool(self.ptr, self.base_offset, field, value, self.label);
    }
}

struct EditorState {
    loaded: bool,
    doubles: Vec<f32>,
    bools: Vec<bool>,
}

/// Cached ImGui editor for one static field catalog.
pub struct FieldEditor {
    id: &'static str,
    state: Mutex<Option<EditorState>>,
}

impl FieldEditor {
    pub const fn new(id: &'static str) -> Self {
        Self {
            id,
            state: Mutex::new(None),
        }
    }

    /// Draw refresh, slider, and checkbox controls for `fields`.
    /// The consumer supplies the live object accessor and the
    /// useful range for each game-specific numeric field.
    pub fn render<A, R>(&self, fields: &'static [FieldDef], accessor: A, range: R)
    where
        A: Fn() -> Result<FieldAccessor, String> + Copy,
        R: Fn(&str, f32) -> (f32, f32),
    {
        let mut state = self.state.lock();
        let state = state.get_or_insert_with(|| EditorState {
            loaded: false,
            doubles: vec![0.0; fields.len()],
            bools: vec![false; fields.len()],
        });

        if crate::ui::button(&format!("Refresh##{}", self.id)) {
            let _ = load_fields(state, fields, accessor());
        }

        if !state.loaded {
            crate::ui::text_disabled("Click Refresh after loading a save.");
            return;
        }

        crate::ui::separator();
        crate::ui::spacing();
        crate::ui::begin_child(&format!("##{}_scroll", self.id), 0.0, 0.0);

        for (i, field) in fields.iter().enumerate() {
            match field.ty {
                FieldType::Double => {
                    let (lo, hi) = range(field.name, state.doubles[i]);
                    crate::ui::text(field.name);
                    crate::ui::same_line();
                    crate::ui::text_disabled(field.desc);
                    crate::ui::set_next_item_width(250.0);
                    if crate::ui::slider_f32(
                        &format!("##{}_{}", self.id, field.name),
                        &mut state.doubles[i],
                        lo,
                        hi,
                    ) && let Ok(accessor) = accessor()
                    {
                        accessor.write_double(field, state.doubles[i] as f64);
                    }
                }
                FieldType::Bool => {
                    if crate::ui::checkbox(
                        &format!("{}##{}_b", field.name, self.id),
                        &mut state.bools[i],
                    ) && let Ok(accessor) = accessor()
                    {
                        accessor.write_bool(field, state.bools[i]);
                    }
                    crate::ui::same_line();
                    crate::ui::text_disabled(field.desc);
                }
            }
            crate::ui::spacing();
        }

        crate::ui::dummy(0.0, 40.0);
        crate::ui::end_child();
    }
}

fn load_fields(
    state: &mut EditorState,
    fields: &[FieldDef],
    accessor: Result<FieldAccessor, String>,
) -> Result<(), String> {
    let accessor = accessor?;
    for (i, field) in fields.iter().enumerate() {
        match accessor.read(field)? {
            FieldValue::Double(value) => state.doubles[i] = value as f32,
            FieldValue::Bool(value) => state.bools[i] = value,
        }
    }
    state.loaded = true;
    Ok(())
}

//! Read and write fields on a UE struct at a raw pointer, driven
//! by a static field definition table. Reusable across any mod
//! that needs to inspect or patch a settings/config struct.

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

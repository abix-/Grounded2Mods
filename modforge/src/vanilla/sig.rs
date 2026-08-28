//! Signature / argument value / return value data types for
//! `modforge::vanilla`. Pure data; no asm. Const-evaluable so a
//! Signature can live as a `'static` constant attached to a
//! `sleuth::TargetDef`.

/// Kind of a single argument in a Win64 function signature.
/// Position of the arg in the sig drives which register slot
/// it goes into (RCX/RDX/R8/R9 for positions 0..3 then stack).
/// Type drives whether the slot is GPR-marshalled or XMM-marshalled,
/// but on Win64 first-4 positions ALWAYS reserve both slots and
/// the dispatcher loads both unconditionally (safe per ABI).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ArgKind {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Ptr,
    Bool,
    F32,
    F64,
}

/// Return type of a Win64 function. Integers/pointers come back in
/// RAX. Floats come back in XMM0. Void means no return.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RetKind {
    Void,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Ptr,
    Bool,
    F32,
    F64,
}

/// One function's full Win64 signature. Attached as
/// `Option<&'static Signature>` to a `sleuth::TargetDef`'s
/// `TargetKind::FunctionEntry { signature }`.
#[derive(Debug, Copy, Clone)]
pub struct Signature {
    pub args: &'static [ArgKind],
    pub ret: RetKind,
}

impl Signature {
    pub const fn new(args: &'static [ArgKind], ret: RetKind) -> Self {
        Self { args, ret }
    }
}

/// One runtime argument value. The dispatcher decodes per the
/// Signature's `args` slot at the matching position.
#[derive(Debug, Copy, Clone)]
pub enum ArgValue {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Ptr(u64),
    Bool(bool),
    F32(f32),
    F64(f64),
}

impl ArgValue {
    /// Match the value's variant to a SignKind. Used by the
    /// dispatcher to validate that supplied args match the
    /// declared signature.
    pub fn kind(&self) -> ArgKind {
        match self {
            Self::I8(_) => ArgKind::I8,
            Self::I16(_) => ArgKind::I16,
            Self::I32(_) => ArgKind::I32,
            Self::I64(_) => ArgKind::I64,
            Self::U8(_) => ArgKind::U8,
            Self::U16(_) => ArgKind::U16,
            Self::U32(_) => ArgKind::U32,
            Self::U64(_) => ArgKind::U64,
            Self::Ptr(_) => ArgKind::Ptr,
            Self::Bool(_) => ArgKind::Bool,
            Self::F32(_) => ArgKind::F32,
            Self::F64(_) => ArgKind::F64,
        }
    }

    /// Marshall to a u64 for placement in a GPR slot (RCX/RDX/R8/R9
    /// or stack). Integer/pointer/bool kinds materialize directly;
    /// floats reinterpret their bit pattern.
    pub fn to_u64(&self) -> u64 {
        match *self {
            Self::I8(v) => v as i64 as u64,
            Self::I16(v) => v as i64 as u64,
            Self::I32(v) => v as i64 as u64,
            Self::I64(v) => v as u64,
            Self::U8(v) => v as u64,
            Self::U16(v) => v as u64,
            Self::U32(v) => v as u64,
            Self::U64(v) => v,
            Self::Ptr(v) => v,
            Self::Bool(v) => {
                if v {
                    1
                } else {
                    0
                }
            }
            Self::F32(v) => v.to_bits() as u64,
            Self::F64(v) => v.to_bits(),
        }
    }

    /// True if this value goes into an XMM register at Win64
    /// positions 0..3. Drives the dispatcher's load.
    pub fn is_xmm(&self) -> bool {
        matches!(self, Self::F32(_) | Self::F64(_))
    }
}

/// One runtime return value. The dispatcher constructs this per
/// the Signature's `ret` field.
#[derive(Debug, Copy, Clone)]
pub enum RetValue {
    Void,
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Ptr(u64),
    Bool(bool),
    F32(f32),
    F64(f64),
}

impl RetValue {
    pub fn kind(&self) -> RetKind {
        match self {
            Self::Void => RetKind::Void,
            Self::I8(_) => RetKind::I8,
            Self::I16(_) => RetKind::I16,
            Self::I32(_) => RetKind::I32,
            Self::I64(_) => RetKind::I64,
            Self::U8(_) => RetKind::U8,
            Self::U16(_) => RetKind::U16,
            Self::U32(_) => RetKind::U32,
            Self::U64(_) => RetKind::U64,
            Self::Ptr(_) => RetKind::Ptr,
            Self::Bool(_) => RetKind::Bool,
            Self::F32(_) => RetKind::F32,
            Self::F64(_) => RetKind::F64,
        }
    }

    /// Construct a RetValue from raw RAX bits and the declared
    /// return kind. Caller separately captures XMM0 for F32/F64.
    pub fn from_rax(rax: u64, kind: RetKind) -> Self {
        match kind {
            RetKind::Void => Self::Void,
            RetKind::I8 => Self::I8(rax as i8),
            RetKind::I16 => Self::I16(rax as i16),
            RetKind::I32 => Self::I32(rax as i32),
            RetKind::I64 => Self::I64(rax as i64),
            RetKind::U8 => Self::U8(rax as u8),
            RetKind::U16 => Self::U16(rax as u16),
            RetKind::U32 => Self::U32(rax as u32),
            RetKind::U64 => Self::U64(rax),
            RetKind::Ptr => Self::Ptr(rax),
            RetKind::Bool => Self::Bool(rax != 0),
            // For floats the caller should call `from_xmm` instead;
            // but if they routed here, reinterpret RAX bits.
            RetKind::F32 => Self::F32(f32::from_bits(rax as u32)),
            RetKind::F64 => Self::F64(f64::from_bits(rax)),
        }
    }

    /// Construct a float RetValue from raw XMM0 bits.
    pub fn from_xmm(xmm: u64, kind: RetKind) -> Self {
        match kind {
            RetKind::F32 => Self::F32(f32::from_bits(xmm as u32)),
            RetKind::F64 => Self::F64(f64::from_bits(xmm)),
            other => Self::from_rax(xmm, other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_const_constructible() {
        const SIG: Signature = Signature::new(&[ArgKind::Ptr, ArgKind::I32], RetKind::U32);
        assert_eq!(SIG.args.len(), 2);
        assert_eq!(SIG.ret, RetKind::U32);
    }

    #[test]
    fn arg_value_to_u64_round_trips() {
        assert_eq!(ArgValue::U32(0xdeadbeef).to_u64(), 0xdeadbeef);
        assert_eq!(ArgValue::I32(-1).to_u64(), u64::MAX);
        assert_eq!(ArgValue::Ptr(0x1_4000_0000).to_u64(), 0x1_4000_0000);
        assert_eq!(ArgValue::Bool(true).to_u64(), 1);
        assert_eq!(ArgValue::Bool(false).to_u64(), 0);
        // Float reinterpret bits.
        assert_eq!(ArgValue::F32(1.0).to_u64() as u32, 1.0f32.to_bits());
        assert_eq!(ArgValue::F64(2.5).to_u64(), 2.5f64.to_bits());
    }

    #[test]
    fn arg_value_kind_matches_variant() {
        assert_eq!(ArgValue::I32(0).kind(), ArgKind::I32);
        assert_eq!(ArgValue::F64(0.0).kind(), ArgKind::F64);
        assert_eq!(ArgValue::Ptr(0).kind(), ArgKind::Ptr);
    }

    #[test]
    fn arg_value_is_xmm_only_for_floats() {
        assert!(ArgValue::F32(1.0).is_xmm());
        assert!(ArgValue::F64(1.0).is_xmm());
        assert!(!ArgValue::I32(0).is_xmm());
        assert!(!ArgValue::Ptr(0).is_xmm());
        assert!(!ArgValue::Bool(true).is_xmm());
    }

    #[test]
    fn ret_value_from_rax_truncates_correctly() {
        assert_eq!(
            RetValue::from_rax(0x1_0000_0042, RetKind::U8),
            RetValue::U8(0x42)
        );
        assert_eq!(
            RetValue::from_rax(0xffff_ffff, RetKind::I32),
            RetValue::I32(-1)
        );
        assert_eq!(RetValue::from_rax(0, RetKind::Bool), RetValue::Bool(false));
        assert_eq!(RetValue::from_rax(1, RetKind::Bool), RetValue::Bool(true));
        assert_eq!(
            RetValue::from_rax(0xdeadbeef_cafebabe, RetKind::Void),
            RetValue::Void
        );
    }

    #[test]
    fn ret_value_from_xmm_for_floats() {
        let bits = 3.14f64.to_bits();
        assert_eq!(RetValue::from_xmm(bits, RetKind::F64), RetValue::F64(3.14));
        let bits = 2.5f32.to_bits() as u64;
        assert_eq!(RetValue::from_xmm(bits, RetKind::F32), RetValue::F32(2.5));
    }
}

impl PartialEq for RetValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Void, Self::Void) => true,
            (Self::I8(a), Self::I8(b)) => a == b,
            (Self::I16(a), Self::I16(b)) => a == b,
            (Self::I32(a), Self::I32(b)) => a == b,
            (Self::I64(a), Self::I64(b)) => a == b,
            (Self::U8(a), Self::U8(b)) => a == b,
            (Self::U16(a), Self::U16(b)) => a == b,
            (Self::U32(a), Self::U32(b)) => a == b,
            (Self::U64(a), Self::U64(b)) => a == b,
            (Self::Ptr(a), Self::Ptr(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::F32(a), Self::F32(b)) => a.to_bits() == b.to_bits(),
            (Self::F64(a), Self::F64(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

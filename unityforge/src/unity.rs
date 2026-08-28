//! Backend-agnostic Unity SDK.
//!
//! The high-level surface mod authors should use. Calls
//! dispatch through the active C# shim's bridge table; the
//! same `unity::Type::find(name)` works against a Mono or
//! IL2CPP target.
//!
//! Backend-specific escape hatches live in
//! [`crate::mono`] (Mono-only) and [`crate::il2cpp`]
//! (IL2CPP-only) modules. Most mod code never touches those.
//!
//! ```ignore
//! use unityforge::unity::Type;
//!
//! let ty = Type::find("PlayerCarryingController")?;
//! let obj = ty.singleton_instance()?;
//! obj.write_field("_maxCapacity", &json!(100.0))?;
//! ```

pub use crate::bridge::{RuntimeKind, runtime_kind};
pub use crate::mono::{LogLevel, MonoObject as Object, MonoType as Type, log};

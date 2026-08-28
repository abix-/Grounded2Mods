//! Function-pointer bridge between the active C# shim and Rust.
//!
//! Whichever shim is loaded (Mono or IL2CPP) calls
//! `unityforge_init(*const BridgeTable)` once at startup,
//! passing a packed struct of `extern "C" fn` pointers. Rust
//! stashes it in a `OnceLock<BridgeTable>` and every subsequent
//! reflection / Harmony call routes through it.
//!
//! The struct layout MUST exactly match the C# `[StructLayout
//! (LayoutKind.Sequential)]` `BridgeTable` in
//! `cs-shim-common/Bridge.cs`. Add fields at the END only;
//! never reorder.
//!
//! ABI versioning: the first two fields are a magic number and
//! a version. The third is the runtime kind tag (Mono / IL2CPP)
//! so Rust code that must branch on the backend can. The shim
//! and Rust check magic + version at init; a mismatch returns
//! an error to the shim and refuses to start.

use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::OnceLock;

/// Magic number identifying a unityforge bridge. ASCII "UFBR"
/// in little-endian.
pub const BRIDGE_MAGIC: u32 = 0x52424655;

/// Current ABI version. v4 added `list_methods`; v5 added
/// `harmony_patch_prefix_ctx`; v6 added `invoke_static` (the
/// Rust side also accepts a v5 table and leaves the new tail
/// None until the game restarts on the upgraded shim).
pub const BRIDGE_VERSION: u32 = 6;

/// Unity runtime backend. Stored in the bridge struct at init;
/// read via [`runtime_kind`] for code that must branch on
/// backend (rare; the high-level SDK is backend-agnostic).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Mono = 0,
    Il2Cpp = 1,
}

impl RuntimeKind {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(RuntimeKind::Mono),
            1 => Some(RuntimeKind::Il2Cpp),
            _ => None,
        }
    }
}

/// Opaque managed-object handle. Identifies a Type / Object /
/// Field / Method on whichever runtime is active. The shim
/// keeps the managed reference in a `Dictionary<int, object>`;
/// Rust only sees the integer cookie. 0 means "null / invalid".
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MonoHandle(pub i32);

impl MonoHandle {
    pub const NULL: Self = Self(0);
    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// One Harmony patch handle. Returned by `harmony_patch_*`;
/// passed back to `harmony_unpatch`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatchHandle(pub i32);

/// The packed function-pointer table. Layout matches the C#
/// `[StructLayout(LayoutKind.Sequential)] BridgeTable`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BridgeTable {
    pub magic: u32,
    pub version: u32,
    /// Runtime backend tag. Set by the active shim. Read via
    /// [`runtime_kind`] from any backend-conditional code path.
    pub runtime_kind: u32,

    // ---- logging --------------------------------------------------------
    /// Log a line. `level` is one of: 0=trace, 1=debug, 2=info,
    /// 3=warn, 4=error. `msg_utf8` is a NUL-terminated UTF-8
    /// string; the shim copies it into BepInEx's log sink.
    pub log_emit: extern "C" fn(level: i32, msg_utf8: *const c_char),

    // ---- reflection (backend-neutral) -----------------------------------
    /// Find a Type by name. Backend-specific resolution: Mono
    /// uses `System.Type` lookup via the loaded assemblies; IL2CPP
    /// uses `Il2CppType.From`. Returns NULL on miss.
    pub find_type: extern "C" fn(name_utf8: *const c_char) -> MonoHandle,

    /// `Singleton<T>.Instance` for the given Type. NULL if T
    /// does not derive from a singleton template or the instance
    /// is null.
    pub singleton_instance: extern "C" fn(type_handle: MonoHandle) -> MonoHandle,

    /// `StaticInstance<T>.Instance` for the given Type. NULL if
    /// the pattern doesn't match.
    pub static_instance: extern "C" fn(type_handle: MonoHandle) -> MonoHandle,

    /// Walk a class and return every live instance as a JSON
    /// array of `{ "handle": i32, "name": str }`. Returns bytes
    /// written, or -1 if cap was too small.
    pub walk_class: extern "C" fn(
        type_handle: MonoHandle,
        include_inactive: i32,
        out_json_utf8: *mut c_char,
        cap: i32,
    ) -> i32,

    /// Dump an object's fields + property values as JSON.
    pub inspect_object: extern "C" fn(obj: MonoHandle, out_json_utf8: *mut c_char, cap: i32) -> i32,

    /// Read a typed field by name. The result kind is encoded
    /// in the JSON. Returns -1 on field-not-found.
    pub read_field: extern "C" fn(
        obj: MonoHandle,
        field_name_utf8: *const c_char,
        out_json_utf8: *mut c_char,
        cap: i32,
    ) -> i32,

    /// Write a field. Returns 0 on success, -1 on type mismatch,
    /// -2 on field-not-found.
    pub write_field: extern "C" fn(
        obj: MonoHandle,
        field_name_utf8: *const c_char,
        value_json_utf8: *const c_char,
    ) -> i32,

    /// Invoke a method by name. Returns 0 on success, -1 on
    /// method-not-found, -2 on arg-mismatch, -3 on exception
    /// (with the exception message written to out_json_utf8 as
    /// `{"error": "..."}`).
    pub invoke_method: extern "C" fn(
        obj: MonoHandle,
        method_name_utf8: *const c_char,
        args_json_utf8: *const c_char,
        out_json_utf8: *mut c_char,
        cap: i32,
    ) -> i32,

    /// Release a handle. Idempotent.
    pub release_handle: extern "C" fn(handle: MonoHandle),

    // ---- harmony --------------------------------------------------------
    /// Patch a prefix on `type::method`. HarmonyX works on both
    /// Mono and IL2CPP backends.
    pub harmony_patch_prefix: extern "C" fn(
        type_name_utf8: *const c_char,
        method_name_utf8: *const c_char,
        prefix_fn: extern "C" fn(*const c_void) -> i32,
    ) -> PatchHandle,

    /// Patch a postfix.
    pub harmony_patch_postfix: extern "C" fn(
        type_name_utf8: *const c_char,
        method_name_utf8: *const c_char,
        postfix_fn: extern "C" fn(*const c_void),
    ) -> PatchHandle,

    /// Remove a Harmony patch. Idempotent.
    pub harmony_unpatch: extern "C" fn(patch: PatchHandle),

    // ---- input (added in v3) -------------------------------------------
    /// Register a callback that fires on `Input.GetKeyDown(keycode)`.
    /// `keycode` is a UnityEngine.KeyCode integer (e.g. 32 = Space).
    /// `callback` is a `extern "C" fn()` invoked from the shim's
    /// Update on the Unity main thread when the key is first
    /// pressed each frame. Returns a binding handle (0 on
    /// failure; idempotent for the same callback).
    pub register_key_binding: extern "C" fn(keycode: i32, callback: extern "C" fn()) -> i32,

    /// Remove a key binding. Idempotent.
    pub unregister_key_binding: extern "C" fn(binding: i32),

    // ---- reflection (v4+) -----------------------------------------------
    /// Enumerate methods on a type by name. Walks the inheritance
    /// chain so methods declared on a base class (e.g.
    /// `Singleton<T>.Awake`) are included along with declared
    /// methods, each tagged with their declaring type. Result is
    /// JSON `{ "type": str, "methods": [{name, declared_on,
    /// params, static, return}] }`. Returns bytes written, or -1
    /// on cap-too-small.
    pub list_methods:
        extern "C" fn(type_name_utf8: *const c_char, out_json_utf8: *mut c_char, cap: i32) -> i32,

    // ---- harmony (v5+) ----------------------------------------------------
    /// Prefix patch whose callback receives a context object as a
    /// FRESH handle in the `*const c_void` parameter (cast the
    /// pointer value to i32). `ctx_kind`: 0 = `__instance`
    /// (instance methods only), 1 = `args[0]`. The callback OWNS
    /// the handle and must release it (wrap via
    /// `MonoObject::from_handle`; Drop releases). NULL context
    /// object arrives as 0. Non-zero return = skip the original,
    /// same as `harmony_patch_prefix`.
    ///
    /// ctx_kind 1 only supports a REFERENCE-type first argument:
    /// the C# dispatcher declares the Harmony indexed parameter
    /// `__0` as `object`, and Harmony 2.0.4 loads arguments as-is
    /// with no boxing (verified against MethodPatcher.cs at tag
    /// v2.0.4.0). A value-type first argument would produce
    /// invalid IL.
    pub harmony_patch_prefix_ctx: extern "C" fn(
        type_name_utf8: *const c_char,
        method_name_utf8: *const c_char,
        ctx_kind: i32,
        prefix_fn: extern "C" fn(*const c_void) -> i32,
    ) -> PatchHandle,

    // ---- v6 ---------------------------------------------------------
    /// Invoke a STATIC method on a named class. None when the
    /// running shim is pre-v6 (install() accepts a v5 table and
    /// leaves this tail None until the game restarts on the
    /// upgraded shim).
    pub invoke_static: Option<
        extern "C" fn(
            class_utf8: *const c_char,
            method_utf8: *const c_char,
            args_utf8: *const c_char,
            out_buf: *mut u8,
            cap: i32,
        ) -> i32,
    >,
}

static BRIDGE: OnceLock<BridgeTable> = OnceLock::new();

/// Install the bridge table (called from the C# shim via
/// `unityforge_init`). Verifies magic + version + runtime_kind;
/// refuses on mismatch. Idempotent.
pub fn install(bridge: *const BridgeTable) -> bool {
    if bridge.is_null() {
        return false;
    }
    // SAFETY: magic/version/runtime_kind live in the first 12
    // bytes of every table version; read them raw before deciding
    // how many bytes the shim actually allocated.
    let head = bridge as *const u32;
    let (magic, version, runtime_kind) = unsafe { (*head, *head.add(1), *head.add(2)) };
    if magic != BRIDGE_MAGIC {
        return false;
    }
    if version != BRIDGE_VERSION && version != BRIDGE_VERSION - 1 {
        return false;
    }
    if RuntimeKind::from_u32(runtime_kind).is_none() {
        return false;
    }
    let table = if version == BRIDGE_VERSION {
        // SAFETY: a current-version shim allocated the full table.
        unsafe { *bridge }
    } else {
        let mut mu = std::mem::MaybeUninit::<BridgeTable>::zeroed();
        // SAFETY: the previous version's table is a byte prefix of
        // this layout ending right before `invoke_static`; the
        // zeroed tail is a valid None for the Option fn pointer.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bridge as *const u8,
                mu.as_mut_ptr() as *mut u8,
                std::mem::offset_of!(BridgeTable, invoke_static),
            );
            mu.assume_init()
        }
    };
    BRIDGE.set(table).is_ok() || BRIDGE.get().is_some()
}

pub fn get() -> Option<&'static BridgeTable> {
    BRIDGE.get()
}

pub fn try_get() -> Result<&'static BridgeTable, String> {
    BRIDGE
        .get()
        .ok_or_else(|| "unityforge: bridge not installed (shim init failed)".to_string())
}

/// Active runtime backend. None before the shim has installed.
pub fn runtime_kind() -> Option<RuntimeKind> {
    BRIDGE
        .get()
        .and_then(|b| RuntimeKind::from_u32(b.runtime_kind))
}

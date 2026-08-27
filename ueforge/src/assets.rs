//! The asset registry: every asset the game ships, loaded or not.
//!
//! Walking GObjects only ever sees what is in memory right now,
//! which varies by where the player is: one probe of MISERY saw 5
//! wall meshes where another saw 12, and the registry reported
//! 55. Unreal keeps an index of everything cooked into the build,
//! so ask that instead of guessing from memory.
//!
//! Three engine APIs, no game specifics:
//!
//! ```text
//! AssetRegistryHelpers:GetAssetRegistry   the index
//! AssetRegistry:GetAssetsByClass          every asset of a class
//! KismetSystemLibrary:LoadAsset_Blocking  pull one into memory
//! ```
//!
//! Layouts are from the UE object dump:
//!
//! ```text
//! FAssetData, stride 0x68:
//!   PackageName 0x00, PackagePath 0x08, AssetName 0x10,
//!   AssetClass 0x18, AssetClassPath 0x20
//! GetAssetsByClass parms 0x28:
//!   ClassPathName 0x00 (FTopLevelAssetPath: package FName then
//!   asset FName), OutAssetData 0x10 (TArray),
//!   bSearchSubClasses 0x20, ReturnValue 0x21
//! LoadAsset_Blocking parms 0x30:
//!   Asset 0x00 (FSoftObjectPtr), ReturnValue 0x28
//! ```
//!
//! Every call here goes through ProcessEvent, so all of it is
//! game-thread only.

use std::ffi::c_void;
use std::time::Duration;

use crate::ue::{self, UObject, read_at};

/// One `FAssetData` entry in the array the registry returns.
pub mod offsets {
    pub const ASSET_DATA_STRIDE: usize = 0x68;
    pub const PACKAGE_NAME: usize = 0x00;
    pub const ASSET_NAME: usize = 0x10;
    /// `UObjectBase::NamePrivate`, how a UObject's own FName is
    /// reached without going through a string.
    pub const OBJECT_NAME: usize = 0x18;
}

/// A refusal threshold, not a limit: a plausible game has tens of
/// thousands of assets, so a larger count means the parm block
/// was misread rather than that the game is enormous.
const IMPLAUSIBLE_ASSET_COUNT: i32 = 500_000;

/// Long enough for a cold registry query over tens of thousands
/// of assets, and for a blocking asset load.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(20);

/// An FName as the engine stores it: comparison index then
/// number, read as one u64.
fn fname_at(base: *const u8, offset: usize) -> u64 {
    // SAFETY: caller guarantees base points at a live struct and
    // offset lands on an FName field.
    unsafe { read_at::<u64>(base, offset) }
}

fn fname_string(raw: u64) -> String {
    let Some(rt) = ue::try_runtime() else {
        return String::new();
    };
    // SAFETY: the resolver reads the engine's name pool, which is
    // valid for the process lifetime once the runtime is up.
    unsafe { rt.name_resolver.to_string(ue::FName::from_u64(raw)) }
}

/// The class path of a UClass, as the pair of FNames the registry
/// wants: the package it lives in, and its own name.
///
/// Taken off a live UClass so no FName has to be constructed from
/// a string, which would need the engine's name pool to already
/// hold it.
pub fn class_path(class_name: &str) -> Option<(u64, u64)> {
    let cls = ue::find_class_fast(class_name)?;
    let obj = cls.as_object();
    let asset = fname_at(obj.as_ptr(), offsets::OBJECT_NAME);
    // The OUTERMOST package: "/Script/Engine" for engine classes.
    let mut outer = obj.outer();
    let mut package = None;
    while let Some(o) = outer {
        package = Some(fname_at(o.as_ptr(), offsets::OBJECT_NAME));
        outer = o.outer();
    }
    Some((package?, asset))
}

/// The asset registry object. Game thread only.
pub fn registry() -> Option<&'static UObject> {
    let helpers = ue::find_class_fast("AssetRegistryHelpers")?;
    let func = helpers.get_function("AssetRegistryHelpers", "GetAssetRegistry")?;
    let cdo = helpers.class_default_object()?;
    let mut parms = [0u8; 0x10];
    // SAFETY: live CDO and function; GetAssetRegistry takes no
    // arguments and returns one object pointer.
    unsafe {
        cdo.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    let ptr = u64::from_le_bytes(parms[0x00..0x08].try_into().ok()?);
    if ptr == 0 {
        return None;
    }
    // SAFETY: the engine returned a live UObject pointer.
    unsafe { (ptr as *const UObject).as_ref() }
}

/// One asset the game ships.
#[derive(Debug, Clone)]
pub struct AssetEntry {
    /// Full package name, e.g. `/Game/.../Walls/SM_Wall_100x100`.
    /// This is what a soft object path wants, NOT the package
    /// PATH, which is only the directory.
    pub package: String,
    pub name: String,
    /// The raw FNames, which is what [`load_asset`] needs.
    pub package_fname: u64,
    pub name_fname: u64,
}

/// Every asset of a class the game ships, loaded or not.
/// Subclasses are included. Game thread only.
pub fn assets_of_class(class_name: &str) -> Result<Vec<AssetEntry>, String> {
    let reg = registry().ok_or("asset registry unavailable")?;
    let (pkg_fname, cls_fname) =
        class_path(class_name).ok_or_else(|| format!("class '{class_name}' not found"))?;
    let ar = ue::find_class_fast("AssetRegistry").ok_or("AssetRegistry class not found")?;
    let func = ar
        .get_function("AssetRegistry", "GetAssetsByClass")
        .ok_or("GetAssetsByClass not found")?;

    let mut parms = [0u8; 0x28];
    parms[0x00..0x08].copy_from_slice(&pkg_fname.to_le_bytes());
    parms[0x08..0x10].copy_from_slice(&cls_fname.to_le_bytes());
    parms[0x20] = 1; // include subclasses

    // SAFETY: reg is the live registry object; the parm block
    // matches the dumped GetAssetsByClass layout. The engine
    // allocates OutAssetData, which we only read.
    unsafe {
        reg.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }

    let data = u64::from_le_bytes(parms[0x10..0x18].try_into().unwrap_or_default());
    let num = i32::from_le_bytes(parms[0x18..0x1C].try_into().unwrap_or_default());
    if data == 0 || num <= 0 {
        return Ok(Vec::new());
    }
    if num > IMPLAUSIBLE_ASSET_COUNT {
        return Err(format!("implausible asset count {num}"));
    }

    let mut out = Vec::with_capacity(num as usize);
    for i in 0..num as usize {
        let entry = (data as usize + i * offsets::ASSET_DATA_STRIDE) as *const u8;
        let package_fname = fname_at(entry, offsets::PACKAGE_NAME);
        let name_fname = fname_at(entry, offsets::ASSET_NAME);
        out.push(AssetEntry {
            package: fname_string(package_fname),
            name: fname_string(name_fname),
            package_fname,
            name_fname,
        });
    }
    Ok(out)
}

/// Pull an asset into memory by its package and asset FNames, so
/// something the game has not loaded can still be used. Returns
/// the loaded object's address, or 0 if the load failed.
///
/// Blocking, and game thread only.
pub fn load_asset(package_fname: u64, asset_fname: u64) -> Result<u64, String> {
    let ksl = ue::find_class_fast("KismetSystemLibrary").ok_or("KismetSystemLibrary not found")?;
    let func = ksl
        .get_function("KismetSystemLibrary", "LoadAsset_Blocking")
        .ok_or("LoadAsset_Blocking not found")?;
    let cdo = ksl.class_default_object().ok_or("no CDO")?;

    // FSoftObjectPtr: a weak pointer, then the path (package
    // FName, asset FName, sub-path string).
    let mut parms = [0u8; 0x30];
    parms[0x08..0x10].copy_from_slice(&package_fname.to_le_bytes());
    parms[0x10..0x18].copy_from_slice(&asset_fname.to_le_bytes());
    // SAFETY: live CDO and function; the parm block matches the
    // dumped LoadAsset_Blocking layout.
    unsafe {
        cdo.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    Ok(u64::from_le_bytes(
        parms[0x28..0x30].try_into().unwrap_or_default(),
    ))
}

/// Register the standard asset inventory and loading operations.
/// Both operations enter Unreal through the canonical game-thread
/// runner because the HTTP server handles requests on worker threads.
pub fn register_ops() {
    crate::ops::OP_REGISTRY.register_many([
        crate::ops::OpDef::new(
            "asset_inventory",
            "Every asset of a class the game ships, loaded or not",
            "{class?: str, contains?: str}",
            inventory_op,
        ),
        crate::ops::OpDef::new(
            "load_asset",
            "Pull an asset into memory by its package and asset FNames",
            "{package_fname: u64, asset_fname: u64}",
            load_op,
        ),
    ]);
}

fn inventory_op(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let class = args
        .get("class")
        .and_then(|v| v.as_str())
        .unwrap_or("StaticMesh")
        .to_string();
    let filter = args
        .get("contains")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let class_for_job = class.clone();
    let rows = crate::game_thread::run(
        move || {
            let list = assets_of_class(&class_for_job)?;
            let total = list.len();
            let rows: Vec<serde_json::Value> = list
                .into_iter()
                .filter(|asset| {
                    filter.is_empty()
                        || asset.package.contains(&filter)
                        || asset.name.contains(&filter)
                })
                .map(|asset| {
                    serde_json::json!({
                        "package": asset.package,
                        "name": asset.name,
                        "package_fname": asset.package_fname,
                        "asset_fname": asset.name_fname,
                    })
                })
                .collect();
            Ok(serde_json::json!({ "total": total, "assets": rows }))
        },
        ENGINE_TIMEOUT,
    )?;

    Ok(serde_json::json!({
        "class": class,
        "total": rows["total"],
        "returned": rows["assets"].as_array().map(|assets| assets.len()).unwrap_or(0),
        "assets": rows["assets"],
    }))
}

fn load_op(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let package_fname = args
        .get("package_fname")
        .and_then(|v| v.as_u64())
        .ok_or("need {package_fname: u64}")?;
    let asset_fname = args
        .get("asset_fname")
        .and_then(|v| v.as_u64())
        .ok_or("need {asset_fname: u64}")?;
    crate::game_thread::run(
        move || {
            let address = load_asset(package_fname, asset_fname)?;
            Ok(serde_json::json!({
                "loaded": address != 0,
                "address": format!("{address:#x}"),
            }))
        },
        ENGINE_TIMEOUT,
    )
}

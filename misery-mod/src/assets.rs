//! The asset registry: every piece the game ships, loaded or not.
//!
//! Walking GObjects only ever sees what is in memory right now,
//! which varies by area: one probe saw 5 wall sizes where another
//! saw 12. Unreal keeps an index of every shipped asset, so ask
//! that instead.
//!
//! `AssetRegistryHelpers:GetAssetRegistry` returns the index;
//! `AssetRegistry:GetAssetsByClass` lists every asset of a class
//! whether or not it is loaded. `FAssetData` carries the package
//! and asset names, which are also what
//! `KismetSystemLibrary:LoadAsset_Blocking` needs to pull a piece
//! into memory on demand.
//!
//! Layouts, all from the object dump:
//! - FAssetData, stride 0x68: PackageName 0x00, PackagePath 0x08,
//!   AssetName 0x10, AssetClass 0x18, AssetClassPath 0x20.
//! - GetAssetsByClass parms 0x28: ClassPathName 0x00 (an
//!   FTopLevelAssetPath: package FName then asset FName),
//!   OutAssetData 0x10 (TArray), bSearchSubClasses 0x20,
//!   ReturnValue 0x21.

use std::ffi::c_void;

use ueforge::ue::{self, UObject, read_at};

/// One FAssetData entry.
const ASSET_DATA_STRIDE: usize = 0x68;
const AD_PACKAGE_NAME: usize = 0x00;
const AD_ASSET_NAME: usize = 0x10;

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
/// wants: the package it lives in, and its own name. Taken from a
/// live UClass so no FName has to be built from a string.
fn class_path(class_name: &str) -> Option<(u64, u64)> {
    let cls = ue::find_class_fast(class_name)?;
    let obj = cls.as_object();
    // The class's own name.
    let asset = fname_at(obj.as_ptr(), 0x18);
    // Its outermost package ("/Script/Engine" for engine classes).
    let mut outer = obj.outer();
    let mut package = None;
    while let Some(o) = outer {
        package = Some(fname_at(o.as_ptr(), 0x18));
        outer = o.outer();
    }
    Some((package?, asset))
}

/// The asset registry object.
fn registry() -> Option<&'static UObject> {
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

/// Every asset of a class the game ships, loaded or not.
/// Returns (package name, asset name) pairs plus their raw
/// FNames, which are what loading one needs.
pub fn assets_of_class(class_name: &str) -> Result<Vec<(String, String, u64, u64)>, String> {
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
    if num > 500_000 {
        return Err(format!("implausible asset count {num}"));
    }

    let mut out = Vec::with_capacity(num as usize);
    for i in 0..num as usize {
        let entry = (data as usize + i * ASSET_DATA_STRIDE) as *const u8;
        // A soft object path wants the full PACKAGE NAME
        // ("/Game/.../Walls/SM_Wall_100x100"), not the package
        // PATH ("/Game/.../Walls"), which is only the directory.
        let pkg = fname_at(entry, AD_PACKAGE_NAME);
        let name = fname_at(entry, AD_ASSET_NAME);
        out.push((fname_string(pkg), fname_string(name), pkg, name));
    }
    Ok(out)
}

/// Pull an asset into memory by its package and asset FNames, so
/// a piece that is not currently loaded can still be used.
/// Returns the loaded object's address.
pub fn load_asset(package_fname: u64, asset_fname: u64) -> Result<u64, String> {
    let ksl = ue::find_class_fast("KismetSystemLibrary")
        .ok_or("KismetSystemLibrary not found")?;
    let func = ksl
        .get_function("KismetSystemLibrary", "LoadAsset_Blocking")
        .ok_or("LoadAsset_Blocking not found")?;
    let cdo = ksl.class_default_object().ok_or("no CDO")?;

    // FSoftObjectPtr: weak pointer, then the path (package FName,
    // asset FName, sub-path string). Same 0x28 layout as the
    // world generator's level pool entries.
    let mut parms = [0u8; 0x30];
    parms[0x08..0x10].copy_from_slice(&package_fname.to_le_bytes());
    parms[0x10..0x18].copy_from_slice(&asset_fname.to_le_bytes());
    // SAFETY: live CDO and function; parm block matches the
    // dumped LoadAsset_Blocking layout (Asset 0x00, return 0x28).
    unsafe {
        cdo.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    Ok(u64::from_le_bytes(
        parms[0x28..0x30].try_into().unwrap_or_default(),
    ))
}

fn inventory_op(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let class = args
        .get("class")
        .and_then(|v| v.as_str())
        .unwrap_or("StaticMesh")
        .to_string();
    let filter = args.get("contains").and_then(|v| v.as_str()).unwrap_or("");
    // Same reasoning as load_op: query the registry on the game
    // thread rather than relying on it tolerating a worker.
    let assets = crate::dispatch::DRAIN
        .queue()
        .enqueue(
            move || {
                let list = assets_of_class(&class)?;
                serde_json::to_value(&list).map_err(|e| e.to_string())
            },
            std::time::Duration::from_secs(20),
        )
        .and_then(|v| {
            serde_json::from_value::<Vec<(String, String, u64, u64)>>(v)
                .map_err(|e| e.to_string())
        })?;
    let class = args
        .get("class")
        .and_then(|v| v.as_str())
        .unwrap_or("StaticMesh");
    let rows: Vec<serde_json::Value> = assets
        .iter()
        .filter(|(pkg, name, _, _)| {
            filter.is_empty() || pkg.contains(filter) || name.contains(filter)
        })
        .map(|(pkg, name, pkg_fname, asset_fname)| {
            serde_json::json!({
                "package": pkg,
                "name": name,
                "package_fname": pkg_fname,
                "asset_fname": asset_fname,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "class": class,
        "total": assets.len(),
        "returned": rows.len(),
        "assets": rows,
    }))
}

fn load_op(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let pkg = args
        .get("package_fname")
        .and_then(|v| v.as_u64())
        .ok_or("need {package_fname: u64}")?;
    let asset = args
        .get("asset_fname")
        .and_then(|v| v.as_u64())
        .ok_or("need {asset_fname: u64}")?;
    // Streaming an asset in touches engine state that is only
    // safe from the game thread; called from the HTTP worker it
    // returns null (research.md 26.1). Every other working
    // UFunction call in this mod goes through the drain, and so
    // does this one.
    crate::dispatch::DRAIN.queue().enqueue(
        move || {
            let addr = load_asset(pkg, asset)?;
            Ok(serde_json::json!({
                "loaded": addr != 0,
                "address": format!("{addr:#x}"),
            }))
        },
        std::time::Duration::from_secs(20),
    )
}

pub fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "asset_inventory",
            "Every asset of a class the game ships, loaded or not",
            "{class?: str, contains?: str}",
            inventory_op,
        ),
        ueforge::ops::OpDef::new(
            "load_asset",
            "Pull an asset into memory by its package and asset FNames",
            "{package_fname: u64, asset_fname: u64}",
            load_op,
        ),
    ]);
}

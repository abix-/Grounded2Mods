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

/// The parts list is the one op that loads every mesh the game
/// ships, one blocking load at a time, because a pivot only
/// exists on a loaded mesh. Two thousand of those do not fit in
/// [`ENGINE_TIMEOUT`], and the game is stopped for the whole
/// pass, which is expected and is why this is asked for by hand
/// rather than run on a timer.
const PARTS_LIST_TIMEOUT: Duration = Duration::from_secs(600);

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

/// The raw bytes of the first few `FAssetData` entries.
///
/// For reading what the registry actually carries per asset,
/// rather than assuming. `FAssetData` is 0x68 bytes and only its
/// first 0x28 are named in the dump we have; the rest should hold
/// `TagsAndValues`, the searchable metadata Unreal cooks in. If
/// that carries a static mesh's bounds, nothing needs loading to
/// measure it.
///
/// Read-only, and it reads only inside the array the registry
/// itself just returned.
///
/// Game thread only.
pub fn asset_data_bytes(class_name: &str, count: usize) -> Result<Vec<(String, String)>, String> {
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
    parms[0x20] = 1;
    // SAFETY: as `assets_of_class`, which this mirrors.
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

    let mut out = Vec::new();
    for i in 0..(num as usize).min(count) {
        let entry = (data as usize + i * offsets::ASSET_DATA_STRIDE) as *const u8;
        // SAFETY: inside the array the registry returned, and
        // within one entry's own stride.
        let raw = unsafe { std::slice::from_raw_parts(entry, offsets::ASSET_DATA_STRIDE) };
        let name = fname_string(fname_at(entry, offsets::ASSET_NAME));
        out.push((
            name,
            raw.iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    Ok(out)
}

/// Read one cooked registry TAG off an asset, without loading it.
///
/// `AssetRegistryHelpers::GetTagValue(FAssetData, FName TagName,
/// FString& OutValue) -> bool`, 4 parms in 129 bytes:
///
/// ```text
/// 0x00  FAssetData    the whole 0x68-byte entry, BY VALUE
/// 0x68  FName         the tag to ask for
/// 0x70  FString       the answer, written by the engine
/// 0x80  bool          whether the tag was there
/// ```
///
/// The tag name has to be an `FName`, which is why this needs
/// `fname::from_str` and why it was out of reach until that
/// landed (research.md 28).
///
/// `None` means the asset carries no such tag. Game thread only.
pub fn asset_tag(entry: *const u8, tag: &str) -> Option<String> {
    let helpers = ue::find_class_fast("AssetRegistryHelpers")?;
    let func = helpers.get_function("AssetRegistryHelpers", "GetTagValue")?;
    let cdo = helpers.class_default_object()?;
    let name = ue::fname::from_str(tag, ue::fname::FindName::Find)?;

    let mut parms = [0u8; 0x81];
    // SAFETY: `entry` is one FAssetData inside the array the
    // registry returned, so a full stride is readable.
    unsafe {
        std::ptr::copy_nonoverlapping(entry, parms.as_mut_ptr(), offsets::ASSET_DATA_STRIDE);
    }
    parms[0x68..0x70].copy_from_slice(&name.as_u64().to_le_bytes());

    // SAFETY: live CDO and function, and the parm block matches
    // the dumped layout above.
    unsafe {
        cdo.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    if parms[0x80] == 0 {
        return None;
    }
    // The answer is an FString: { TCHAR* Data; int32 Num; int32 Max }.
    let ptr = u64::from_le_bytes(parms[0x70..0x78].try_into().ok()?);
    let num = i32::from_le_bytes(parms[0x78..0x7C].try_into().ok()?);
    if ptr == 0 || num <= 0 {
        return None;
    }
    // SAFETY: the engine wrote this FString; its own length says
    // how much is readable.
    let units = unsafe { std::slice::from_raw_parts(ptr as *const u16, num as usize) };
    Some(
        String::from_utf16_lossy(units)
            .trim_end_matches(' ')
            .to_string(),
    )
}

/// Every tag asked for, on the first few assets of a class.
///
/// Read-only, and it loads nothing. Game thread only.
pub fn asset_tags(
    class_name: &str,
    tags: &[String],
    count: usize,
) -> Result<Vec<(String, Vec<(String, Option<String>)>)>, String> {
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
    parms[0x20] = 1;
    // SAFETY: as `assets_of_class`, which this mirrors.
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

    let mut out = Vec::new();
    for i in 0..(num as usize).min(count) {
        let entry = (data as usize + i * offsets::ASSET_DATA_STRIDE) as *const u8;
        let name = fname_string(fname_at(entry, offsets::ASSET_NAME));
        let values = tags
            .iter()
            .map(|t| (t.clone(), asset_tag(entry, t)))
            .collect();
        out.push((name, values));
    }
    Ok(out)
}

/// Parse Unreal's `ApproxSize` tag: `"320x320x320"`, a FULL size
/// in centimetres, on Unreal's axes.
///
/// A zero is meaningful. `SM_MediaPlateScreen` is `0x100x100`,
/// which is a flat mesh, not a missing measurement.
pub fn parse_approx_size(text: &str) -> Option<(f64, f64, f64)> {
    let mut parts = text.split('x').map(|p| p.trim().parse::<f64>());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Ok(x)), Some(Ok(y)), Some(Ok(z))) => Some((x, y, z)),
        _ => None,
    }
}

/// Unreal's axes and centimetres to this crate's: metres, y up.
/// `mf(x, y, z) = ue(y, z, x) / 100`, the same permutation
/// `ue::parts` uses.
fn ue_to_mf(v: (f64, f64, f64)) -> glam::Vec3 {
    glam::Vec3::new(
        (v.1 / CM_PER_M) as f32,
        (v.2 / CM_PER_M) as f32,
        (v.0 / CM_PER_M) as f32,
    )
}

/// One entry in the parts list: what a mesh is, without loading
/// it.
pub struct Part {
    pub name: String,
    pub package: String,
    /// Half-size in this crate's convention: metres, y up.
    pub extent: glam::Vec3,
    /// Where the bounding box's centre sits relative to the
    /// mesh's PIVOT, metres, y up. The part's faces run from
    /// `pivot - extent` to `pivot + extent`, which is what
    /// deciding whether two placed parts touch needs.
    ///
    /// `None` when the mesh was not in memory. The registry tags
    /// carry a size and no pivot, so only a loaded mesh can
    /// answer this.
    pub pivot: Option<glam::Vec3>,
    pub shape: modforge::structure::PartShape,
    pub triangles: Option<u32>,
    pub vertices: Option<u32>,
    pub materials: Option<u32>,
    pub lods: Option<u32>,
}

/// Every mesh the game ships, as a part: size, shape and counts,
/// read from the cooked registry tags with NOTHING loaded.
///
/// The size comes from `ApproxSize`, which is approximate and
/// rounded to whole units. That is enough to tell a 4 m wall from
/// a 2 m one, which is what a parts list is for; anything needing
/// exact bounds loads that one mesh (parts.md).
///
/// Game thread only.
pub fn parts_list(class_name: &str) -> Result<Vec<Part>, String> {
    const APPROX_SIZE: &str = "ApproxSize";
    let wanted: Vec<String> = ["ApproxSize", "Triangles", "Vertices", "Materials", "LODs"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let entries = assets_of_class(class_name)?;
    let mut out = Vec::with_capacity(entries.len());
    let rows = asset_tags(class_name, &wanted, usize::MAX)?;
    for (name, tags) in rows {
        let get = |key: &str| -> Option<String> {
            tags.iter()
                .find(|(t, _)| t == key)
                .and_then(|(_, v)| v.clone())
        };
        let num = |key: &str| -> Option<u32> { get(key).and_then(|v| v.parse().ok()) };

        // Unreal's axes and centimetres to this crate's: metres,
        // y up, and HALF the size because `PartDef::extent` is a
        // half-extent. The permutation is the same one
        // `ue::parts` uses (mf x,y,z = ue y,z,x).
        let extent = match get(APPROX_SIZE).as_deref().and_then(parse_approx_size) {
            Some(size) => ue_to_mf(size) / 2.0,
            None => glam::Vec3::ZERO,
        };
        let package = entries
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.package.clone())
            .unwrap_or_default();
        out.push(Part {
            name,
            package,
            extent,
            pivot: None,
            shape: modforge::structure::classify(extent),
            triangles: num("Triangles"),
            vertices: num("Vertices"),
            materials: num("Materials"),
            lods: num("LODs"),
        });
    }

    // The pivot lives on the loaded mesh, so every pivot means a
    // load. ONE way of getting one: `LoadAsset_Blocking` hands
    // back a mesh that is already in memory without reloading it,
    // so asking it for all of them covers both cases and there is
    // no second path to disagree with the first.
    //
    // This is the expensive half: a blocking load per mesh, on
    // the game thread, where the registry pass before it loaded
    // nothing at all. The game stops for the whole pass, which is
    // why the op that calls this has a timeout of its own.
    for part in out.iter_mut() {
        let Some(entry) = entries.iter().find(|a| a.name == part.name) else {
            continue;
        };
        let addr = load_asset(entry.package_fname, entry.name_fname)?;
        if addr == 0 {
            continue;
        }
        // SAFETY: the entry is a StaticMesh asset, so what
        // LoadAsset_Blocking returned is a live UStaticMesh.
        let (origin, _) = unsafe { crate::ue::transform::mesh_bounds(addr as *const u8) };
        part.pivot = Some(ue_to_mf(origin));
    }
    Ok(out)
}

/// Centimetres per metre, as `ue::parts` uses it.
const CM_PER_M: f64 = 100.0;

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
            "parts_list",
            "Every mesh the game ships as a part: size, shape, pivot and counts. Sizes come from the registry, but a pivot only exists on a loaded mesh, so this loads every one of them and the game stops until it finishes. Writes it to disk when given a path",
            "{class?: str, path?: str}",
            |args| {
                let class = args
                    .get("class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("StaticMesh")
                    .to_string();
                let path = args.get("path").and_then(|v| v.as_str()).map(str::to_string);
                crate::game_thread::run(
                    move || {
                        let parts = parts_list(&class)?;
                        let rows: Vec<serde_json::Value> = parts
                            .iter()
                            .map(|p| {
                                serde_json::json!({
                                    "name": p.name,
                                    "package": p.package,
                                    // Half-size, metres, y up.
                                    "extent": [p.extent.x, p.extent.y, p.extent.z],
                                    // Box centre from the pivot,
                                    // metres, y up. Null when the
                                    // mesh was not loaded.
                                    "pivot": p.pivot.map(|v| vec![v.x, v.y, v.z]),
                                    "shape": format!("{:?}", p.shape),
                                    "triangles": p.triangles,
                                    "vertices": p.vertices,
                                    "materials": p.materials,
                                    "lods": p.lods,
                                })
                            })
                            .collect();
                        let doc = serde_json::json!({
                            "class": class,
                            "count": rows.len(),
                            "units": "half-extent and pivot in metres, y up",
                            "source": "size from the ApproxSize registry tag with nothing loaded; pivot from ExtendedBounds on the meshes that were already in memory",
                            // Said out loud, because a null pivot
                            // is the difference between a part
                            // that can be placed against another
                            // and one that cannot.
                            "with_pivot": parts.iter().filter(|p| p.pivot.is_some()).count(),
                            "no_pivot": parts.iter().filter(|p| p.pivot.is_none()).count(),
                            "parts": rows,
                        });
                        // Written where the caller asks, because
                        // the point of this file is that a person
                        // opens it.
                        let written = match &path {
                            Some(p) => match std::fs::write(
                                p,
                                serde_json::to_string_pretty(&doc).unwrap_or_default(),
                            ) {
                                Ok(()) => Some(p.clone()),
                                Err(e) => return Err(format!("could not write {p}: {e}")),
                            },
                            None => None,
                        };
                        Ok(serde_json::json!({
                            "class": doc["class"],
                            "count": doc["count"],
                            "with_pivot": doc["with_pivot"],
                            "no_pivot": doc["no_pivot"],
                            "written_to": written,
                            // The whole list only comes back when
                            // it is not being written; it is large.
                            "parts": if written.is_some() { serde_json::Value::Null } else { doc["parts"].clone() },
                        }))
                    },
                    PARTS_LIST_TIMEOUT,
                )
            },
        ),
        crate::ops::OpDef::new(
            "asset_tags",
            "Cooked registry tag values for the first few assets of a class, WITHOUT loading them",
            "{class?: str, tags: [str], count?: u64}",
            |args| {
                let class = args
                    .get("class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("StaticMesh")
                    .to_string();
                let tags: Vec<String> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                if tags.is_empty() {
                    return Err("need {tags: [str]}".to_string());
                }
                let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
                crate::game_thread::run(
                    move || {
                        let rows = asset_tags(&class, &tags, count)?;
                        Ok(serde_json::json!({
                            "assets": rows
                                .iter()
                                .map(|(name, values)| serde_json::json!({
                                    "name": name,
                                    "tags": values
                                        .iter()
                                        .map(|(t, v)| (t.clone(), serde_json::json!(v)))
                                        .collect::<serde_json::Map<_, _>>(),
                                }))
                                .collect::<Vec<_>>(),
                        }))
                    },
                    ENGINE_TIMEOUT,
                )
            },
        ),
        crate::ops::OpDef::new(
            "asset_data_bytes",
            "Raw FAssetData bytes for the first few assets of a class, to see what metadata the registry carries",
            "{class?: str, count?: u64}",
            |args| {
                let class = args
                    .get("class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("StaticMesh")
                    .to_string();
                let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
                let rows = crate::game_thread::run(
                    move || {
                        let entries = asset_data_bytes(&class, count)?;
                        Ok(serde_json::json!(
                            entries
                                .iter()
                                .map(|(name, hex)| serde_json::json!({
                                    "name": name,
                                    "bytes": hex,
                                }))
                                .collect::<Vec<_>>()
                        ))
                    },
                    ENGINE_TIMEOUT,
                )?;
                Ok(serde_json::json!({ "stride": offsets::ASSET_DATA_STRIDE, "assets": rows }))
            },
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

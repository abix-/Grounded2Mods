//! UE introspection helpers: GObjects population summaries +
//! Outer-chain sampling. Pure engine traversal, useful in any
//! UE mod for diagnosing leaks ("which package is growing?",
//! "where in the asset tree do these instances live?").

use std::collections::HashMap;

use serde_json::{Value as Json, json};

use crate::ue::offsets;
use crate::ue::{self, UObject};

/// Walk GObjects once and return a population summary:
///
/// - total entries / valid UObjects
/// - top `top_n` classes by instance count
/// - top `top_n` root-packages by hosted-object count
///   (tells you WHICH content path holds the leaking objects,
///   not just what class they are)
/// - loaded levels (`Level`, `LevelStreaming*`,
///   `WorldPartitionRuntime*`) with name + root package
///
/// Cost: O(N) over GObjects + O(D) outer-chain walk per object
/// (D ~= 3-5). One pass; no extra walks.
pub fn gobjects_population(top_n: usize) -> Json {
    let Some(rt) = ue::try_runtime() else {
        return json!({"error": "ueforge: ue runtime not initialized"});
    };
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return json!({"error": "gobjects view invalid"});
    }

    let mut total = 0usize;
    let mut valid = 0usize;
    let mut by_class: HashMap<String, usize> = HashMap::new();
    let mut by_package: HashMap<String, usize> = HashMap::new();
    let mut levels: Vec<Json> = Vec::new();

    for obj in view.iter() {
        total += 1;
        valid += 1;
        let class_name = obj
            .class()
            .map(|c| c.as_object().name())
            .unwrap_or_else(|| String::from("<no-class>"));
        *by_class.entry(class_name.clone()).or_insert(0) += 1;

        let root_name = root_package_name(obj);
        *by_package.entry(root_name.clone()).or_insert(0) += 1;

        if levels.len() < 200 && is_level_class(&class_name) {
            levels.push(json!({
                "class": class_name,
                "name": obj.name(),
                "package": root_name,
            }));
        }
    }

    let top_classes = top_n_by_count(by_class, top_n, "class", "count");
    let top_packages = top_n_by_count(by_package, top_n, "package", "hosted_count");

    json!({
        "gobjects_total": total,
        "gobjects_valid": valid,
        "top_classes": top_classes,
        "top_packages": top_packages,
        "loaded_levels": levels,
    })
}

/// For up to `k` instances of the named class, walk the Outer
/// chain and return the chain as a `"Root.Outer.Outer.Name"`
/// string. Used to find WHICH SoundWave / WHICH Package is
/// growing. "SoundWave +144" is a class delta; this turns it
/// into specific asset paths.
///
/// `class_name` matches the short class name (e.g. `"SoundWave"`).
/// Cost: O(N) over GObjects (continues counting matches after
/// `k` samples for accurate `matched_count`).
pub fn class_outer_samples(class_name: &str, k: usize) -> Json {
    let Some(rt) = ue::try_runtime() else {
        return json!({"error": "ueforge: ue runtime not initialized"});
    };
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return json!({"error": "gobjects view invalid"});
    }

    let mut samples: Vec<Json> = Vec::new();
    let mut scanned = 0usize;
    let mut matched = 0usize;
    for obj in view.iter() {
        scanned += 1;
        let cls = match obj.class() {
            Some(c) => c.as_object().name(),
            None => continue,
        };
        if cls != class_name {
            continue;
        }
        matched += 1;
        if samples.len() >= k {
            continue;
        }
        let (full_path, depth) = outer_chain_path(obj);
        samples.push(json!({
            "full_path": full_path,
            "name": obj.name(),
            "depth": depth,
        }));
    }

    json!({
        "class": class_name,
        "scanned_objects": scanned,
        "matched_count": matched,
        "samples_returned": samples.len(),
        "samples": samples,
    })
}

/// Minimal eager describe for a UDataTable. Just the short
/// name. The schema walk (`walk_struct_fields` on the RowStruct)
/// moves to the on-demand path (`dump_data_table`): we've seen
/// OWS instances whose RowStruct FField chain has a corrupt
/// FName that AVs `AppendString` during eager refresh.
pub fn describe_data_table(obj: &UObject) -> Json {
    let table_name = obj.name();
    json!({
        "id": table_name.to_lowercase(),
        "table_name": table_name,
    })
}

/// Walk `ChildProperties` on any UStruct-derived UObject
/// (UScriptStruct / UClass / UFunction) and emit one JSON entry
/// per FProperty. Mirrors `UClass::iter_native_properties` but
/// takes a bare UObject so it works on `UScriptStruct` (the row
/// struct of a UDataTable) without a transmute at the call site.
pub fn walk_struct_fields(struct_obj: &UObject) -> Vec<Json> {
    let Some(rt) = ue::try_runtime() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let head_addr = struct_obj.as_ptr() as usize + offsets::ustruct::CHILD_PROPERTIES;
    if !crate::winproc::is_addr_readable(head_addr) {
        return out;
    }
    let mut cur: *const u8 = unsafe { (head_addr as *const *const u8).read_unaligned() };
    let mut depth = 0;
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::with_capacity(64);
    while !cur.is_null() && depth < 4096 {
        // Validate that the entire FField head is in mapped memory
        // before any read. The chain walk hits stub / freed nodes
        // in late content-loaded GObjects on some games.
        if !crate::winproc::is_addr_readable(cur as usize + offsets::ffield::SIZE - 1) {
            break;
        }
        // Cycle detection: a corrupt RowStruct on OWS produces a
        // Next chain that points back into the FFieldClass list /
        // self-loops. Break the moment we revisit a node.
        if !seen.insert(cur as usize) {
            break;
        }
        unsafe {
            let name_ptr = cur.add(offsets::ffield::NAME_PRIVATE);
            let name_fname: crate::ue::fname::FName =
                (name_ptr as *const crate::ue::fname::FName).read_unaligned();
            let name = if name_fname.is_none() {
                String::from("<none>")
            } else {
                rt.name_resolver.to_string(name_fname)
            };
            let offset =
                (cur.add(offsets::fproperty::OFFSET_INTERNAL) as *const i32).read_unaligned();
            let element_size =
                (cur.add(offsets::fproperty::ELEMENT_SIZE) as *const i32).read_unaligned();
            let class = read_ffield_class_name(cur, rt);
            let next: *const u8 =
                (cur.add(offsets::ffield::NEXT) as *const *const u8).read_unaligned();
            out.push(json!({
                "name": name,
                "class": class,
                "offset": offset,
                "element_size": element_size,
            }));
            cur = next;
        }
        depth += 1;
    }
    out
}

/// Read the `FFieldClass::Name` for the FField at `field_ptr`.
/// `FField::ClassPrivate` (+0x08) points at an `FFieldClass` whose
/// first member is `FName Name`. The property class identifier
/// ("IntProperty" / "FloatProperty" / "NameProperty" / etc.).
///
/// VirtualQuery-guards every dereference: corrupt / unmapped
/// `ClassPrivate` pointers show up in UE5 builds during the
/// discovery walk (typically after engine teardown of placeholder
/// FProperty entries). Bailing to "<unreadable>" keeps the walk
/// going instead of taking the game with us.
unsafe fn read_ffield_class_name(field_ptr: *const u8, rt: &ue::Runtime) -> String {
    if !crate::winproc::is_addr_readable(field_ptr as usize + offsets::ffield::CLASS_PRIVATE) {
        return String::from("<unreadable>");
    }
    unsafe {
        let class_ptr: *const u8 =
            (field_ptr.add(offsets::ffield::CLASS_PRIVATE) as *const *const u8).read_unaligned();
        if class_ptr.is_null() {
            return String::from("<no-class>");
        }
        if !crate::winproc::is_addr_readable(class_ptr as usize) {
            return String::from("<unreadable>");
        }
        let fname: crate::ue::fname::FName =
            (class_ptr as *const crate::ue::fname::FName).read_unaligned();
        if fname.is_none() {
            String::from("<unnamed>")
        } else {
            rt.name_resolver.to_string(fname)
        }
    }
}

fn root_package_name(obj: &UObject) -> String {
    let mut cur = obj.outer();
    let mut depth = 0;
    while let Some(o) = cur {
        if depth > 16 {
            break;
        }
        let next = o.outer();
        if next.is_none() {
            return o.name();
        }
        cur = next;
        depth += 1;
    }
    String::from("<no-package>")
}

fn outer_chain_path(obj: &UObject) -> (String, usize) {
    let mut parts: Vec<String> = Vec::new();
    parts.push(obj.name());
    let mut cur = obj.outer();
    let mut depth = 0;
    while let Some(o) = cur {
        if depth > 16 {
            break;
        }
        parts.push(o.name());
        cur = o.outer();
        depth += 1;
    }
    parts.reverse();
    (parts.join("."), depth + 1)
}

fn is_level_class(name: &str) -> bool {
    name == "Level"
        || name.starts_with("LevelStreaming")
        || name.starts_with("WorldPartitionRuntime")
}

fn top_n_by_count(
    map: HashMap<String, usize>,
    n: usize,
    name_key: &'static str,
    count_key: &'static str,
) -> Vec<Json> {
    let mut v: Vec<(String, usize)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.into_iter()
        .take(n)
        .map(|(name, count)| json!({ name_key: name, count_key: count }))
        .collect()
}

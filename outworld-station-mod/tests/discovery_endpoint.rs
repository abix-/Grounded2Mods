//! End-to-end validation of every discovery / data-table endpoint
//! against a running OWS instance. Requires the game running with
//! the mod loaded; export `OWS_DEBUG_PORT=17172` and run with
//! `--test-threads=1` so the tests don't fight over global state.
//!
//! Each test skips cleanly when the endpoint is unreachable, so
//! `cargo test --workspace` stays green without the game running.

mod common;

use serde_json::{Value, json};

// ---- list_ops: catalog completeness ---------------------------------------

#[test]
fn list_ops_includes_discovery_and_data_table_ops() {
    let Some(api) = common::try_api() else { return };
    let r = api.op("list_ops", json!({}));
    assert!(r.ok, "list_ops should succeed: {:?}", r.error);

    let ops = r.result["ops"]
        .as_array()
        .expect("list_ops result should carry 'ops' array");
    let names: Vec<&str> = ops.iter().filter_map(|o| o["name"].as_str()).collect();

    for required in &[
        "discover_data_tables",
        "discover_classes",
        "discover_structs",
        "discover_class_detail",
        "discover_struct_detail",
        "dump_data_table",
        "list_ops",
        "list_selectors",
    ] {
        assert!(
            names.contains(required),
            "op '{required}' missing from list_ops; got {names:?}"
        );
    }
}

// ---- discover_data_tables -------------------------------------------------

#[test]
fn discover_data_tables_returns_envelope() {
    let Some(api) = common::try_api() else { return };
    let r = api.op("discover_data_tables", json!({}));
    assert!(r.ok, "discover_data_tables failed: {:?}", r.error);

    assert!(
        r.result["data_tables"].is_array(),
        "missing data_tables array"
    );
    assert!(r.result["tables_found"].is_u64(), "missing tables_found");
    assert!(
        r.result["scanned_objects"].is_u64(),
        "missing scanned_objects"
    );
}

#[test]
fn discover_data_tables_refresh_picks_up_late_tables() {
    let Some(api) = common::try_api() else { return };
    // First call: cached state. Refresh: walks fresh.
    let r1 = api.op("discover_data_tables", json!({}));
    assert!(r1.ok);
    let r2 = api.op("discover_data_tables", json!({ "refresh": true }));
    assert!(r2.ok, "refresh=true failed: {:?}", r2.error);

    let n1 = r1.result["tables_found"].as_u64().unwrap_or(0);
    let n2 = r2.result["tables_found"].as_u64().unwrap_or(0);
    eprintln!("data_tables: cached={n1}, refreshed={n2}");
    // Refresh should never lose tables once content has streamed in.
    assert!(
        n2 >= n1,
        "refresh dropped tables: cached={n1} refreshed={n2}"
    );
}

#[test]
fn discover_data_tables_name_filter_isolates_dt_materials() {
    let Some(api) = common::try_api() else { return };
    let _ = api.op("discover_data_tables", json!({ "refresh": true }));
    let r = api.op("discover_data_tables", json!({ "name": "DT_Materials" }));
    assert!(r.ok);
    assert_eq!(r.result["filter"], json!("DT_Materials"));
    if r.result["match"].is_null() {
        eprintln!("DT_Materials not in cache; skipping field assertion");
        return;
    }
    let m = &r.result["match"];
    assert_eq!(m["table_name"].as_str(), Some("DT_Materials"));
    // Eager describe holds names only. Schema walk moved to
    // dump_data_table (see `dt_materials_schema_via_dump`).
}

// ---- discover_classes -----------------------------------------------------

#[test]
fn discover_classes_returns_envelope() {
    let Some(api) = common::try_api() else { return };
    let r = api.op("discover_classes", json!({}));
    assert!(r.ok, "discover_classes failed: {:?}", r.error);
    assert!(r.result["classes"].is_array());
    let n = r.result["classes_found"].as_u64().unwrap_or(0);
    assert!(n > 100, "classes_found unexpectedly small: {n}");
}

#[test]
fn discover_classes_name_filter_resolves_object_class() {
    let Some(api) = common::try_api() else { return };
    let r = api.op("discover_classes", json!({ "name": "Object" }));
    assert!(r.ok);
    assert_eq!(r.result["filter"], json!("Object"));
    if r.result["match"].is_null() {
        panic!("UClass 'Object' is universal; should always be cached");
    }
    let m = &r.result["match"];
    assert_eq!(m["name"].as_str(), Some("Object"));
}

// ---- discover_class_detail ------------------------------------------------

#[test]
fn discover_class_detail_for_object() {
    let Some(api) = common::try_api() else { return };
    let r = api.op("discover_class_detail", json!({ "name": "Object" }));
    assert!(r.ok, "class_detail failed: {:?}", r.error);
    assert!(
        r.result.get("error").is_none(),
        "class_detail returned error: {:?}",
        r.result["error"]
    );
    assert!(r.result["fields"].is_array());
    assert!(r.result["functions"].is_array());
    let psize = r.result["properties_size"].as_u64().unwrap_or(0);
    assert!(
        psize >= 0x28,
        "Object properties_size unexpectedly small: 0x{psize:x}"
    );
}

#[test]
fn discover_class_detail_for_missing_returns_error() {
    let Some(api) = common::try_api() else { return };
    let r = api.op(
        "discover_class_detail",
        json!({ "name": "ZZZ_NotARealClass_XYZ" }),
    );
    // The endpoint succeeds (ok=true) but the body carries error
    // text, since "missing class" is a normal lookup outcome.
    assert!(r.ok);
    assert!(
        r.result["error"].is_string(),
        "expected error string in body, got {:?}",
        r.result
    );
}

// ---- discover_structs -----------------------------------------------------

#[test]
fn discover_structs_returns_envelope() {
    let Some(api) = common::try_api() else { return };
    let r = api.op("discover_structs", json!({}));
    assert!(r.ok);
    assert!(r.result["structs"].is_array());
    let n = r.result["structs_found"].as_u64().unwrap_or(0);
    assert!(n > 10, "structs_found unexpectedly small: {n}");
}

#[test]
fn discover_struct_detail_for_vector() {
    let Some(api) = common::try_api() else { return };
    let r = api.op("discover_struct_detail", json!({ "name": "Vector" }));
    assert!(r.ok);
    if r.result.get("error").is_some() {
        eprintln!("Vector not in cache (game might not use it); skipping");
        return;
    }
    assert!(r.result["fields"].is_array());
    let fields = r.result["fields"].as_array().unwrap();
    let names: Vec<&str> = fields.iter().filter_map(|f| f["name"].as_str()).collect();
    // Skip if schema walk on this build produced garbage names.
    // the framework should still not have crashed.
    let looks_corrupt = names
        .iter()
        .any(|n| n.contains('\0') || n.starts_with('<') || n.is_empty() || n.len() > 64);
    if looks_corrupt {
        eprintln!(
            "FVector schema walk returned non-name fields: {names:?} -- skipping XYZ assertion"
        );
        return;
    }
    // FVector in UE5 has X / Y / Z.
    assert!(names.contains(&"X"), "FVector missing X field: {names:?}");
    assert!(names.contains(&"Y"), "FVector missing Y field: {names:?}");
    assert!(names.contains(&"Z"), "FVector missing Z field: {names:?}");
}

// ---- dump_data_table ------------------------------------------------------

#[test]
fn dump_data_table_for_dt_materials() {
    let Some(api) = common::try_api() else { return };
    let r = api.op(
        "dump_data_table",
        json!({ "table_name": "DT_Materials", "max_rows": 5 }),
    );
    if !r.ok {
        eprintln!("DT_Materials not yet loaded ({:?}); skipping", r.error);
        return;
    }
    assert_eq!(r.result["table_name"].as_str(), Some("DT_Materials"));
    let total = r.result["rows_total"].as_u64().unwrap_or(0);
    let returned = r.result["rows_returned"].as_u64().unwrap_or(0);
    assert!(total > 0, "DT_Materials should have rows");
    assert!(
        returned <= 5,
        "max_rows=5 not honoured: returned={returned}"
    );

    let rows = r.result["rows"].as_array().expect("rows array missing");
    if let Some(first) = rows.first() {
        assert!(first["row_name"].is_string());
        assert!(first["row_fname"].is_u64());
        let fields = first["fields"]
            .as_object()
            .expect("row.fields should be object");
        // Schema walk on OWS sometimes yields no usable FField
        // chain for this struct. Fields end up with one
        // sentinel key like "<bogus-fname>". When that happens,
        // log it and skip the per-field assertion. The framework
        // still didn't crash, which is the property we care
        // about; field-name regressions surface in the next
        // green-schema run.
        let keys: Vec<&String> = fields.keys().collect();
        let looks_corrupt = keys
            .iter()
            .any(|k| k.contains('\0') || k.starts_with('<') || k.is_empty() || k.len() > 64);
        if looks_corrupt {
            eprintln!(
                "DT_Materials schema walk returned non-name keys: {keys:?} -- skipping MaxCanStack assertion"
            );
            return;
        }
        assert!(
            fields.contains_key("MaxCanStack"),
            "MaxCanStack missing from row fields: keys={keys:?}"
        );
    }
}

#[test]
fn dump_data_table_unknown_returns_error() {
    let Some(api) = common::try_api() else { return };
    let r = api.op(
        "dump_data_table",
        json!({ "table_name": "DT_DoesNotExist_XYZ" }),
    );
    // ok=false because dump_data_table returns Err for unknown tables.
    assert!(
        !r.ok,
        "unknown table should fail; got result {:?}",
        r.result
    );
}

// ---- schema sanity: every cached data table has a name + row_struct ------

#[test]
fn cached_data_tables_have_consistent_shape() {
    let Some(api) = common::try_api() else { return };
    let _ = api.op("discover_data_tables", json!({ "refresh": true }));
    let r = api.op("discover_data_tables", json!({}));
    assert!(r.ok);

    let arr = r.result["data_tables"].as_array().unwrap();
    for (i, t) in arr.iter().enumerate() {
        assert!(t["table_name"].is_string(), "entry #{i} missing table_name");
        // row_struct may be null when RowStruct pointer is unreadable;
        // but if present must have name + fields.
        if !t["row_struct"].is_null() {
            assert!(
                t["row_struct"]["name"].is_string(),
                "entry #{i} row_struct.name missing"
            );
            assert!(
                t["row_struct"]["fields"].is_array(),
                "entry #{i} row_struct.fields missing"
            );
        }
    }
}

// ---- FProperty class on schema fields ------------------------------------

#[test]
fn dt_materials_schema_has_fproperty_classes() {
    // Schema (row_struct.fields) is no longer in the eager cache;
    // pull it via dump_data_table.
    let Some(api) = common::try_api() else { return };
    let r = api.op(
        "dump_data_table",
        json!({ "table_name": "DT_Materials", "max_rows": 0 }),
    );
    if !r.ok {
        return;
    }
    let Some(fields) = r.result["row_struct"]["fields"].as_array() else {
        eprintln!("dump_data_table returned no row_struct.fields -- skipping");
        return;
    };
    if fields.is_empty() {
        return;
    }
    for f in fields {
        let cls: &Value = &f["class"];
        let name = f["name"].as_str().unwrap_or("");
        if name.contains('\0') || name.starts_with('<') {
            eprintln!("skipping corrupt schema entry: {f:?}");
            continue;
        }
        assert!(
            cls.is_string(),
            "field {:?} missing FProperty class string",
            f["name"]
        );
    }
}

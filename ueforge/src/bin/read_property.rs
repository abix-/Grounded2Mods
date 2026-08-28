//! Thin CLI over `ueforge::uasset::find_int_property`. See module docs.
//!
//! ```sh
//! cargo run -p ueforge --bin read-property. <uasset> <uexp> [property=DefaultMaxSize]
//! ```

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use ueforge::uasset::{find_int_property, parse_name_table};

fn run(uasset_path: &str, uexp_path: &str, target: &str) -> Result<(), String> {
    let uasset =
        fs::read(Path::new(uasset_path)).map_err(|e| format!("read {uasset_path}: {e}"))?;
    let uexp = fs::read(Path::new(uexp_path)).map_err(|e| format!("read {uexp_path}: {e}"))?;

    let names = parse_name_table(&uasset);
    println!("recovered {} FName entries from .uasset", names.len());

    if !names.iter().any(|n| n == target) {
        return Err(format!(
            "'{target}' NOT in name table -- mod does not reference this property"
        ));
    }
    let idx = names.iter().position(|n| n == target).unwrap();
    println!("FName index for '{target}': {idx}");

    let int_idx = names.iter().position(|n| n == "IntProperty");
    if let Some(i) = int_idx {
        println!("FName index for 'IntProperty': {i}");
    } else {
        println!("FName index for 'IntProperty': (not in name table)");
    }

    println!("\nscanning .uexp for {target} property occurrences:");
    let hits = find_int_property(&uasset, &uexp, target)?;
    for h in &hits {
        println!(
            "  @ uexp offset {}: size={} arr_idx={} guid={} VALUE={}",
            h.offset, h.size, h.array_idx, h.guid_flag, h.value
        );
    }
    println!("total hits: {}", hits.len());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: read-property <uasset> <uexp> [property=DefaultMaxSize]");
        return ExitCode::from(2);
    }
    let target = args.get(2).map(String::as_str).unwrap_or("DefaultMaxSize");
    match run(&args[0], &args[1], target) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

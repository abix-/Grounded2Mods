//! GENERIC: verify vanilla gene names load from `<game>/data/genes.xml`
//! and assert the count + named anchors lock the XML-order contract.
//!
//! Modes (env-driven):
//! - default: assert 240 names + SIZE/ASPECT/SKINNY at flat 0/1/2
//!   (the historical anchor for genes.xml order).
//! - `HORSEY_EXPECT_COUNT=<n>`: override the expected name count.
//! - `HORSEY_EXPECT_NAMES=<idx:name,idx:name,...>`: override the
//!   anchor set. e.g. `0:SIZE,1:ASPECT,239:LAST_GENE`.

mod common;

use serde_json::{Value, json};

const DEFAULT_ANCHORS: &[(usize, &str)] = &[(0, "SIZE"), (1, "ASPECT"), (2, "SKINNY")];

#[test]
fn vanilla_gene_names_load() {
    let Some(game) = common::launch("vanilla_gene_names") else {
        return;
    };

    let expected_count: usize = common::env_or("HORSEY_EXPECT_COUNT", 240usize);
    let anchors: Vec<(usize, String)> = match std::env::var("HORSEY_EXPECT_NAMES") {
        Ok(s) => s
            .split(',')
            .filter_map(|t| {
                let (i, n) = t.trim().split_once(':')?;
                Some((i.parse().ok()?, n.to_string()))
            })
            .collect(),
        Err(_) => DEFAULT_ANCHORS
            .iter()
            .map(|(i, n)| (*i, (*n).to_string()))
            .collect(),
    };

    let r = game.op_json("genes.vanilla.names", &json!({})).unwrap();
    let result = r.get("result").expect("no result");
    assert_eq!(
        result.get("loaded").and_then(Value::as_bool),
        Some(true),
        "names not loaded"
    );
    let names: Vec<String> = result
        .get("names")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    assert_eq!(
        names.len(),
        expected_count,
        "expected {expected_count} names, got {}",
        names.len()
    );

    eprintln!("first 10: {:?}", &names[..names.len().min(10)]);
    eprintln!("last 10:  {:?}", &names[names.len().saturating_sub(10)..]);

    for (idx, expected) in &anchors {
        assert!(*idx < names.len(), "anchor idx {idx} out of range");
        assert_eq!(
            &names[*idx], expected,
            "flat {idx}: expected '{expected}', got '{}'",
            names[*idx]
        );
    }

    game.pass(&format!(
        "{} names loaded, {} anchors locked",
        names.len(),
        anchors.len()
    ));
}

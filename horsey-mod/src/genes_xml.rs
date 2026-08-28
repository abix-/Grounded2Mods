//! XML loader for `genes-extended.xml` (the user-facing authoring
//! file for extended genes). Mirrors the shape of vanilla
//! `data/genes.xml` so authors who already know the vanilla format
//! can edit our extended file without re-learning anything.
//!
//! Format:
//! ```xml
//! <genes-extended>
//!   <gene name="BX_WING_SIZE" m="100" s="1"
//!         g0="0" g1="20" g2="50" g3="100">
//!     <render slot="42" mode="add" />
//!   </gene>
//!   <gene name="BX_GLOW" m="100" s="1"
//!         g0="0" g1="0" g2="50" g3="255" />
//! </genes-extended>
//! ```
//!
//! Each `<gene>` becomes one entry in `EXT_GENE_TABLE` at the next
//! available extended-range index. Optional `<render slot="..." mode="..."/>`
//! child sets the per-gene render mapping (D5 trampoline target).
//!
//! Optional attributes:
//! - `ext_idx="N"`: pin this gene to extended-range index N. Default
//!   is "next available" (sequential).
//! - missing `m` / `s` defaults to vanilla 100 / 1.
//! - missing `g0..g3` defaults to 0.
//! - missing `<render>` child means no render mapping (the gene's
//!   value is computed but not visually applied).

use crate::genes::{self, EXT_GENE_COUNT, ExtGene, RenderMapping, RenderMode};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// Results from loading `genes-extended.xml`. Returned to callers so
/// they can log / surface counts and errors.
#[derive(Debug, Default)]
pub struct LoadStats {
    /// Number of `<gene>` elements parsed.
    pub parsed: usize,
    /// Number of genes successfully placed into the extended table.
    pub placed: usize,
    /// Number of genes skipped because the slot was out of range.
    pub skipped_oor: usize,
    /// Number of genes skipped because their slot conflicted with an
    /// already-placed gene with a different name.
    pub skipped_conflict: usize,
    /// Number of `<render>` mappings successfully attached.
    pub render_mappings: usize,
    /// Per-gene parse errors (gene name + error message).
    pub errors: Vec<(String, String)>,
}

fn gene_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // Match a <gene ...> opening tag and capture (1) attribute
        // string, (2) the trailing character that closes the tag.
        // Self-closing form has trailing `/>`; body form has `>` and
        // we then locate the matching </gene> in a second pass.
        // The attribute group [^>] disallows `>` so we never overrun
        // a tag. Slash is allowed inside attributes so `/>` ends up
        // at the END (matched by the trailing group).
        Regex::new(r#"<gene\s+([^>/]*?)\s*(/?)>"#).expect("static regex")
    })
}

/// Position of the next `</gene>` after `start`. None if missing.
fn find_close(xml: &str, start: usize) -> Option<usize> {
    xml[start..].find("</gene>").map(|p| start + p)
}

fn render_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"<render\s+([^>/]*?)\s*/?>"#).expect("static regex"))
}

fn attr_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // `key="value"` with double quotes. Vanilla uses double
        // quotes throughout; we don't bother with single quotes.
        Regex::new(r#"(\w+)="([^"]*)""#).expect("static regex")
    })
}

/// Parse one attribute string like `name="X" m="100" s="1" g0="0"`
/// into a vector of (key, value) pairs.
fn parse_attrs(s: &str) -> Vec<(String, String)> {
    attr_re()
        .captures_iter(s)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect()
}

fn attr_get<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Load extended-gene definitions from a string. Useful for tests
/// and for the `genes.ext.reload` HTTP op (which reads from disk
/// then calls this).
pub fn load_from_string(xml: &str) -> LoadStats {
    let mut stats = LoadStats::default();
    let mut next_auto_idx: usize = 0;

    for cap in gene_re().captures_iter(xml) {
        stats.parsed += 1;

        let attr_str = &cap[1];
        let is_self_closing = !cap[2].is_empty();
        // For self-closing tags there's no body. For body tags, find
        // the matching </gene> and slice out the body content (the
        // string between the opening tag's `>` and the closing
        // `</gene>`). The opening tag's end is `m_full.end()`.
        let body: &str = if is_self_closing {
            ""
        } else {
            let m_full = cap.get(0).unwrap();
            let body_start = m_full.end();
            match find_close(xml, body_start) {
                Some(end) => &xml[body_start..end],
                None => "",
            }
        };
        let attrs = parse_attrs(attr_str);

        let name = match attr_get(&attrs, "name") {
            Some(n) => n.to_string(),
            None => {
                stats
                    .errors
                    .push(("<unnamed>".into(), "missing name attr".into()));
                continue;
            }
        };

        // Determine slot: explicit ext_idx wins; otherwise next auto.
        let ext_idx = match attr_get(&attrs, "ext_idx") {
            Some(s) => match s.parse::<usize>() {
                Ok(i) => i,
                Err(_) => {
                    stats
                        .errors
                        .push((name.clone(), format!("ext_idx not a usize: {s}")));
                    continue;
                }
            },
            None => {
                let i = next_auto_idx;
                next_auto_idx += 1;
                i
            }
        };

        if ext_idx >= EXT_GENE_COUNT {
            stats.skipped_oor += 1;
            stats.errors.push((
                name.clone(),
                format!("ext_idx {ext_idx} >= EXT_GENE_COUNT {EXT_GENE_COUNT}"),
            ));
            continue;
        }

        // Conflict detection: if the slot already has a non-default
        // gene with a different name, skip with an error.
        if let Some(existing) = genes::get_ext_gene(ext_idx) {
            let is_default = existing.name.starts_with("BX_RESERVED_");
            if !is_default && existing.name != name {
                stats.skipped_conflict += 1;
                stats.errors.push((
                    name.clone(),
                    format!("ext_idx {ext_idx} already occupied by '{}'", existing.name),
                ));
                continue;
            }
        }

        let mutation_rate = attr_get(&attrs, "m")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(genes::DEFAULT_MUTATION_RATE);
        let scale = attr_get(&attrs, "s")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(genes::DEFAULT_SCALE);
        let alleles = [
            attr_get(&attrs, "g0")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
            attr_get(&attrs, "g1")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
            attr_get(&attrs, "g2")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
            attr_get(&attrs, "g3")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
        ];

        // Optional <render> child.
        let render = render_re().captures(body).and_then(|rcap| {
            let rattrs = parse_attrs(&rcap[1]);
            let slot = attr_get(&rattrs, "slot")?.parse::<u32>().ok()?;
            let mode_s = attr_get(&rattrs, "mode")?;
            let mode = match mode_s {
                "add" => RenderMode::Add,
                "mul" => RenderMode::Mul,
                "set" => RenderMode::Set,
                _ => return None,
            };
            Some(RenderMapping { slot, mode })
        });
        if render.is_some() {
            stats.render_mappings += 1;
        }

        let gene = ExtGene {
            name: name.clone(),
            mutation_rate,
            scale,
            alleles,
            render,
        };
        genes::set_ext_gene(ext_idx, gene);
        stats.placed += 1;
    }

    stats
}

/// Load `genes-extended.xml` from a file path. Wraps `load_from_string`.
pub fn load_from_file(path: &Path) -> Result<LoadStats, String> {
    let xml = std::fs::read_to_string(path)
        .map_err(|e| format!("read {} failed: {e}", path.display()))?;
    Ok(load_from_string(&xml))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genes::{TEST_LOCK, reset_all_for_tests};

    fn fresh_test_xml() -> &'static str {
        r#"
<genes-extended>
  <gene name="TEST_A" m="100" s="1" g0="10" g1="20" g2="30" g3="40">
    <render slot="42" mode="add" />
  </gene>
  <gene name="TEST_B" m="50" s="2" g0="0" g1="100" g2="200" g3="255" />
  <gene name="TEST_C" ext_idx="50" g0="1" g3="9" />
</genes-extended>
"#
    }

    #[test]
    fn parses_three_genes() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let stats = load_from_string(fresh_test_xml());
        assert_eq!(stats.parsed, 3);
        assert_eq!(stats.placed, 3);
        assert_eq!(stats.render_mappings, 1);
        assert_eq!(stats.skipped_oor, 0);
        assert!(stats.errors.is_empty(), "errors: {:?}", stats.errors);
    }

    #[test]
    fn pins_explicit_ext_idx() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        load_from_string(fresh_test_xml());
        let g = genes::get_ext_gene(50).expect("TEST_C should be at ext_idx 50");
        assert_eq!(g.name, "TEST_C");
        assert_eq!(g.alleles, [1, 0, 0, 9]);
    }

    #[test]
    fn skips_oor() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let xml = format!(
            r#"<genes-extended><gene name="BAD" ext_idx="{}" /></genes-extended>"#,
            EXT_GENE_COUNT + 5
        );
        let stats = load_from_string(&xml);
        assert_eq!(stats.parsed, 1);
        assert_eq!(stats.placed, 0);
        assert_eq!(stats.skipped_oor, 1);
    }

    #[test]
    fn render_mode_add_parsed() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        load_from_string(fresh_test_xml());
        let g = genes::find_ext_gene_by_name("TEST_A").expect("TEST_A").1;
        let r = g.render.expect("render mapping");
        assert_eq!(r.slot, 42);
        assert_eq!(r.mode, RenderMode::Add);
    }

    #[test]
    fn missing_attrs_default() {
        let _g = TEST_LOCK.lock();
        reset_all_for_tests();
        let xml = r#"<genes-extended><gene name="MIN" /></genes-extended>"#;
        let stats = load_from_string(xml);
        assert_eq!(stats.parsed, 1);
        assert_eq!(stats.placed, 1);
        let (_, g) = genes::find_ext_gene_by_name("MIN").expect("MIN placed");
        assert_eq!(g.mutation_rate, genes::DEFAULT_MUTATION_RATE);
        assert_eq!(g.scale, genes::DEFAULT_SCALE);
        assert_eq!(g.alleles, [0, 0, 0, 0]);
    }
}

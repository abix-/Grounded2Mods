//! Vanilla gene names sourced from `<game-root>/data/genes.xml`.
//!
//! The engine's `FUN_1400a3eb0` loads `genes.xml` at startup and
//! assigns each `<gene>` element a flat index in document order
//! (the std::map<string,int> populated during parse hands out
//! sequential ids on first-encounter). We mirror that by reading
//! the same file and indexing names in file order.
//!
//! Loaded once on first access via OnceLock; cached for the rest of
//! the process. If the file is missing or malformed we cache `None`
//! and the UI falls back to "g{idx}" labels.

use std::path::PathBuf;
use std::sync::OnceLock;

use regex::Regex;

const VANILLA_GENE_COUNT: usize = 240;

static NAMES: OnceLock<Option<Vec<String>>> = OnceLock::new();

/// Locate `data/genes.xml` next to the currently-loaded Horsey.exe.
/// Verified path on Steam install:
///   `<exe-dir>/data/genes.xml`.
fn locate_genes_xml() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let p = dir.join("data").join("genes.xml");
    p.exists().then_some(p)
}

/// One-shot regex for `<gene ... />` extracting the attribute blob.
fn gene_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"<gene\s+([^/>]*?)/?>").expect("static regex"))
}

fn name_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"name="([^"]+)""#).expect("static regex"))
}

fn build() -> Option<Vec<String>> {
    let path = locate_genes_xml()?;
    let xml = std::fs::read_to_string(&path).ok()?;
    let mut names = Vec::with_capacity(VANILLA_GENE_COUNT);
    for cap in gene_re().captures_iter(&xml) {
        let attrs = &cap[1];
        if let Some(m) = name_re().captures(attrs) {
            names.push(m[1].to_string());
        }
        if names.len() == VANILLA_GENE_COUNT {
            break;
        }
    }
    if names.len() != VANILLA_GENE_COUNT {
        modforge::log!(
            "gene_names: parsed {} from {}; expected {VANILLA_GENE_COUNT}",
            names.len(),
            path.display()
        );
        return None;
    }
    modforge::log!(
        "gene_names: loaded {VANILLA_GENE_COUNT} from {}",
        path.display()
    );
    Some(names)
}

/// Full list of vanilla gene names indexed 0..=239 (engine-flat order).
pub fn vanilla_gene_names() -> Option<&'static [String]> {
    NAMES.get_or_init(build).as_deref()
}

/// Single-gene lookup. Returns `Some("SIZE")` etc. or `None` if the
/// file isn't loaded or `idx >= 240`.
pub fn vanilla_gene_name(idx: u8) -> Option<&'static str> {
    let all = vanilla_gene_names()?;
    all.get(idx as usize).map(|s| s.as_str())
}

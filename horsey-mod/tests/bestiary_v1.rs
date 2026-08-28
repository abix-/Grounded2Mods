//! Lock the shipped bestiary v1 XML against its contract.
//!
//! The file at `horsey-mod/bestiary/genes-extended.xml` is the
//! reference content authors will start from. This test parses it
//! through the production loader and asserts:
//!
//!   1. Every `<gene>` element places cleanly (no skips, no errors).
//!   2. `BX_GIANT_BABY` lands at extended index 0 with the right
//!      mutation/scale/alleles (the empirically-verified "giant
//!      babies" config from GENE-CATALOG.md "Confirmed visible-effect
//!      slots").
//!   3. The render mapping is `slot=0, mode=set` (the only known
//!      visible-effect slot as of 2026-05-15).
//!
//! When the bestiary expands, append additional asserts to the single
//! test below rather than splitting; the loader writes into shared
//! statics and the test binary owns no reset helper.

use horsey::genes::{self, RenderMode};
use horsey::genes_xml;

const BESTIARY_XML: &str = include_str!("../bestiary/genes-extended.xml");

#[test]
fn bestiary_v1_contract() {
    let stats = genes_xml::load_from_string(BESTIARY_XML);

    assert!(
        stats.errors.is_empty(),
        "bestiary parser errors: {:?}",
        stats.errors
    );
    assert_eq!(stats.skipped_oor, 0, "OOR skips: {stats:?}");
    assert_eq!(stats.skipped_conflict, 0, "conflict skips: {stats:?}");
    assert_eq!(
        stats.placed, stats.parsed,
        "every parsed gene must place: {stats:?}"
    );

    let g = genes::get_ext_gene(0).expect("BX_GIANT_BABY must occupy ext idx 0");
    assert_eq!(g.name, "BX_GIANT_BABY");
    assert_eq!(
        g.alleles,
        [0, 50, 100, 200],
        "g0..g3 must match the giant-baby ladder"
    );

    let r = g.render.expect("BX_GIANT_BABY needs a render mapping");
    assert_eq!(r.slot, 0, "must target the only verified visible slot");
    assert_eq!(
        r.mode,
        RenderMode::Set,
        "mode=set replaces the slot value (vs add/mul on top)"
    );
}

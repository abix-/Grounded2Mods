//! patternsleuth-backed multi-target resolver.
//!
//! Promotes the hand-rolled `scan_loaded_image` into a production
//! address-resolution primitive backed by the `patternsleuth` crate:
//!   - SIMD-accelerated scanning across many patterns in one pass.
//!   - Per-target candidate-signature lists with first-match-wins.
//!   - Reads the loaded PE image via `process::internal::read_image`.
//!
//! Public API:
//!   - `Target { name, sigs }`: one symbol to resolve, with one or
//!     more IDA-style signature strings.
//!   - `resolve_all(&[Target]) -> Resolution`: scan the loaded image
//!     for every target, return a map from name to matched address.
//!
//! The intent is that every per-game mod (horsey-mod, schedule1,
//! grounded2, future) declares a single `&[Target]` constant at
//! attach time and calls `resolve_all`. The returned addresses are
//! used as the source of truth for detour install + patch sites.

use anyhow::{Context, Result, anyhow};
use patternsleuth::process::internal::read_image;
use patternsleuth::scanner::Pattern;
use patternsleuth::{PatternConfig, ScanResult};
use std::collections::HashMap;

// Scan only the executable .text section, so 16-byte runs of 0xff
// (or other innocuous data) in .rdata/.data don't pass for function
// addresses.
const TEXT_SECTION: object::SectionKind = object::SectionKind::Text;

/// One symbol to resolve. `sigs` is tried in order; first match wins.
/// Empty `sigs` is treated as "no candidates"; resolution returns
/// `None` for that name.
#[derive(Debug, Clone)]
pub struct Target<'a> {
    pub name: &'a str,
    pub sigs: &'a [&'a str],
}

/// Result of `resolve_all`: maps each requested target name to the
/// matched runtime address (`Some(addr)`) or `None` if no candidate
/// signature matched.
#[derive(Debug, Default, Clone)]
pub struct Resolution {
    pub by_name: HashMap<String, Option<usize>>,
}

impl Resolution {
    pub fn get(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied().flatten()
    }
    pub fn require(&self, name: &str) -> Result<usize> {
        self.get(name)
            .ok_or_else(|| anyhow!("target {name:?} did not resolve"))
    }
    pub fn unresolved(&self) -> Vec<&str> {
        self.by_name
            .iter()
            .filter_map(|(k, v)| v.is_none().then_some(k.as_str()))
            .collect()
    }
}

/// Scan the loaded `.exe` image for every target's signature
/// candidates in a single pass. Returns a `Resolution` mapping each
/// target name to the matched address (or `None`).
///
/// Candidate-list semantics: for each target, signatures are tried
/// in order; the FIRST signature that matches in the image wins.
/// Subsequent candidates are not consulted for that name.
///
/// Errors only on PE parse failure (the loaded image is malformed),
/// which should never happen in practice; missing matches are not
/// errors, they appear as `None` in the resolution.
pub fn resolve_all(targets: &[Target<'_>]) -> Result<Resolution> {
    // Build patternsleuth pattern configs. The `S` (sig handle) we
    // use is `(target_index, sig_index)` so we can group results
    // back to the source target and pick the earliest-matching
    // candidate per target.
    type Sig = (usize, usize);
    let mut configs: Vec<PatternConfig<Sig>> = Vec::new();
    for (ti, t) in targets.iter().enumerate() {
        for (si, sig) in t.sigs.iter().enumerate() {
            let pat = Pattern::new(sig)
                .with_context(|| format!("target {:?} sig {si} parse failed", t.name))?;
            // Section None means "all sections"; we scan .text via
            // the section walk inside patternsleuth.
            configs.push(PatternConfig::new(
                (ti, si),
                t.name.to_string(),
                Some(TEXT_SECTION),
                pat,
            ));
        }
    }

    let image = read_image().map_err(|e| anyhow!("patternsleuth read_image failed: {e}"))?;
    let scan_result: ScanResult<Sig> = image
        .scan(&configs)
        .map_err(|e| anyhow!("patternsleuth scan failed: {e}"))?;

    // For each target, walk its candidate signatures in order and
    // accept the first one that produced ANY match. If a signature
    // matches multiple addresses, we take the first (lowest)
    // because the candidate's job is to be specific enough to
    // identify one entry; ambiguity is the caller's problem.
    let mut by_name: HashMap<String, Option<usize>> = HashMap::new();
    for (ti, t) in targets.iter().enumerate() {
        let mut chosen: Option<usize> = None;
        for si in 0..t.sigs.len() {
            let mut matches: Vec<u64> = scan_result
                .results
                .iter()
                .filter(|(cfg, _)| cfg.sig == (ti, si))
                .map(|(_, r)| r.address)
                .collect();
            if !matches.is_empty() {
                matches.sort_unstable();
                chosen = Some(matches[0] as usize);
                break;
            }
        }
        by_name.insert(t.name.to_string(), chosen);
    }

    Ok(Resolution { by_name })
}

/// Scan the loaded image's `.text` for every position matching a
/// single patternsleuth signature. Unlike [`resolve_all`], returns
/// EVERY hit (not just the first per name). Use this for xref
/// scans (`<opcode prefix> X0x<target>`), enumerating all writes of
/// a constant, etc.
///
/// Addresses are returned sorted ascending, deduplicated.
pub fn scan_all_matches(sig: &str) -> Result<Vec<usize>> {
    scan_section(sig, Some(TEXT_SECTION))
}

/// Patternsleuth scan restricted to mutable initialized data (`.data`).
/// Use for finding literal values in game state globals (e.g. the
/// player's current money u32) so we can anchor address resolution
/// on observed live values instead of guessing MSVC encodings. See
/// `horsey-mod/docs/todo.md` P0 BLOCKER section for the workflow.
pub fn scan_data_matches(sig: &str) -> Result<Vec<usize>> {
    scan_section(sig, Some(object::SectionKind::Data))
}

/// Patternsleuth scan restricted to read-only initialized data
/// (`.rdata`). String literals, vtables, jump tables, and other
/// constant rodata live here. Use to locate format strings or
/// vtable signatures so callers in `.text` can be xref-found.
pub fn scan_rdata_matches(sig: &str) -> Result<Vec<usize>> {
    scan_section(sig, Some(object::SectionKind::ReadOnlyData))
}

/// Patternsleuth scan with a caller-chosen section restriction.
/// Pass `None` to scan every section the crate walks (note:
/// patternsleuth's image walker still excludes non-loadable
/// sections like `.reloc`).
pub fn scan_section(sig: &str, section: Option<object::SectionKind>) -> Result<Vec<usize>> {
    let pat = Pattern::new(sig).with_context(|| format!("sig {sig:?} parse failed"))?;
    let config = PatternConfig::new((), "scan_all".to_string(), section, pat);
    let image = read_image().map_err(|e| anyhow!("patternsleuth read_image failed: {e}"))?;
    let configs = [config];
    let scan_result: ScanResult<()> = image
        .scan(&configs)
        .map_err(|e| anyhow!("patternsleuth scan failed: {e}"))?;
    let mut addrs: Vec<usize> = scan_result
        .results
        .iter()
        .map(|(_, r)| r.address as usize)
        .collect();
    addrs.sort_unstable();
    addrs.dedup();
    Ok(addrs)
}

// =============================================================================
// Declarative registry layer (extends Target into a persistent, validated
// resolver). Design: ../docs/target-registry.md.
//
// The existing `Target<'a>` / `Resolution` / `resolve_all` above is the
// transient one-shot API. The types below are the `'static`, validator-
// aware persistent shape. `Resolver::resolve_all` delegates the actual
// SIMD scan to the function above.
// =============================================================================

use std::sync::OnceLock;

/// Persistent, `'static`-friendly Def for one resolvable target.
/// See `def-registry.md` for the CRD shape this implements.
#[derive(Debug)]
pub struct TargetDef {
    pub name: &'static str,
    pub kind: TargetKind,
    pub candidates: &'static [Candidate],
    /// Optional RVA hint from a known-good build. Drives the
    /// `WithinHintTolerance` validator (auto-injected when set) and
    /// provides a fallback when no candidate matches.
    pub hint_rva: Option<u64>,
    /// Maximum acceptable delta between resolved and hint, in bytes.
    /// Use 0 for field offsets (exact match required), 0x1000 for
    /// data globals, larger for function entries that may shift.
    pub hint_tolerance: u64,
    /// Plausibility validators run on the candidate's decoded value
    /// before it's accepted. Each must return Ok for the candidate
    /// to win.
    pub validators: &'static [&'static dyn Validator],
}

/// What a target resolves to.
#[derive(Debug, Copy, Clone)]
pub enum TargetKind {
    /// Function entry in `.text`. Resolved value is the runtime
    /// address of the first prologue byte. `signature` is `Some`
    /// when the function is callable via `modforge::vanilla::Invoker`;
    /// `None` for hook-only or detour-only targets.
    FunctionEntry {
        signature: Option<&'static crate::vanilla::Signature>,
    },
    /// Data global in `.data` or `.rdata`. Resolved value is the
    /// runtime address of the slot itself, not its dereffed value.
    DataGlobal,
    /// Byte offset within a struct (e.g. horse + 0x205). Resolved
    /// value is the offset (not an address). Decoded by scanning
    /// the body of `containing_fn` for the operand.
    FieldOffset { containing_fn: &'static str },
}

/// One pattern-match candidate for a target. Multiple candidates
/// are tried in order; first one that decodes + passes validators
/// wins.
#[derive(Debug)]
pub struct Candidate {
    /// IDA-style hex byte sig (`48 89 5C 24 ?? ...`). Parsed by
    /// `patternsleuth::Pattern`.
    pub sig: &'static str,
    /// How to convert a sig match into the final value.
    pub recipe: Recipe,
}

/// How to convert a sig match into the final value.
#[derive(Debug, Copy, Clone)]
pub enum Recipe {
    /// The match address itself is the answer. Used for function
    /// entries where the sig matches on the prologue.
    MatchIsAddress,
    /// Decode a 4-byte RIP-relative displacement at byte offset
    /// `disp_off` inside the match. CPU's next_ip is
    /// `match_addr + instr_len`. Effective target =
    /// `(match_addr + instr_len) + disp32_signed`. Used for data
    /// globals: `mov reg, [rip+disp32]`.
    DecodeRipDisp { disp_off: u8, instr_len: u8 },
    /// Decode an immediate (1, 2, 4, or 8 bytes) at byte offset
    /// `imm_off` inside the match. Used for field offsets that
    /// are immediates in the matched instruction.
    DecodeImm { imm_off: u8, imm_size: u8 },
    /// Scan a window after each match, collect every disp32 found
    /// at instructions matching `predicate`, return the modal
    /// value. Used when one anchor produces many candidate
    /// instructions and the right field offset is the most-common
    /// disp.
    HistogramDisp {
        window_bytes: u32,
        predicate: HistPredicate,
    },
    /// Decode TWO RIP-relative disp32s at byte offsets `disp1_off`
    /// and `disp2_off` inside the match. Compute their effective
    /// targets: target_i = (match_addr + next_ip_i) + disp_i. Accept
    /// the match only if `target2 - target1 == delta`. Return
    /// `target1`. Used for resolvers like DEBUG_MODE_ACTIVE that
    /// disambiguate by the relative position of two stores.
    PairedRipDispWithDelta {
        disp1_off: u8,
        disp1_next_ip: u8,
        disp2_off: u8,
        disp2_next_ip: u8,
        delta: i64,
    },
    /// Like `DecodeRipDisp` but ADD `rel_offset` (signed) to the
    /// effective target before returning. Used when the anchor sig
    /// matches a NEIGHBORING global whose offset to the real target
    /// is known from decomp.
    RipDispWithRelOffset {
        disp_off: u8,
        instr_len: u8,
        rel_offset: i64,
    },
    /// Escape hatch for resolvers whose logic doesn't fit any of
    /// the closed-enum recipes. The function pointer is invoked
    /// with the resolved image_base and must return the final
    /// value (address for FunctionEntry/DataGlobal, offset for
    /// FieldOffset). Use sparingly; prefer extending the closed
    /// recipe set when a pattern appears more than once.
    ///
    /// The `Candidate.sig` is ignored for `Custom`; pass `""` or
    /// any placeholder. The CustomResolver gets full control over
    /// scanning, decoding, and validation.
    Custom(CustomResolver),
}

/// Function-pointer signature for `Recipe::Custom`. Const-evaluable
/// in `&'static [Candidate]` because `fn(...)` is `Copy`.
pub type CustomResolver = fn(image_base: u64) -> Result<u64, String>;

/// Predicate for `Recipe::HistogramDisp`. Filters which disp32
/// values get counted into the histogram.
#[derive(Debug, Copy, Clone)]
pub enum HistPredicate {
    /// Any disp32 with value in [lo, hi].
    InRange { lo: i64, hi: i64 },
    /// Any disp32 satisfying both: positive AND % stride == 0.
    PositiveAlignedTo { stride: u32 },
}

// ----- Validators -----------------------------------------------------------

/// Context passed to each `Validator::check` call. Carries the
/// image base + a closure for reading memory at runtime.
pub struct ValidationCtx<'a> {
    pub image_base: u64,
    /// Reads `n` bytes at `addr`. Returns None if the address is
    /// not readable (unmapped / guard / noaccess page).
    pub read_bytes: &'a dyn Fn(u64, usize) -> Option<Vec<u8>>,
}

/// Plausibility check on a resolved value. Each `TargetDef`
/// carries a slice of `&'static dyn Validator`; all must pass.
pub trait Validator: Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;
    fn check(&self, value: u64, ctx: &ValidationCtx) -> Result<()>;
}

/// Resolved address must be in the Win64 user-mode heap range
/// `[0x10000, 0x7fff_ffff_ffff)` AND 8-aligned.
#[derive(Debug)]
pub struct HeapShape;
impl Validator for HeapShape {
    fn name(&self) -> &'static str {
        "HeapShape"
    }
    fn check(&self, v: u64, _: &ValidationCtx) -> Result<()> {
        if v < 0x10000 {
            return Err(anyhow!("HeapShape: 0x{v:x} below heap floor 0x10000"));
        }
        if v >= 0x7fff_ffff_ffff {
            return Err(anyhow!("HeapShape: 0x{v:x} above heap ceiling"));
        }
        if v & 0x7 != 0 {
            return Err(anyhow!("HeapShape: 0x{v:x} not 8-aligned"));
        }
        Ok(())
    }
}

/// Resolved address must be inside the loaded image (`image_base`
/// to `image_base + 16 MiB`).
#[derive(Debug)]
pub struct InImage;
impl Validator for InImage {
    fn name(&self) -> &'static str {
        "InImage"
    }
    fn check(&self, v: u64, ctx: &ValidationCtx) -> Result<()> {
        let end = ctx.image_base.saturating_add(0x100_0000);
        if v < ctx.image_base || v >= end {
            return Err(anyhow!(
                "InImage: 0x{v:x} outside [0x{:x}, 0x{end:x})",
                ctx.image_base
            ));
        }
        Ok(())
    }
}

/// `*(value)` must equal `image_base + expected_rva`. Use to check
/// that a candidate pointer is to an object whose vtable we know.
#[derive(Debug)]
pub struct VtableAtRva(pub u64);
impl Validator for VtableAtRva {
    fn name(&self) -> &'static str {
        "VtableAtRva"
    }
    fn check(&self, v: u64, ctx: &ValidationCtx) -> Result<()> {
        let bytes =
            (ctx.read_bytes)(v, 8).ok_or_else(|| anyhow!("VtableAtRva: 0x{v:x} not readable"))?;
        if bytes.len() != 8 {
            return Err(anyhow!("VtableAtRva: short read at 0x{v:x}"));
        }
        let vtbl = u64::from_le_bytes(bytes.try_into().unwrap());
        let expected = ctx.image_base.wrapping_add(self.0);
        if vtbl != expected {
            return Err(anyhow!(
                "VtableAtRva: vtable at 0x{v:x} is 0x{vtbl:x}, expected 0x{expected:x}"
            ));
        }
        Ok(())
    }
}

/// Read a signed integer at `value + off` and assert it's in
/// `[lo, hi]`. Width is 4 bytes (i32). Use to plausibility-check
/// the value of a known field on a candidate object.
#[derive(Debug)]
pub struct FieldInRange {
    pub off: usize,
    pub lo: i64,
    pub hi: i64,
}
impl Validator for FieldInRange {
    fn name(&self) -> &'static str {
        "FieldInRange"
    }
    fn check(&self, v: u64, ctx: &ValidationCtx) -> Result<()> {
        let bytes = (ctx.read_bytes)(v + self.off as u64, 4)
            .ok_or_else(|| anyhow!("FieldInRange: 0x{:x} + 0x{:x} not readable", v, self.off))?;
        if bytes.len() != 4 {
            return Err(anyhow!("FieldInRange: short read"));
        }
        let val = i32::from_le_bytes(bytes.try_into().unwrap()) as i64;
        if val < self.lo || val > self.hi {
            return Err(anyhow!(
                "FieldInRange: field@+0x{:x}={val} outside [{},{}]",
                self.off,
                self.lo,
                self.hi
            ));
        }
        Ok(())
    }
}

/// Read a 8-byte pointer at `value + off`, assert it points to
/// readable memory. Use to plausibility-check a candidate that
/// should have a non-null heap pointer at a known offset.
#[derive(Debug)]
pub struct NestedPtrReadable {
    pub off: usize,
}
impl Validator for NestedPtrReadable {
    fn name(&self) -> &'static str {
        "NestedPtrReadable"
    }
    fn check(&self, v: u64, ctx: &ValidationCtx) -> Result<()> {
        let p_bytes = (ctx.read_bytes)(v + self.off as u64, 8).ok_or_else(|| {
            anyhow!(
                "NestedPtrReadable: outer 0x{:x}+0x{:x} not readable",
                v,
                self.off
            )
        })?;
        if p_bytes.len() != 8 {
            return Err(anyhow!("NestedPtrReadable: short read on outer"));
        }
        let inner = u64::from_le_bytes(p_bytes.try_into().unwrap());
        if inner == 0 {
            return Err(anyhow!("NestedPtrReadable: inner pointer is null"));
        }
        let probe = (ctx.read_bytes)(inner, 8);
        if probe.is_none() || probe.unwrap().len() != 8 {
            return Err(anyhow!("NestedPtrReadable: inner 0x{inner:x} not readable"));
        }
        Ok(())
    }
}

/// Auto-injected when `TargetDef.hint_rva` is set. Asserts the
/// resolved value is within `hint_tolerance` of the hint.
#[derive(Debug)]
pub struct WithinHintTolerance;
impl WithinHintTolerance {
    fn check_against(&self, value: u64, hint: u64, tolerance: u64) -> Result<()> {
        let delta = value.abs_diff(hint);
        if delta > tolerance {
            return Err(anyhow!(
                "WithinHintTolerance: value 0x{value:x} delta 0x{delta:x} from \
                 hint 0x{hint:x} exceeds tolerance 0x{tolerance:x}"
            ));
        }
        Ok(())
    }
}

// ----- Registry + Resolver --------------------------------------------------

/// Per-mod static collection of `TargetDef` entries. Owned by the
/// consumer mod; `modforge::patterns::sleuth::Resolver` is the
/// controller that walks it.
#[derive(Debug)]
pub struct TargetRegistry {
    pub name: &'static str,
    pub entries: &'static [&'static TargetDef],
}

impl TargetRegistry {
    pub const fn new(name: &'static str, entries: &'static [&'static TargetDef]) -> Self {
        Self { name, entries }
    }

    pub fn def(&self, key: &str) -> Option<&'static TargetDef> {
        self.entries.iter().copied().find(|d| d.name == key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static TargetDef> + '_ {
        self.entries.iter().copied()
    }
}

/// One resolution outcome. Logged for diagnostic, cached by name
/// inside `Resolver`.
#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    pub name: &'static str,
    /// Final value: address for FunctionEntry/DataGlobal, offset for
    /// FieldOffset. `None` if no candidate resolved + no hint
    /// fallback applied.
    pub value: Option<u64>,
    pub candidate_idx: Option<usize>,
    pub validation_log: Vec<String>,
    /// Set when no candidate matched but `hint_rva` was used as
    /// fallback.
    pub from_hint: bool,
}

/// Caching controller bound to a `TargetRegistry`. Per-process
/// singleton recommended (one `Resolver` per registry per mod
/// attach). Subsequent calls hit the cache.
pub struct Resolver {
    registry: &'static TargetRegistry,
    cache: OnceLock<HashMap<&'static str, ResolvedTarget>>,
}

impl Resolver {
    pub const fn new(registry: &'static TargetRegistry) -> Self {
        Self {
            registry,
            cache: OnceLock::new(),
        }
    }

    pub fn registry(&self) -> &'static TargetRegistry {
        self.registry
    }

    /// Resolve every entry. Cached after first call.
    pub fn resolve_all(&self) -> &HashMap<&'static str, ResolvedTarget> {
        self.cache.get_or_init(|| self.run())
    }

    /// Resolved value for a single named entry, or `None`.
    pub fn resolve(&self, name: &str) -> Option<u64> {
        self.resolve_all().get(name).and_then(|r| r.value)
    }

    /// Full diagnostic JSON view of every entry's resolution +
    /// validation log. For the HTTP control-plane cmdlet.
    pub fn diagnostic(&self) -> serde_json::Value {
        let all = self.resolve_all();
        let mut entries: Vec<_> = self
            .registry
            .iter()
            .map(|def| {
                let r = all.get(def.name);
                serde_json::json!({
                    "name": def.name,
                    "kind": format!("{:?}", def.kind),
                    "value": r.and_then(|x| x.value).map(|v| format!("0x{v:x}")),
                    "candidate_idx": r.and_then(|x| x.candidate_idx),
                    "from_hint": r.map(|x| x.from_hint).unwrap_or(false),
                    "hint_rva": def.hint_rva.map(|h| format!("0x{h:x}")),
                    "validation_log": r.map(|x| x.validation_log.clone()).unwrap_or_default(),
                })
            })
            .collect();
        entries.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        serde_json::json!({
            "registry": self.registry.name,
            "entries": entries,
        })
    }

    fn run(&self) -> HashMap<&'static str, ResolvedTarget> {
        let image_base = current_image_base();
        let read_bytes = make_read_bytes_closure();
        let ctx = ValidationCtx {
            image_base,
            read_bytes: &read_bytes,
        };

        let mut out = HashMap::new();
        for def in self.registry.iter() {
            let resolved = resolve_one(def, &ctx, image_base);
            out.insert(def.name, resolved);
        }
        out
    }
}

/// Resolve a single TargetDef: try each candidate in order, decode
/// per Recipe, run validators, return the first that passes.
fn resolve_one(def: &'static TargetDef, ctx: &ValidationCtx, image_base: u64) -> ResolvedTarget {
    let mut log: Vec<String> = Vec::new();

    for (idx, cand) in def.candidates.iter().enumerate() {
        match decode_candidate(cand, def, image_base) {
            Ok(value) => {
                log.push(format!("candidate[{idx}] decoded -> 0x{value:x}"));
                if let Err(e) = run_validators(def, value, ctx) {
                    log.push(format!("candidate[{idx}] validators rejected: {e}"));
                    continue;
                }
                log.push(format!("candidate[{idx}] accepted"));
                return ResolvedTarget {
                    name: def.name,
                    value: Some(value),
                    candidate_idx: Some(idx),
                    validation_log: log,
                    from_hint: false,
                };
            }
            Err(e) => {
                log.push(format!("candidate[{idx}] decode failed: {e}"));
            }
        }
    }

    // No candidate worked. Fall back to hint_rva if set.
    if let Some(hint) = def.hint_rva {
        // Accept hint as either a bare RVA (small, e.g. 0x3d95a5) or a
        // pre-rebased VA at the conventional x64 PE preferred base of
        // 0x140000000 (e.g. 0x1403d95a5). The previous formulation
        // `hint & 0xffff_ffff` was wrong: for a pre-rebased VA above 4GB
        // it kept the bits including the 0x40000000 nibble below the
        // preferred-base prefix, producing an off-by-0x40000000 address
        // that lands past the loaded image.
        const PE_PREFERRED_BASE_X64: u64 = 0x140000000;
        let rva = if hint >= PE_PREFERRED_BASE_X64 {
            hint - PE_PREFERRED_BASE_X64
        } else {
            hint
        };
        let value = match def.kind {
            TargetKind::FieldOffset { .. } => hint, // offset, not address
            _ => image_base.wrapping_add(rva),
        };
        log.push(format!("fallback to hint 0x{value:x}"));
        return ResolvedTarget {
            name: def.name,
            value: Some(value),
            candidate_idx: None,
            validation_log: log,
            from_hint: true,
        };
    }

    log.push("no candidate matched and no hint fallback".into());
    ResolvedTarget {
        name: def.name,
        value: None,
        candidate_idx: None,
        validation_log: log,
        from_hint: false,
    }
}

/// Decode a candidate to its final value per the Recipe.
fn decode_candidate(cand: &Candidate, def: &TargetDef, image_base: u64) -> Result<u64> {
    let hits = scan_all_matches(cand.sig)?;
    if hits.is_empty() {
        return Err(anyhow!("no match for sig {:?}", cand.sig));
    }

    match cand.recipe {
        Recipe::MatchIsAddress => Ok(hits[0] as u64),
        Recipe::DecodeRipDisp {
            disp_off,
            instr_len,
        } => {
            let match_addr = hits[0] as u64;
            let disp = read_i32_at(match_addr + disp_off as u64)?;
            Ok(match_addr
                .wrapping_add(instr_len as u64)
                .wrapping_add(disp as i64 as u64))
        }
        Recipe::DecodeImm { imm_off, imm_size } => {
            let match_addr = hits[0] as u64;
            let v = read_imm_at(match_addr + imm_off as u64, imm_size)?;
            Ok(v)
        }
        Recipe::HistogramDisp {
            window_bytes,
            predicate,
        } => decode_histogram(&hits, window_bytes, predicate, def, image_base),
        Recipe::PairedRipDispWithDelta {
            disp1_off,
            disp1_next_ip,
            disp2_off,
            disp2_next_ip,
            delta,
        } => {
            // Try every match; accept the first one whose two
            // disp32s form effective-targets whose delta matches.
            for &m in &hits {
                let m = m as u64;
                let d1 = read_i32_at(m + disp1_off as u64)?;
                let d2 = read_i32_at(m + disp2_off as u64)?;
                let t1 = m
                    .wrapping_add(disp1_next_ip as u64)
                    .wrapping_add(d1 as i64 as u64);
                let t2 = m
                    .wrapping_add(disp2_next_ip as u64)
                    .wrapping_add(d2 as i64 as u64);
                let observed = (t2 as i64).wrapping_sub(t1 as i64);
                if observed == delta {
                    return Ok(t1);
                }
            }
            Err(anyhow!(
                "no match passed paired-disp delta {} for sig {:?}",
                delta,
                cand.sig
            ))
        }
        Recipe::RipDispWithRelOffset {
            disp_off,
            instr_len,
            rel_offset,
        } => {
            let match_addr = hits[0] as u64;
            let disp = read_i32_at(match_addr + disp_off as u64)?;
            let target = match_addr
                .wrapping_add(instr_len as u64)
                .wrapping_add(disp as i64 as u64);
            Ok((target as i64).wrapping_add(rel_offset) as u64)
        }
        Recipe::Custom(resolver) => {
            resolver(image_base).map_err(|e| anyhow!("custom resolver: {e}"))
        }
    }
}

fn read_i32_at(addr: u64) -> Result<i32> {
    if !crate::winproc::is_addr_readable(addr as usize) {
        return Err(anyhow!("addr 0x{addr:x} not readable"));
    }
    let bytes = unsafe { std::slice::from_raw_parts(addr as *const u8, 4) };
    Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_imm_at(addr: u64, size: u8) -> Result<u64> {
    if !crate::winproc::is_addr_readable(addr as usize) {
        return Err(anyhow!("addr 0x{addr:x} not readable"));
    }
    let n = size as usize;
    if n == 0 || n > 8 {
        return Err(anyhow!("invalid imm size {n}"));
    }
    let bytes = unsafe { std::slice::from_raw_parts(addr as *const u8, n) };
    let mut buf = [0u8; 8];
    buf[..n].copy_from_slice(bytes);
    Ok(u64::from_le_bytes(buf))
}

fn decode_histogram(
    hits: &[usize],
    window_bytes: u32,
    predicate: HistPredicate,
    _def: &TargetDef,
    _image_base: u64,
) -> Result<u64> {
    use std::collections::BTreeMap;
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    for &h in hits {
        let start = h as u64;
        let end = start.saturating_add(window_bytes as u64);
        let mut p = start;
        while p + 4 <= end {
            if !crate::winproc::is_addr_readable(p as usize) {
                break;
            }
            let bytes = unsafe { std::slice::from_raw_parts(p as *const u8, 4) };
            let v = i32::from_le_bytes(bytes.try_into().unwrap()) as i64;
            let pass = match predicate {
                HistPredicate::InRange { lo, hi } => v >= lo && v <= hi,
                HistPredicate::PositiveAlignedTo { stride } => {
                    v > 0 && (v as u64 % stride as u64) == 0
                }
            };
            if pass {
                *hist.entry(v).or_insert(0) += 1;
            }
            p += 1;
        }
    }
    hist.into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(v, _)| v as u64)
        .ok_or_else(|| anyhow!("histogram empty"))
}

/// Run every validator on a value. Includes the auto-injected
/// WithinHintTolerance check when hint_rva is set.
fn run_validators(def: &TargetDef, value: u64, ctx: &ValidationCtx) -> Result<()> {
    if let Some(hint) = def.hint_rva {
        let real_hint = match def.kind {
            TargetKind::FieldOffset { .. } => hint,
            _ => {
                if hint < 0x1_0000_0000 {
                    ctx.image_base.wrapping_add(hint)
                } else {
                    hint
                }
            }
        };
        WithinHintTolerance.check_against(value, real_hint, def.hint_tolerance)?;
    }
    for v in def.validators {
        v.check(value, ctx)
            .with_context(|| format!("validator {}", v.name()))?;
    }
    Ok(())
}

/// Runtime image-base for the loaded `.exe`. On Win64 this is what
/// `GetModuleHandleW(NULL)` returns. Used by the Resolver to feed
/// validators their `ValidationCtx`.
fn current_image_base() -> u64 {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    let h = unsafe { GetModuleHandleW(std::ptr::null()) } as u64;
    h
}

/// Make a memory-reading closure for the ValidationCtx. Uses the
/// process's VirtualQuery to avoid faulting on unmapped pages.
fn make_read_bytes_closure() -> impl Fn(u64, usize) -> Option<Vec<u8>> {
    |addr: u64, n: usize| -> Option<Vec<u8>> {
        if n == 0 || n > 0x1000 {
            return None;
        }
        if !crate::winproc::is_addr_readable(addr as usize) {
            return None;
        }
        if !crate::winproc::is_addr_readable(addr as usize + n - 1) {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(addr as *const u8, n) };
        Some(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // No-game unit tests: ensure the parser-side of patternsleuth
    // wrappers behaves. Live-image resolution is tested in
    // `horsey-mod/tests/r2_resolver.rs` because it requires a
    // running game.

    #[test]
    fn resolution_get_returns_inner() {
        let mut r = Resolution::default();
        r.by_name.insert("X".into(), Some(0xdead));
        r.by_name.insert("Y".into(), None);
        assert_eq!(r.get("X"), Some(0xdead));
        assert_eq!(r.get("Y"), None);
        assert_eq!(r.get("Z"), None);
    }

    #[test]
    fn resolution_require_errs_on_missing() {
        let mut r = Resolution::default();
        r.by_name.insert("X".into(), None);
        assert!(r.require("X").is_err());
        assert!(r.require("Z").is_err());
    }

    #[test]
    fn resolution_unresolved_lists_nones_only() {
        let mut r = Resolution::default();
        r.by_name.insert("A".into(), Some(1));
        r.by_name.insert("B".into(), None);
        r.by_name.insert("C".into(), None);
        let mut got = r.unresolved();
        got.sort();
        assert_eq!(got, vec!["B", "C"]);
    }

    // ----- Registry-layer tests -----

    fn never_readable() -> impl Fn(u64, usize) -> Option<Vec<u8>> {
        |_, _| None
    }

    fn scripted_reader(map: HashMap<u64, Vec<u8>>) -> impl Fn(u64, usize) -> Option<Vec<u8>> {
        move |addr, n| map.get(&addr).map(|v| v[..n.min(v.len())].to_vec())
    }

    #[test]
    fn heapshape_rejects_low_high_unaligned() {
        let ctx = ValidationCtx {
            image_base: 0x140000000,
            read_bytes: &never_readable(),
        };
        assert!(HeapShape.check(0x1000, &ctx).is_err());
        assert!(HeapShape.check(0x8000_0000_0000, &ctx).is_err());
        assert!(HeapShape.check(0x1_0000_0001, &ctx).is_err()); // misaligned
        assert!(HeapShape.check(0x1_0000_0008, &ctx).is_ok());
    }

    #[test]
    fn inimage_window_check() {
        let ctx = ValidationCtx {
            image_base: 0x140000000,
            read_bytes: &never_readable(),
        };
        assert!(InImage.check(0x140000000, &ctx).is_ok());
        assert!(InImage.check(0x140abcdef, &ctx).is_ok());
        assert!(InImage.check(0x13fffffff, &ctx).is_err());
        assert!(InImage.check(0x141000000, &ctx).is_err());
    }

    #[test]
    fn vtable_at_rva_round_trips() {
        let image_base = 0x140000000u64;
        let expected = image_base.wrapping_add(0x3037d0);
        let obj_addr = 0x1f_2a7c_8540u64;
        let mut map = HashMap::new();
        map.insert(obj_addr, expected.to_le_bytes().to_vec());
        let ctx = ValidationCtx {
            image_base,
            read_bytes: &scripted_reader(map),
        };

        assert!(VtableAtRva(0x3037d0).check(obj_addr, &ctx).is_ok());
        assert!(VtableAtRva(0xdeadbeef).check(obj_addr, &ctx).is_err());
        assert!(VtableAtRva(0x3037d0).check(0xbadbadbad, &ctx).is_err()); // unmapped
    }

    #[test]
    fn field_in_range_bounds() {
        let mut map = HashMap::new();
        // value 42 at addr+0x10
        map.insert(0x1000 + 0x10, 42i32.to_le_bytes().to_vec());
        // value -5 at addr+0x14
        map.insert(0x2000 + 0x14, (-5i32).to_le_bytes().to_vec());
        let ctx = ValidationCtx {
            image_base: 0,
            read_bytes: &scripted_reader(map),
        };

        assert!(
            FieldInRange {
                off: 0x10,
                lo: 0,
                hi: 200
            }
            .check(0x1000, &ctx)
            .is_ok()
        );
        assert!(
            FieldInRange {
                off: 0x10,
                lo: 100,
                hi: 200
            }
            .check(0x1000, &ctx)
            .is_err()
        );
        assert!(
            FieldInRange {
                off: 0x14,
                lo: -10,
                hi: 0
            }
            .check(0x2000, &ctx)
            .is_ok()
        );
    }

    #[test]
    fn nested_ptr_readable_checks_both_hops() {
        let outer = 0x1000u64;
        let inner = 0x2000u64;
        let mut map = HashMap::new();
        map.insert(outer + 0x8, inner.to_le_bytes().to_vec());
        map.insert(inner, vec![0u8; 8]); // make inner readable
        let ctx = ValidationCtx {
            image_base: 0,
            read_bytes: &scripted_reader(map),
        };

        assert!(NestedPtrReadable { off: 0x8 }.check(outer, &ctx).is_ok());

        // Null inner pointer rejects.
        let mut map_null = HashMap::new();
        map_null.insert(outer + 0x8, vec![0u8; 8]);
        let ctx_null = ValidationCtx {
            image_base: 0,
            read_bytes: &scripted_reader(map_null),
        };
        assert!(
            NestedPtrReadable { off: 0x8 }
                .check(outer, &ctx_null)
                .is_err()
        );
    }

    #[test]
    fn within_hint_tolerance_respects_delta() {
        let v = WithinHintTolerance;
        assert!(v.check_against(0x140000000, 0x140000000, 0).is_ok());
        assert!(v.check_against(0x140000800, 0x140000000, 0x1000).is_ok());
        assert!(v.check_against(0x140002000, 0x140000000, 0x1000).is_err());
        // Direction-agnostic.
        assert!(v.check_against(0x140000000, 0x140000800, 0x1000).is_ok());
    }

    #[test]
    fn registry_lookup_and_iter() {
        static D1: TargetDef = TargetDef {
            name: "A",
            kind: TargetKind::DataGlobal,
            candidates: &[],
            hint_rva: None,
            hint_tolerance: 0,
            validators: &[],
        };
        static D2: TargetDef = TargetDef {
            name: "B",
            kind: TargetKind::DataGlobal,
            candidates: &[],
            hint_rva: None,
            hint_tolerance: 0,
            validators: &[],
        };
        static REG: TargetRegistry = TargetRegistry::new("test", &[&D1, &D2]);

        assert_eq!(REG.def("A").unwrap().name, "A");
        assert_eq!(REG.def("B").unwrap().name, "B");
        assert!(REG.def("missing").is_none());
        assert_eq!(REG.iter().count(), 2);
    }

    #[test]
    fn validators_const_evaluate_in_slice() {
        // The big payoff: a TargetDef can be const-constructed with
        // a `&'static [&'static dyn Validator]`. This compile-tests
        // that the slice literal evaluates with the built-in
        // validators.
        static VALIDATORS: &[&dyn Validator] = &[
            &HeapShape,
            &InImage,
            &VtableAtRva(0x3037d0),
            &FieldInRange {
                off: 0x10,
                lo: 0,
                hi: 100,
            },
            &NestedPtrReadable { off: 0x8 },
        ];
        assert_eq!(VALIDATORS.len(), 5);
        assert_eq!(VALIDATORS[0].name(), "HeapShape");
        assert_eq!(VALIDATORS[2].name(), "VtableAtRva");
    }
}

//! Offline UE5 cooked `.uasset` / `.uexp` parsing.
//!
//! Tools for inspecting cooked UE assets WITHOUT a running game.
//! useful for diffing third-party pak mods, reverse-engineering data
//! tables ahead of writing a runtime patch, or sanity-checking that a
//! mod actually touches the field its description claims.
//!
//! This is the offline counterpart to [`crate::ue`] (which models the
//! engine's *runtime* memory layout). Use [`crate::ue`] when the game
//! is running; use this module when you have raw `.uasset` /
//! `.uexp` bytes and no engine.
//!
//! ## Capabilities
//!
//! - [`extract_printable_strings`]. Scan-for-strings dump (`strings`-
//!   style). Cheap reconnaissance pass over an unfamiliar asset.
//! - [`classify_ue_name`] / [`classify_ue_path`] / [`classify_prose`]
//!  . String classifiers for filtering strings into useful buckets.
//! - [`parse_name_table`]. Recover the FName table from a cooked
//!   `.uasset` header by scanning for the densest run of valid
//!   length-prefixed entries.
//! - [`find_int_property`]. Given a property name, find every
//!   IntProperty tag for that name in the corresponding `.uexp` and
//!   decode the value.
//!
//! ## CLI binaries
//!
//! ueforge ships two thin `[[bin]]` wrappers for ad-hoc inspection
//! from a shell:
//!
//! - `cargo run --bin dump-strings -- <file> [file ...]`
//! - `cargo run --bin read-property -- <uasset> <uexp> [property]`
//!
//! ## Limitations
//!
//! Targets UE 5.4+ legacy cooked format as produced by `retoc to-legacy`.
//! Doesn't handle every UE asset format edge case. Good enough for
//! the typical "what fields does this mod touch" investigation.
//! Cross-references [`crate::ue`] knowledge for runtime work.

use std::collections::HashSet;

/// Default minimum length for [`extract_printable_strings`].
pub const DEFAULT_MIN_PRINTABLE_LEN: usize = 4;

/// Scan `bytes` for runs of printable ASCII (0x20..=0x7E) at least
/// `min_len` long, dedupe (preserving first-seen order), return the
/// list. The standard reconnaissance pass over an unfamiliar
/// `.uasset` / `.uexp`.
pub fn extract_printable_strings(bytes: &[u8], min_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut i = 0;
    while i < bytes.len() {
        if !is_printable(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_printable(bytes[i]) {
            i += 1;
        }
        if i - start >= min_len {
            // SAFETY: every byte in [start, i) is in 0x20..=0x7E.
            let s = std::str::from_utf8(&bytes[start..i]).unwrap().to_string();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
    }
    out
}

#[inline]
fn is_printable(b: u8) -> bool {
    (0x20..=0x7E).contains(&b)
}

/// True if `s` looks like a UE identifier
/// (`/[A-Za-z_][A-Za-z0-9_]{2,}/`).
pub fn classify_ue_name(s: &str) -> bool {
    if s.len() < 3 {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// True if `s` looks like a UE asset path (`/Game/...`).
pub fn classify_ue_path(s: &str) -> bool {
    s.starts_with('/') && s[1..].contains('/')
}

/// True if `s` looks like prose. Has at least one alphabetic
/// character AND at least one whitespace or sentence-punctuation
/// character.
pub fn classify_prose(s: &str) -> bool {
    let has_alpha = s.chars().any(|c| c.is_alphabetic());
    let has_punct = s
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ':'));
    has_alpha && has_punct
}

/// Walk a cooked legacy `.uasset` looking for the FName table and
/// return the names in order.
///
/// UE legacy cooked name table entries look like:
///
/// ```text
///     int32 length        (positive = ASCII, negative = UTF-16; length includes null)
///     bytes / wchars
///     uint16 nonCasePreserving + uint16 casePreserving (hashes)
/// ```
///
/// We don't know the exact start offset without parsing the asset
/// header, but we can find the table by scanning for the densest run
/// of valid length-prefixed entries and returning that group's names.
///
/// Currently handles ASCII names only (positive length). UTF-16
/// names are skipped.
pub fn parse_name_table(uasset: &[u8]) -> Vec<String> {
    let mut entries: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i + 8 < uasset.len() {
        let length = i32::from_le_bytes([uasset[i], uasset[i + 1], uasset[i + 2], uasset[i + 3]]);
        if length == 0 || length > 256 || length < -256 {
            i += 1;
            continue;
        }
        if length > 0 {
            let len = length as usize;
            let null_at = i + 4 + len - 1;
            if null_at >= uasset.len() || uasset[null_at] != 0 {
                i += 1;
                continue;
            }
            let text_bytes = &uasset[i + 4..null_at];
            // Validate ASCII printable + null terminator.
            if text_bytes.iter().any(|&b| b < 32 && b != 9) {
                i += 1;
                continue;
            }
            let Ok(name) = std::str::from_utf8(text_bytes) else {
                i += 1;
                continue;
            };
            if name.is_empty() {
                i += 1;
                continue;
            }
            entries.push((i, name.to_string()));
            i += 4 + len + 4; // length + bytes + 4-byte hash word
            continue;
        }
        // UTF-16 (negative length). Skip for now.
        i += 1;
    }

    if entries.is_empty() {
        return Vec::new();
    }

    // Group by gap. Pick the longest contiguous run.
    let mut groups: Vec<Vec<(usize, String)>> = Vec::new();
    let mut cur: Vec<(usize, String)> = vec![entries[0].clone()];
    for w in entries.windows(2) {
        let prev = &w[0];
        let item = &w[1];
        if item.0.saturating_sub(prev.0) < 200 {
            cur.push(item.clone());
        } else {
            groups.push(std::mem::take(&mut cur));
            cur.push(item.clone());
        }
    }
    groups.push(cur);
    let best = groups.into_iter().max_by_key(|g| g.len()).unwrap();
    best.into_iter().map(|(_, n)| n).collect()
}

/// One IntProperty tag found in the `.uexp` for the searched property
/// name.
#[derive(Debug, Clone, Copy)]
pub struct PropertyHit {
    /// Offset of the tag in the `.uexp` bytes.
    pub offset: usize,
    /// Size word after the tag header.
    pub size: u32,
    /// Array-index word after the tag header.
    pub array_idx: u32,
    /// "Has property GUID" flag (0 or 1).
    pub guid_flag: u8,
    /// Decoded i32 value.
    pub value: i32,
}

/// Search a cooked `.uexp` for every IntProperty tag carrying the
/// given property name and decode the value.
///
/// Cooked property tag layout:
///
/// ```text
///     u32 name_idx; u32 name_num;       // FName: NameIdx, NameNum
///     u32 type_idx; u32 type_num;       // FName: type ("IntProperty")
///     u32 size; u32 array_idx;
///     u8  has_property_guid;            // (skipped here)
///     <value bytes>
/// ```
///
/// For IntProperty, the value is a single `i32` immediately after the
/// guid flag (or after a 16-byte GUID when guid_flag is 1).
///
/// Returns one [`PropertyHit`] per tagged occurrence. Returns
/// `Err(...)` if the name table can't be parsed or if `property_name`
/// / `"IntProperty"` aren't present in it.
pub fn find_int_property(
    uasset: &[u8],
    uexp: &[u8],
    property_name: &str,
) -> Result<Vec<PropertyHit>, String> {
    let names = parse_name_table(uasset);
    if names.is_empty() {
        return Err("could not recover FName table from .uasset".into());
    }
    let name_idx = names
        .iter()
        .position(|n| n == property_name)
        .ok_or_else(|| format!("'{property_name}' not in name table"))?;
    let int_idx = names
        .iter()
        .position(|n| n == "IntProperty")
        .ok_or_else(|| "'IntProperty' not in name table".to_string())?;

    let target_tag = u32_pair(name_idx as u32, 0);
    let type_tag = u32_pair(int_idx as u32, 0);

    let mut hits = Vec::new();
    let mut pos = 0;
    while let Some(p) = find_subseq(&uexp[pos..], &target_tag).map(|q| pos + q) {
        if p + 24 + 1 > uexp.len() {
            break;
        }
        if &uexp[p + 8..p + 16] == &type_tag {
            let size = u32_le(&uexp[p + 16..p + 20]);
            let array_idx = u32_le(&uexp[p + 20..p + 24]);
            let guid_flag = uexp[p + 24];
            let value_offset = p + 25 + if guid_flag != 0 { 16 } else { 0 };
            if value_offset + 4 <= uexp.len() {
                let value = i32::from_le_bytes([
                    uexp[value_offset],
                    uexp[value_offset + 1],
                    uexp[value_offset + 2],
                    uexp[value_offset + 3],
                ]);
                hits.push(PropertyHit {
                    offset: p,
                    size,
                    array_idx,
                    guid_flag,
                    value,
                });
            }
        }
        pos = p + 1;
    }
    Ok(hits)
}

fn u32_pair(a: u32, b: u32) -> [u8; 8] {
    let mut out = [0u8; 8];
    out[0..4].copy_from_slice(&a.to_le_bytes());
    out[4..8].copy_from_slice(&b.to_le_bytes());
    out
}

fn u32_le(bs: &[u8]) -> u32 {
    u32::from_le_bytes([bs[0], bs[1], bs[2], bs[3]])
}

fn find_subseq(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_dedupes() {
        let bytes = b"\0\0Hello\0\0World\0Hello\0";
        let strings = extract_printable_strings(bytes, 4);
        assert_eq!(strings, vec!["Hello".to_string(), "World".to_string()]);
    }

    #[test]
    fn extract_min_len() {
        let bytes = b"\0\0abc\0Hello\0";
        let strings = extract_printable_strings(bytes, 4);
        assert_eq!(strings, vec!["Hello".to_string()]);
    }

    #[test]
    fn classifiers() {
        assert!(classify_ue_name("BP_Foo_C"));
        assert!(classify_ue_name("DefaultMaxSize"));
        assert!(!classify_ue_name("ab"));
        assert!(!classify_ue_name("3leading"));
        assert!(classify_ue_path("/Game/Blueprints/Foo"));
        assert!(!classify_ue_path("/single"));
        assert!(classify_prose("Hello, world!"));
        assert!(!classify_prose("BP_Foo_C"));
    }

    #[test]
    fn name_table_empty_input() {
        assert!(parse_name_table(&[]).is_empty());
    }
}

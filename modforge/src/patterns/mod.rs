//! IDA-style byte-pattern scanner for resolving function entry points
//! in a loaded PE image.
//!
//! Two layers:
//!   - This module's hand-rolled `parse` / `find_first` / `find_all` /
//!     `scan_loaded_image`. Minimal, dependency-free, single-target.
//!     Good for tests + quick one-offs.
//!   - `modforge::patterns::sleuth`: wraps the `patternsleuth` crate
//!     for multi-target SIMD-accelerated scanning with per-target
//!     candidate-signature lists. The right primitive for production
//!     address resolution at DLL attach.
//!
//! Pattern grammar (same in both layers):
//!   - Whitespace separated.
//!   - Two-hex-digit token (e.g. `48`, `8b`) matches that exact byte.
//!   - `??` (or `?`) matches any byte.

pub mod sleuth;

/// One element of a parsed pattern: either a required byte or a
/// wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatByte {
    Byte(u8),
    Any,
}

/// Parse an IDA-style pattern string into a vector of `PatByte`.
/// Returns `Err` on malformed tokens or empty input.
pub fn parse(pattern: &str) -> Result<Vec<PatByte>, String> {
    let mut out = Vec::new();
    for tok in pattern.split_whitespace() {
        if tok == "?" || tok == "??" {
            out.push(PatByte::Any);
            continue;
        }
        if tok.len() != 2 {
            return Err(format!("token {tok:?} is not 2 hex digits or `??`"));
        }
        match u8::from_str_radix(tok, 16) {
            Ok(b) => out.push(PatByte::Byte(b)),
            Err(_) => return Err(format!("token {tok:?} is not valid hex")),
        }
    }
    if out.is_empty() {
        return Err("empty pattern".into());
    }
    Ok(out)
}

/// Find the FIRST offset in `haystack` where `pattern` matches.
/// Returns `None` if no match. Wildcards in `pattern` match any byte.
pub fn find_first(haystack: &[u8], pattern: &str) -> Option<usize> {
    let pat = parse(pattern).ok()?;
    find_first_parsed(haystack, &pat)
}

/// Like `find_first` but takes a pre-parsed pattern. Useful when
/// scanning many haystacks against the same pattern.
pub fn find_first_parsed(haystack: &[u8], pattern: &[PatByte]) -> Option<usize> {
    if pattern.is_empty() || pattern.len() > haystack.len() {
        return None;
    }
    let stop = haystack.len() - pattern.len() + 1;
    'outer: for i in 0..stop {
        for (j, pb) in pattern.iter().enumerate() {
            match pb {
                PatByte::Any => continue,
                PatByte::Byte(b) => {
                    if haystack[i + j] != *b {
                        continue 'outer;
                    }
                }
            }
        }
        return Some(i);
    }
    None
}

/// Find ALL offsets where `pattern` matches.
pub fn find_all(haystack: &[u8], pattern: &str) -> Vec<usize> {
    let Ok(pat) = parse(pattern) else {
        return Vec::new();
    };
    find_all_parsed(haystack, &pat)
}

pub fn find_all_parsed(haystack: &[u8], pattern: &[PatByte]) -> Vec<usize> {
    let mut out = Vec::new();
    if pattern.is_empty() || pattern.len() > haystack.len() {
        return out;
    }
    let stop = haystack.len() - pattern.len() + 1;
    'outer: for i in 0..stop {
        for (j, pb) in pattern.iter().enumerate() {
            match pb {
                PatByte::Any => continue,
                PatByte::Byte(b) => {
                    if haystack[i + j] != *b {
                        continue 'outer;
                    }
                }
            }
        }
        out.push(i);
    }
    out
}

/// Scan the loaded `.exe` image for `pattern` and return the
/// matching runtime address. Reads `GetModuleHandleW(NULL)` for the
/// image base, walks PE section headers to find `.text`, and scans
/// that section's bytes.
pub fn scan_loaded_image(pattern: &str) -> Option<usize> {
    let pat = parse(pattern).ok()?;
    let (text_start, text_len) = locate_text_section()?;
    // SAFETY: `text_start..text_start+text_len` is inside the
    // loaded `.exe` image and stays valid for the process
    // lifetime.
    let bytes = unsafe { std::slice::from_raw_parts(text_start as *const u8, text_len) };
    let off = find_first_parsed(bytes, &pat)?;
    Some(text_start.wrapping_add(off))
}

fn locate_text_section() -> Option<(usize, usize)> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    let image_base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
    if image_base == 0 {
        return None;
    }
    // SAFETY: reading bytes from our own process image; bounds
    // sanity-checked below before each deref.
    unsafe {
        let dos = image_base as *const u8;
        let e_lfanew = *(dos.add(0x3c) as *const i32) as usize;
        let nt = dos.add(e_lfanew);
        if *nt != b'P' || *nt.add(1) != b'E' || *nt.add(2) != 0 || *nt.add(3) != 0 {
            return None;
        }
        let num_sections = *(nt.add(4 + 2) as *const u16) as usize;
        let size_of_opt_hdr = *(nt.add(4 + 16) as *const u16) as usize;
        let first_section = nt.add(4 + 20 + size_of_opt_hdr);
        for i in 0..num_sections {
            let sec = first_section.add(i * 40);
            let mut name = [0u8; 8];
            std::ptr::copy_nonoverlapping(sec, name.as_mut_ptr(), 8);
            if &name[..6] == b".text\0" {
                let virtual_size = *(sec.add(0x08) as *const u32) as usize;
                let virtual_address = *(sec.add(0x0c) as *const u32) as usize;
                return Some((image_base + virtual_address, virtual_size));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_bytes() {
        let p = parse("48 8b c4").unwrap();
        assert_eq!(
            p,
            vec![
                PatByte::Byte(0x48),
                PatByte::Byte(0x8b),
                PatByte::Byte(0xc4)
            ]
        );
    }

    #[test]
    fn parses_wildcards() {
        let p = parse("48 ?? c4 ?").unwrap();
        assert_eq!(
            p,
            vec![
                PatByte::Byte(0x48),
                PatByte::Any,
                PatByte::Byte(0xc4),
                PatByte::Any
            ]
        );
    }

    #[test]
    fn parses_handles_extra_whitespace() {
        let p = parse("  48\t  ??\n c4  ").unwrap();
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn parser_rejects_empty() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
    }

    #[test]
    fn parser_rejects_malformed_tokens() {
        assert!(parse("48 GG c4").is_err());
        assert!(parse("48 1 c4").is_err());
        assert!(parse("48 1234 c4").is_err());
    }

    #[test]
    fn find_first_exact_match_at_start() {
        let hay = [0x48, 0x8b, 0xc4, 0x55, 0x53];
        assert_eq!(find_first(&hay, "48 8b c4"), Some(0));
    }

    #[test]
    fn find_first_exact_match_at_offset() {
        let hay = [0x90, 0x90, 0x48, 0x8b, 0xc4, 0x55, 0x53, 0x90];
        assert_eq!(find_first(&hay, "48 8b c4 55"), Some(2));
    }

    #[test]
    fn find_first_with_wildcards() {
        let hay = [0x90, 0x48, 0x8b, 0xc4, 0x55, 0x53, 0x90];
        assert_eq!(find_first(&hay, "48 8b ?? 55 53"), Some(1));
    }

    #[test]
    fn find_first_no_match() {
        let hay = [0x90, 0x90, 0x90, 0x90];
        assert_eq!(find_first(&hay, "48 8b c4"), None);
    }

    #[test]
    fn find_first_pattern_longer_than_haystack() {
        let hay = [0x48, 0x8b];
        assert_eq!(find_first(&hay, "48 8b c4 55"), None);
    }

    #[test]
    fn find_first_returns_earliest_match() {
        let hay = [0x48, 0x8b, 0x90, 0x48, 0x8b];
        assert_eq!(find_first(&hay, "48 8b"), Some(0));
    }

    #[test]
    fn find_first_at_haystack_end() {
        let hay = [0x90, 0x90, 0x48, 0x8b];
        assert_eq!(find_first(&hay, "48 8b"), Some(2));
    }

    #[test]
    fn find_all_multiple_matches() {
        let hay = [0x90, 0xab, 0xcd, 0x90, 0xab, 0xcd, 0x90];
        assert_eq!(find_all(&hay, "ab cd"), vec![1, 4]);
    }

    #[test]
    fn find_all_overlapping_matches() {
        let hay = [0xaa, 0xaa, 0xaa, 0xaa];
        assert_eq!(find_all(&hay, "aa aa"), vec![0, 1, 2]);
    }

    #[test]
    fn find_all_empty_on_no_match() {
        let hay = [0x90; 16];
        assert!(find_all(&hay, "12 34").is_empty());
    }

    #[test]
    fn pattern_matches_apply_gene_to_horse_prologue() {
        let hay = [
            0x90, 0x90, 0x90, 0x90, 0x48, 0x8b, 0xc4, 0x55, 0x53, 0x56, 0x57, 0x41, 0x54, 0x41,
            0x55, 0x41, 0x56, 0x41, 0x57, 0x48, 0x90, 0x90,
        ];
        assert_eq!(
            find_first(&hay, "48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57"),
            Some(4)
        );
    }

    #[test]
    fn pattern_matches_combinator_prologue_with_wildcards() {
        let hay = [
            0xcc, 0xcc, 0xcc, 0x48, 0x89, 0x5c, 0x24, 0x08, 0x48, 0x89, 0x6c, 0x24, 0x10, 0x48,
            0x89, 0x74, 0x24, 0x18, 0x57,
        ];
        assert_eq!(
            find_first(&hay, "48 89 5c 24 ?? 48 89 6c 24 ?? 48 89 74 24 ?? 57"),
            Some(3)
        );
    }
}

//! MSVC-specific runtime layout parsers + plausibility checks.
//!
//! These primitives encode invariants of MSVC-compiled x64 binaries
//! that any native-PE mod target inherits. Pure where possible; the
//! one HTTP-touching helper (`is_vtable_at_image_rva`) is a thin
//! `mem.peek` wrapper.

use crate::harness::RunningGame;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

/// Parsed MSVC `std::string` SSO header. Layout for the build we
/// reverse-engineered against:
///
/// - bytes  0..16 : inline string data (SSO), zero-padded if shorter
/// - bytes 16..24 : size (u64, little-endian)
/// - bytes 24..32 : capacity (u64, little-endian; 15 indicates SSO)
///
/// If `capacity > 15`, the string spilled to the heap and the first
/// 8 bytes of the inline area are actually the heap pointer.
#[derive(Debug, Clone)]
pub struct MsvcStdString {
    pub inline: [u8; 16],
    pub size: u64,
    pub capacity: u64,
}

impl MsvcStdString {
    /// Parse 32 bytes as a `std::string` header. Returns `None` if the
    /// input is too short.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 32 {
            return None;
        }
        let mut inline = [0u8; 16];
        inline.copy_from_slice(&bytes[0..16]);
        let size = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
        let capacity = u64::from_le_bytes(bytes[24..32].try_into().ok()?);
        Some(Self {
            inline,
            size,
            capacity,
        })
    }

    /// True if the string fits in the 16-byte inline area (SSO).
    pub fn is_sso(&self) -> bool {
        self.capacity <= 15
    }

    /// Decode the inline area as printable ASCII, replacing non-printable
    /// bytes with `.`. Truncated at the first NUL or at `size`.
    pub fn ascii_inline(&self) -> String {
        let len = (self.size as usize).min(16);
        self.inline[..len]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect()
    }

    /// Heap pointer if the string spilled out of SSO; otherwise None.
    pub fn heap_ptr(&self) -> Option<u64> {
        if self.is_sso() {
            return None;
        }
        Some(u64::from_le_bytes(self.inline[0..8].try_into().ok()?))
    }
}

/// Read 8 bytes at `obj_ptr` and compare to `image_base + expected_rva`.
/// Used as a plausibility check that `obj_ptr` is an instance of a
/// class whose vtable we've located via static RE. HLT-3 work item:
/// adopting this throughout horsey-mod's pointer resolvers.
pub fn is_vtable_at_image_rva(
    game: &RunningGame,
    obj_ptr: u64,
    image_base: u64,
    expected_rva: u64,
) -> Result<bool> {
    let resp = game
        .op_json("mem.peek", &json!({"addr": obj_ptr, "kind": "u64"}))
        .map_err(|e| anyhow!("mem.peek: {e}"))?;
    let s = resp
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("mem.peek u64 returned no string value"))?;
    let vtbl = u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
        .context("vtable hex parse")?;
    Ok(vtbl == image_base.wrapping_add(expected_rva))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sso_parse_round_trip() {
        let mut buf = [0u8; 32];
        buf[..7].copy_from_slice(b"tomtato");
        buf[16..24].copy_from_slice(&7u64.to_le_bytes());
        buf[24..32].copy_from_slice(&15u64.to_le_bytes());
        let s = MsvcStdString::parse(&buf).unwrap();
        assert_eq!(s.size, 7);
        assert_eq!(s.capacity, 15);
        assert!(s.is_sso());
        assert_eq!(s.ascii_inline(), "tomtato");
        assert!(s.heap_ptr().is_none());
    }

    #[test]
    fn heap_spilled_returns_ptr() {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&0x1234_5678_dead_beefu64.to_le_bytes());
        buf[16..24].copy_from_slice(&64u64.to_le_bytes());
        buf[24..32].copy_from_slice(&127u64.to_le_bytes());
        let s = MsvcStdString::parse(&buf).unwrap();
        assert!(!s.is_sso());
        assert_eq!(s.heap_ptr(), Some(0x1234_5678_dead_beef));
    }
}

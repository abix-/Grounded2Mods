//! JSON parsing helpers used across testkit consumers.
//!
//! All HTTP cmdlets return JSON; addresses and offsets are encoded as
//! hex strings (`"0x1403fb0d8"`). These helpers parse them safely.

use serde_json::Value;

/// Read `v[key]` as a hex string and parse to u64. None if the key is
/// missing, the value isn't a string, or the string isn't valid hex.
pub fn u64_at_key(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_str).and_then(|s| {
        u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
    })
}

/// Parse a hex string (`0x...`) or decimal string to u64. Tolerates
/// optional `0x`/`0X` prefix.
pub fn parse_u64_hex_or_dec(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).ok();
    }
    s.parse().ok()
}

/// Same as `parse_u64_hex_or_dec` but for usize.
pub fn parse_usize_hex_or_dec(s: &str) -> Option<usize> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return usize::from_str_radix(h, 16).ok();
    }
    s.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn u64_at_key_parses_hex_string() {
        let v = json!({"addr": "0x1403fb0d8", "other": "garbage"});
        assert_eq!(u64_at_key(&v, "addr"), Some(0x1403fb0d8));
        assert_eq!(u64_at_key(&v, "missing"), None);
        assert_eq!(u64_at_key(&v, "other"), None);
    }

    #[test]
    fn parse_hex_or_dec_both_forms() {
        assert_eq!(parse_u64_hex_or_dec("0x140"), Some(0x140));
        assert_eq!(parse_u64_hex_or_dec("320"), Some(320));
        assert_eq!(parse_u64_hex_or_dec("0X1F"), Some(0x1f));
        assert_eq!(parse_u64_hex_or_dec(""), None);
    }
}

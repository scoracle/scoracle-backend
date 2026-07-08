//! Small shared helpers.

use sha2::{Digest, Sha256};

/// truncate bounds a string to at most `max` bytes, backing off to the nearest
/// UTF-8 char boundary (slicing mid-codepoint would panic). Used to cap error
/// strings before they land in `pipeline_work.last_error` and to clip response
/// bodies in log/error messages.
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// truncate_bytes clips a string to at most `max` BYTES then appends "..." — the exact
/// behaviour of the Go `ml.truncate` (`s[:max] + "..."`): it slices raw bytes (it may split a
/// multi-byte codepoint), and on the JSON-marshal that follows Go replaces the partial tail with
/// U+FFFD — which `from_utf8_lossy` reproduces, so the wire prompt matches byte-for-byte (and is
/// identical for ASCII). Use this for any value rendered INTO a model prompt (transfer news
/// descriptions, summaries) so the built-prompt bytes stay parity-equal to Go; use [`truncate`]
/// (char-boundary, no ellipsis) only for internal log/error clipping where Go parity is moot.
pub fn truncate_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = String::from_utf8_lossy(&s.as_bytes()[..max]).into_owned();
    out.push_str("...");
    out
}

/// round1 rounds to one decimal place. Mirrors Go's `round1` (`math.Round(x*10)/10`); Rust's
/// `f64::round` rounds half away from zero, like `math.Round`. Single-homed here (was a
/// byte-identical private copy in both sigil.rs and rating.rs — see plan A6): its `round1`'d
/// values feed `go_json_float` into each stage's canonical `input_components` JSON → the
/// `input_hash`, so the shared definition keeps that debounce axis identical across stages.
pub fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

// ---------------------------------------------------------------------------
// Go `encoding/json`-compatible primitives — the shared building blocks for an `input_hash` that
// matches Go's `hashComponents` byte-for-byte (the strict debounce parity axis: sigil's `input_hash`,
// rating's `input_hash`). Each stage assembles its OWN canonical JSON (the structure differs), but
// the leaf encoding (string escaping, float form) + the hash MUST be identical, so they live here.
// First single-homed for L12 (rating); sigil.rs still carries byte-identical private copies — a
// noted one-line cleanup (migrate sigil to these), deferred to avoid perturbing the proven L3 stage.
// ---------------------------------------------------------------------------

/// go_json_string quotes + escapes a string EXACTLY as Go's `encoding/json` does by default
/// (HTMLEscape on): `"` `\` and the control chars `\n` `\r` `\t` get short escapes, every other byte
/// < 0x20 becomes `\u00XX` (lowercase hex), `<` `>` `&` become `<`/`>`/`&`, and
/// U+2028 / U+2029 become ` ` / ` `. Everything else passes through as UTF-8. This is what
/// makes a SHA-256 over the canonical components match Go's `input_hash`.
pub(crate) fn go_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// go_json_float renders an f64 EXACTLY as Go's `encoding/json` does for our domain. Go uses
/// `strconv.AppendFloat(f, 'f', -1, 64)` for |f| in [1e-6, 1e21) — the shortest positional form with
/// NO trailing ".0". Our inputs are `round1` of percentiles / T-scores (0..100-ish), so Rust's f64
/// `Display` (also shortest-positional, also no ".0") matches Go byte-for-byte. (serde_json would
/// emit "85.0"; this deliberately does not.)
pub(crate) fn go_json_float(f: f64) -> String {
    format!("{f}")
}

/// hash_components is the stable hash of a canonical components JSON — the debounce signal. Mirrors
/// Go's `hashComponents`: SHA-256, then the lowercase hex of the first 16 bytes (a 128-bit prefix is
/// ample for a change signal). The caller passes the EXACT bytes Go's `json.Marshal` produced (built
/// with [`go_json_string`] / [`go_json_float`]), so the hex equals Go's `input_hash`.
pub(crate) fn hash_components(canonical_json: &str) -> String {
    let digest = Sha256::digest(canonical_json.as_bytes());
    hex::encode(&digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_json_float_omits_trailing_zero() {
        // Integral floats render with NO ".0" (Go's shortest 'f' form), fractions keep their digits.
        assert_eq!(go_json_float(73.0), "73");
        assert_eq!(go_json_float(72.4), "72.4");
        assert_eq!(go_json_float(0.0), "0");
        assert_eq!(go_json_float(100.0), "100");
        assert_eq!(go_json_float(0.4), "0.4");
    }

    #[test]
    fn go_json_string_html_escapes_like_go() {
        let bs = "\\"; // a single backslash, to spell the expected escape forms unambiguously
        assert_eq!(go_json_string("A & B"), format!("\"A {bs}u0026 B\""));
        assert_eq!(go_json_string("<x>"), format!("\"{bs}u003cx{bs}u003e\""));
        assert_eq!(go_json_string("a\"b\nc"), r#""a\"b\nc""#);
        assert_eq!(go_json_string("plain"), r#""plain""#);
    }

    #[test]
    fn hash_components_is_deterministic_and_128_bit() {
        let json = r#"{"a":1,"b":"x"}"#;
        let h = hash_components(json);
        assert_eq!(h.len(), 32); // 16 bytes → 32 hex chars
        assert_eq!(h, hash_components(json)); // deterministic
        assert_ne!(h, hash_components(r#"{"a":2,"b":"x"}"#)); // sensitive to content
    }
}

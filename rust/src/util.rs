//! Small shared helpers.

use sha2::{Digest, Sha256};

/// Removes paired `**`/`__` and leading bullet/heading/quote decoration; leaves inner text and
/// ordinary apostrophes or underscores in words alone.
pub fn strip_markdown_emphasis(line: &str) -> String {
    let t = line.trim();
    // Leading list/heading/quote markers, e.g. "- **SCORE:** 4" or "### READ:".
    let t = t.trim_start_matches(['#', '>', '-', '+']);
    let mut out = String::with_capacity(t.len());
    let bytes = t.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Paired emphasis runs: ** or __ (and *** / ___ by repetition).
        if bytes[i] == b'*' || bytes[i] == b'_' {
            let c = bytes[i];
            let mut run = 0;
            while i + run < bytes.len() && bytes[i + run] == c {
                run += 1;
            }
            // A single `_` inside a word is part of the word (snake_case); a single `*` at a
            // label boundary is decoration. Drop runs of 2+, and single `*` always.
            if run >= 2 || c == b'*' {
                i += run;
                continue;
            }
        }
        // Not decoration — copy this char whole (UTF-8 safe).
        let ch_len = t[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        out.push_str(&t[i..i + ch_len]);
        i += ch_len;
    }
    out.trim().to_string()
}

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

/// Clip model-facing text to `max` bytes and append an ellipsis. A split UTF-8 code point becomes
/// the replacement character rather than panicking.
pub fn truncate_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = String::from_utf8_lossy(&s.as_bytes()[..max]).into_owned();
    out.push_str("...");
    out
}

/// Round to one decimal place for compact model inputs and debounce components.
pub fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Stable 128-bit debounce fingerprint of a canonical JSON pre-image.
pub(crate) fn hash_components(canonical_json: &str) -> String {
    let digest = Sha256::digest(canonical_json.as_bytes());
    hex::encode(&digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_components_is_deterministic_and_128_bit() {
        let json = r#"{"a":1,"b":"x"}"#;
        let h = hash_components(json);
        assert_eq!(h.len(), 32); // 16 bytes → 32 hex chars
        assert_eq!(h, hash_components(json)); // deterministic
        assert_ne!(h, hash_components(r#"{"a":2,"b":"x"}"#)); // sensitive to content
    }

    // --- strip_markdown_emphasis ---

    #[test]
    fn strips_the_exact_line_that_broke_momentum_in_production() {
        // Verbatim from pipeline_work.last_error after the topology split.
        assert_eq!(strip_markdown_emphasis("**SCORE: -1**"), "SCORE: -1");
        assert_eq!(
            strip_markdown_emphasis("**READ:** Clark's **form** remains flat"),
            "READ: Clark's form remains flat"
        );
    }

    #[test]
    fn strips_bullet_heading_and_quote_decoration() {
        assert_eq!(strip_markdown_emphasis("- **SCORE:** 4"), "SCORE: 4");
        assert_eq!(strip_markdown_emphasis("### READ: steady"), "READ: steady");
        assert_eq!(strip_markdown_emphasis("> VIBE: buzzing"), "VIBE: buzzing");
        assert_eq!(
            strip_markdown_emphasis("__HOOK:__ big night"),
            "HOOK: big night"
        );
    }

    #[test]
    fn leaves_undecorated_lines_byte_identical() {
        // The pre-split shape must be untouched, or this fix would itself be a regression.
        for s in [
            "SCORE: 62",
            "READ: steady form, no change",
            "HOOK: Arsenal surge",
        ] {
            assert_eq!(strip_markdown_emphasis(s), s);
        }
    }

    #[test]
    fn preserves_apostrophes_underscores_in_words_and_non_ascii() {
        assert_eq!(
            strip_markdown_emphasis("READ: Clark's form"),
            "READ: Clark's form"
        );
        assert_eq!(
            strip_markdown_emphasis("READ: snake_case stays"),
            "READ: snake_case stays"
        );
        // Em dash and accents must survive byte-for-byte (the parser slices by byte offset).
        assert_eq!(
            strip_markdown_emphasis("**READ:** Guimarães — flat"),
            "READ: Guimarães — flat"
        );
    }

    #[test]
    fn a_hyphenated_read_line_is_not_eaten_as_a_bullet() {
        // Leading '-' is bullet decoration, but a negative score after the label must survive.
        assert_eq!(strip_markdown_emphasis("SCORE: -12"), "SCORE: -12");
    }
}

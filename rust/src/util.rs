//! Small shared helpers.

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

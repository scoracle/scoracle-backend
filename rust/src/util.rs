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

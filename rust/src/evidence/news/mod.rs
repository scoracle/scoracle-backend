//! Shared article evidence, compiled storyline packets and bounded rendering.
pub mod packet;
pub mod render;

const QUOTE_WINDOW_CHARS: usize = 160;
/// slice_quote extracts ±QUOTE_WINDOW_CHARS around the name's first occurrence in the
/// stored body — char-boundary safe (the İ lesson from fetch.rs: never index the original
/// with offsets from a lowercased copy). Case-insensitive via per-char primary fold; a
/// name whose fold expands (rare) simply misses and yields None — a lost quote, never a
/// panic.
pub(crate) fn slice_quote(body: &str, name: &str) -> Option<String> {
    let needle: Vec<char> = name.chars().flat_map(|c| c.to_lowercase().next()).collect();
    if needle.is_empty() {
        return None;
    }
    let hay: Vec<(usize, char)> = body
        .char_indices()
        .map(|(i, c)| (i, c.to_lowercase().next().unwrap_or(c)))
        .collect();
    if hay.len() < needle.len() {
        return None;
    }
    let mut start_char_idx = None;
    'outer: for i in 0..=hay.len() - needle.len() {
        for (j, nc) in needle.iter().enumerate() {
            if hay[i + j].1 != *nc {
                continue 'outer;
            }
        }
        start_char_idx = Some(i);
        break;
    }
    let start_char_idx = start_char_idx?;
    let from_char = start_char_idx.saturating_sub(QUOTE_WINDOW_CHARS);
    let to_char = (start_char_idx + needle.len() + QUOTE_WINDOW_CHARS).min(hay.len());
    let from_byte = hay[from_char].0;
    let to_byte = if to_char == hay.len() {
        body.len()
    } else {
        hay[to_char].0
    };
    let quote = body[from_byte..to_byte].trim();
    if quote.is_empty() {
        None
    } else {
        Some(quote.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_slices_a_window_around_the_first_occurrence() {
        let body = format!(
            "{} Yan Diomande scored twice. {}",
            "x".repeat(500),
            "y".repeat(500)
        );
        let q = slice_quote(&body, "yan diomande").unwrap();
        assert!(q.contains("Yan Diomande scored twice."));
        // 160 chars each side + the name ≈ bounded window, never the whole body.
        assert!(q.chars().count() <= 2 * QUOTE_WINDOW_CHARS + 30);
    }

    #[test]
    fn quote_is_case_insensitive_and_boundary_safe() {
        let body = "İstanbul derby: Mauro İcardi opened the scoring for Galatasaray.";
        let q = slice_quote(body, "mauro icardi");
        // Dotted-capital fold may or may not match the ASCII needle char-for-char; the
        // contract is NO PANIC and a sensible Option either way.
        if let Some(q) = q {
            assert!(q.contains("Galatasaray"));
        }
        assert!(slice_quote(body, "Galatasaray")
            .unwrap()
            .contains("Galatasaray"));
    }

    #[test]
    fn quote_misses_yield_none() {
        assert_eq!(slice_quote("short body", "vinicius"), None);
        assert_eq!(slice_quote("", "vinicius"), None);
        assert_eq!(slice_quote("body", ""), None);
    }
}

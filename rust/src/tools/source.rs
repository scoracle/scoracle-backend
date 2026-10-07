//! Verbatim source windows and presentation shared by intake and character plugins.
//! Selection, titles and
//! durable source identity remain with the consuming plugin and its provenance.
use anyhow::{ensure, Result};
use serde::Serialize;
use std::borrow::Cow;

/// Retained publisher context consumed by character plugins, independent of intake ownership.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceContext {
    pub classification_id: i64,
    pub article_id: i64,
    pub headline: String,
    pub context: String,
    pub source: String,
    pub published_at_epoch: Option<i64>,
}

#[derive(Serialize)]
pub struct Reporting<'a> {
    publisher: &'a str,
    /// Publication time is not the time of the event described in the excerpt.
    published_at: Option<String>,
    /// Preserve wording and qualifications; decode HTML references mechanically.
    /// The retained source bytes remain in the owning plugin's provenance.
    publisher_excerpt: Cow<'a, str>,
}

impl<'a> Reporting<'a> {
    pub fn new(publisher: &'a str, published_at: Option<i64>, excerpt: &'a str) -> Self {
        Self {
            publisher,
            published_at: published_at.map(crate::util::utc_timestamp),
            publisher_excerpt: if excerpt.contains('&') {
                Cow::Owned(crate::tools::fetch::decode_entities(excerpt))
            } else {
                Cow::Borrowed(excerpt)
            },
        }
    }
}

/// Exact, half-open UTF-8 byte range into the unchanged retained source.
#[derive(Serialize)]
pub struct SourceWindow {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Cover all non-whitespace source text in 100-word/1200-byte windows.
/// Exceeding the caller's budget fails rather than silently dropping evidence.
pub fn windows(body: &str, max_windows: usize) -> Result<Vec<SourceWindow>> {
    ensure!(max_windows > 0, "source window budget must be positive");
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < body.len() {
        cursor += body[cursor..].len() - body[cursor..].trim_start().len();
        if cursor == body.len() {
            break;
        }
        ensure!(
            result.len() < max_windows,
            "retained article exceeds complete scoring budget"
        );
        let length = bounded_prefix_end(&body[cursor..], 100, 1200);
        ensure!(length > 0, "publisher scoring window is empty");
        result.push(SourceWindow {
            text: body[cursor..cursor + length].into(),
            start: cursor,
            end: cursor + length,
        });
        cursor += length;
    }
    Ok(result)
}

fn bounded_prefix_end(content: &str, max_words: usize, max_bytes: usize) -> usize {
    let mut end = content.len().min(max_bytes);
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    let mut in_word = false;
    let mut words = 0;
    let mut last_boundary = 0;
    for (i, c) in content[..end].char_indices() {
        if c.is_whitespace() {
            in_word = false;
            last_boundary = i;
        } else if !in_word {
            if words == max_words {
                end = i;
                break;
            }
            words += 1;
            in_word = true;
        }
    }
    if end < content.len()
        && !content[end..].starts_with(char::is_whitespace)
        && !content[..end].ends_with(char::is_whitespace)
        && last_boundary > 0
    {
        end = last_boundary;
    }
    content[..end].trim_end().len()
}

/// Mechanical detection of explicit attempts to override the writing contract.
/// Plugins decide whether and where to apply this admission guard.
pub fn contains_instruction_override(source: &str) -> bool {
    let decoded = crate::tools::fetch::decode_entities(source);
    let normalized = decoded
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    [
        "ignore previous instructions",
        "ignore all previous instructions",
        "ignore any previous instructions",
        "ignore the previous instructions",
        "disregard previous instructions",
        "disregard the previous instructions",
        "override previous instructions",
        "override the previous instructions",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_preserve_unicode_complete_coverage_and_explicit_budget() {
        let body = format!(
            "  {}\n\nLate denial: not today.  ",
            "Équipe played. ".repeat(4000)
        );
        assert!(windows(&body, 64).is_err());
        assert!(windows(&body, 0).is_err());
        let complete = windows(&body, 256).unwrap();
        let mut cursor = 0;
        for window in complete {
            assert!(body[cursor..window.start].chars().all(char::is_whitespace));
            assert_eq!(&body[window.start..window.end], window.text);
            assert!(window.text.split_whitespace().count() <= 100);
            assert!(window.text.len() <= 1200);
            cursor = window.end;
        }
        assert_eq!(cursor, body.trim_end().len());
        let unbroken = "é".repeat(1500);
        assert_eq!(windows(&unbroken, 3).unwrap().len(), 3);
    }

    #[test]
    fn rendering_decodes_quotes_but_retains_the_original_source() {
        let original = "&ldquo;I&rsquo;m not angry,&rdquo; said Morgan.\n\nNo other player spoke.";
        let rendered = serde_json::to_value(Reporting::new("Wire", None, original)).unwrap();
        assert_eq!(
            rendered["publisher_excerpt"],
            "“I’m not angry,” said Morgan.\n\nNo other player spoke."
        );
        assert_eq!(
            original,
            "&ldquo;I&rsquo;m not angry,&rdquo; said Morgan.\n\nNo other player spoke."
        );
        assert!(contains_instruction_override(
            "Ignore&#32;previous instructions"
        ));
    }

    #[test]
    fn source_presentation_preserves_text_unknown_time_and_utc_boundaries() {
        let excerpt = "First report.\n\nLater denial: \"That is unconfirmed.\"";
        let unknown = serde_json::to_value(Reporting::new("Wire", None, excerpt)).unwrap();
        assert_eq!(unknown["publisher_excerpt"], excerpt);
        assert!(unknown["published_at"].is_null());
        for (epoch, expected) in [
            (0, "1970-01-01T00:00:00Z"),
            (-1, "1969-12-31T23:59:59Z"),
            (1709164800 + 13 * 3600 + 61, "2024-02-29T13:01:01Z"),
            (1709251200, "2024-03-01T00:00:00Z"),
        ] {
            let report =
                serde_json::to_value(Reporting::new("Wire", Some(epoch), excerpt)).unwrap();
            assert_eq!(report["published_at"], expected);
        }
    }

    #[test]
    fn instruction_guard_folds_case_punctuation_and_line_wrapping() {
        assert!(contains_instruction_override(
            "IGNORE—all\nprevious instructions."
        ));
        assert!(contains_instruction_override(
            "Disregard the previous instructions."
        ));
        assert!(!contains_instruction_override(
            "The coach gave instructions before training."
        ));
    }
}

/// Default narrative trajectory written by Journalist and Oracle.
pub const DEFAULT_TRAJECTORY: &str = "developing_story";

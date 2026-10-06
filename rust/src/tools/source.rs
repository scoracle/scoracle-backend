//! Source presentation shared by articulation plugins. Selection, titles and
//! durable source identity remain with the consuming plugin and its provenance.
use serde::Serialize;

#[derive(Serialize)]
pub struct Reporting<'a> {
    publisher: &'a str,
    /// Publication time is not the time of the event described in the excerpt.
    published_at: Option<String>,
    /// Preserve the intact evidence unit, including its qualifications.
    publisher_excerpt: &'a str,
}

impl<'a> Reporting<'a> {
    pub fn new(publisher: &'a str, published_at: Option<i64>, excerpt: &'a str) -> Self {
        Self {
            publisher,
            published_at: published_at.map(crate::util::utc_timestamp),
            publisher_excerpt: excerpt,
        }
    }
}

/// Mechanical detection of explicit attempts to override the writing contract.
/// Plugins decide whether and where to apply this admission guard.
pub fn contains_instruction_override(source: &str) -> bool {
    let normalized = source
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

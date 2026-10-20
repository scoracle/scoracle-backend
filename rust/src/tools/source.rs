//! Source identities and instruction guards shared by headline character plugins.
/// Retained publisher context consumed by character plugins, independent of intake ownership.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceContext {
    pub classification_id: i64,
    pub article_id: i64,
    pub headline: String,
    pub context: String,
    pub source: String,
    pub published_at_epoch: Option<i64>,
    /// Native, source-bound relationships; absent on historical intake receipts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifier_world: Option<serde_json::Value>,
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

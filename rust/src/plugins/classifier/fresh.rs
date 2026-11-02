//! Source-only decision context and the rules for interpreting it.
use serde_json::{json, Value};

pub(crate) fn context(
    headline: &str,
    publisher: &str,
    sport: &str,
    meta: &Value,
) -> crate::tools::prompt::Context<Value> {
    crate::tools::prompt::Context {
        guide: crate::tools::prompt::Guide {
            relationship: "Use subject metadata only to identify the entity. Judge reporting from the RSS headline; use its publisher name only to disambiguate the subject. Publisher identity alone does not establish relevance, emotion or movement.",
            fields: &[],
        },
        data: json!({"subject_meta":meta,"article":{"sport":sport,"headline":headline,"publisher":publisher}}),
    }
}

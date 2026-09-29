//! Shared instructions retained by character paths awaiting plugin alignment.
//! Form owns structure; this module owns legacy writing and correction directions.
//! Journalist uses its own narrow prompt.rs and never composes this instruction stack.
use super::form::{SurfaceError, BODY_MAX_CHARS, HOOK_MAX_CHARS};

pub const IDENTITY_CARD_FRAMING: &str = "Identity context, not event evidence. Distinguish current roles from career history; dated reporting may supersede these records. Unknown means unknown.";

pub const OBSERVATION_SCOPE: &str = "Express the source-backed observations selected by the plugin. Their attribution, dates, scope and uncertainty are part of the observation. An ordinary observation is enough; significance does not need to be added.";

/// Enabled only for characters whose parser and publication path accept a called pass.
pub const ABSTENTION: &str = "If the supplied material contains no observation within your character scope, return JSON null instead of a card. Passing is a complete response. The card structure applies when there is a supported observation to express.";

pub const EVIDENCE_SCOPE: &str = "Describe the supplied observations. A partial profile can be a complete reading. Keep explanations, relationships and trends within the supplied context. A measured zero or an observed absence is a finding; a missing measurement or report is unknown. Without a supported comparison, direction is unknown, not unchanged. Mention a gap when it changes the interpretation; otherwise leave it open. Stop when the supported story is told.";

pub const STORY_FORM: &str = "Express the selected observations as a coherent read, preserving the connections supplied with them. Use paragraphs where the story turns, separated by a blank line; no headings or repeated conclusion.";

pub const WIRE_COPY: &str = "Write plain sporting prose in the character's voice. Preserve uncertainty. Prior readings offer continuity, not new evidence.";

pub const CHARACTER_SCOPE: &str = "Express the supplied observations in your character's voice; a complete summary of the entity is unnecessary. The reader already has the identity card: use metadata as context, without biographical introductions or listings of team, position, season or appearances. Mention an identity detail only when it explains a relevant development.";

pub const HOOK: &str =
    "The hook names the entity and states the card's main observation without changing its timing.";

#[derive(Clone, Copy, Debug)]
pub enum CardFormat {
    Scout,
    Analyst,
    Insider,
    Oracle,
}

/// Existing user-facing card correction policy. The Studio owns the three-attempt
/// bound; publishing plugins own the instruction appended after a correctable failure.
pub fn publishing_correction(error: &anyhow::Error) -> Option<String> {
    if error.is::<SurfaceError>() {
        return Some(format!(
            "{error} Rewrite from scratch as one compact paragraph. Keep only the main finding and one supporting detail. Target at most 500 body characters so the complete JSON fits. Do not enumerate every input."
        ));
    }
    if error.is::<crate::studio::model::IncompleteOutput>() {
        return Some("the response ran out of space. Rewrite from scratch as one compact paragraph. Keep only the main finding and one supporting detail. Target at most 500 body characters so the complete JSON fits. Do not enumerate every input.".to_string());
    }
    None
}

/// Correction policy for structured internal tasks. It retains the existing retry on
/// provider truncation without asking a typed extraction to write card prose.
pub fn structured_correction(error: &anyhow::Error) -> Option<String> {
    error
        .is::<crate::studio::model::IncompleteOutput>()
        .then(|| "the response was truncated. Return the complete requested JSON object from scratch, preserving the supplied evidence and schema.".to_string())
}

/// Compose the system instruction from the character and shared form.
pub fn compose(character: &str, format: CardFormat) -> String {
    let output = match format {
        CardFormat::Scout | CardFormat::Analyst => "Return JSON with headline (the hook) and body (the paragraphs). Preserve paragraph breaks as escaped newlines.",
        CardFormat::Insider => "Return JSON with read containing the body, headline containing the hook, and score containing an integer from 1 to 99. Preserve paragraph breaks inside read as escaped newlines.",
        CardFormat::Oracle => "Return JSON with reading containing the body, headline containing the hook, and score containing an integer from 1 to 100. Open the reading with this entity's supplied name and speak directly about its circumstances as one interpretation. The reading is body only: do not describe its hook or headline, the evidence structure, its speakers, computation or JSON fields. Preserve paragraph breaks inside reading as escaped newlines.",
    };
    let abstention = if matches!(format, CardFormat::Scout) {
        ABSTENTION
    } else {
        ""
    };
    let canvas = format!("Card surface: hook ≤{HOOK_MAX_CHARS} characters; body ≤{BODY_MAX_CHARS}, including spaces. These are ceilings, not targets. Choose what earns the space; finish your sentences. Multiple narrative bodies share the body allowance.");
    format!("{character}\n\n{canvas}\n\n{CHARACTER_SCOPE}\n\n{EVIDENCE_SCOPE}\n\n{OBSERVATION_SCOPE}\n\n{STORY_FORM}\n\n{WIRE_COPY}\n\n{HOOK}\n\n{output}\n\n{abstention}")
}

/// Existing character schema descriptions are instructions, separate from shape.
/// Journalist does not use this legacy composition path.
pub fn card_schema(scored: bool) -> serde_json::Value {
    let mut schema = super::form::card_schema(scored);
    schema["properties"]["headline"]["description"] =
        serde_json::json!("The read's main finding, stated as a sentence naming the entity.");
    schema["properties"]["body"]["description"] = serde_json::json!(format!("The character's articulation of the supplied observations, within {BODY_MAX_CHARS} characters."));
    schema
}

pub fn oracle_format_schema() -> serde_json::Value {
    let mut schema = super::form::oracle_format_schema();
    schema["properties"]["reading"]["description"] = serde_json::json!("Unified prose about the entity and its circumstances, without card, field, computation or score commentary");
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::analyst::cognition as analyst;
    use crate::plugins::influencer::cognition as influencer;
    use crate::plugins::insider::cognition as insider;
    use crate::plugins::journalist::cognition as journalist;
    use crate::plugins::oracle::cognition as oracle;
    use crate::plugins::scout::cognition as scout;
    use crate::plugins::support::form::journalist_schema;
    #[test]
    fn remaining_characters_keep_their_shared_instruction_composition() {
        let characters = [
            (scout::CHARACTER, scout::RATING_SYSTEM_PROMPT.as_str()),
            (
                analyst::prompt::CHARACTER,
                analyst::prompt::MOMENTUM_SYSTEM_PROMPT.as_str(),
            ),
            (
                insider::CHARACTER,
                insider::INSIDER_SCORE_SYSTEM_PROMPT.as_str(),
            ),
            (oracle::CHARACTER, oracle::ORACLE_SYSTEM_PROMPT.as_str()),
        ];
        for (brief, system) in characters {
            assert!(!brief.trim().is_empty());
            assert!(brief.split_whitespace().count() <= 200);
            for shared in [
                STORY_FORM,
                OBSERVATION_SCOPE,
                HOOK,
                WIRE_COPY,
                CHARACTER_SCOPE,
                EVIDENCE_SCOPE,
            ] {
                assert_eq!(system.matches(shared).count(), 1);
            }
            for retired in [
                "TWO OR THREE",
                "EIGHT SENTENCES",
                "Harborview",
                "Strengths:",
                "Limitations:",
                "Summary:",
                "one to three sentences",
            ] {
                assert!(!system.contains(retired), "retired instruction: {retired}");
            }
        }
        // The keyed report surface is generated from the same declaration the
        // validator enforces, so it is a flat map of the plugin's own keys.
        assert_eq!(
            journalist_schema(2)["required"],
            serde_json::json!(["report_1", "report_2"])
        );
        assert!(influencer::VIBE_SYSTEM_PROMPT.contains("publisher_excerpt"));
        assert!(journalist::NARRATIVES_SYSTEM_PROMPT.starts_with("Articulate each fresh item"));
        assert!(!journalist::NARRATIVES_SYSTEM_PROMPT.contains("Choose the most meaningful claims"));
        assert!(journalist_schema(1)["additionalProperties"] == serde_json::json!(false));
        assert!(journalist_schema(1)["properties"]
            .get("card_score")
            .is_none());
        assert!(oracle::ORACLE_SYSTEM_PROMPT
            .contains("Open the reading with this entity's supplied name"));
        // Character ceilings guide composition and are checked after decoding. A grammar
        // maxLength can force a string closed in the middle of a word or sentence.
        assert!(oracle_format_schema()["properties"]["reading"]["maxLength"].is_null());
    }
}

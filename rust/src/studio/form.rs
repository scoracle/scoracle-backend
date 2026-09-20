//! Shared character form and parser-compatible output contracts.
//!
//! The reader sees a hook header and a body. Transport labels and JSON fields below
//! retain the existing parser/storage contracts; they are not section headings.
//! Character files own voice and judgment. Inputs supply evidence, not an outline.

pub const IDENTITY_CARD_FRAMING: &str = "Identity context, not event evidence. Distinguish current roles from career history; dated reporting may supersede these records. Unknown means unknown.";

pub const CLAIM_SELECTION: &str = "Choose the most meaningful claims supported by the evidence. Ordinary, unchanged and uncertain findings are valid.";

pub const STORY_FORM: &str = "Connect the selected findings, evidence and meaning into a coherent read. Use paragraphs where the story turns, separated by a blank line; no headings or repeated conclusion.";

pub const WIRE_COPY: &str = "Write plain sporting prose in the character's voice. Preserve uncertainty. Prior readings offer continuity, not new evidence.";

pub const HOOK: &str =
    "The hook names the entity and states the card's main finding in present tense.";

#[derive(Clone, Copy, Debug)]
pub enum CardFormat {
    Scout,
    Analyst,
    Influencer,
    Journalist,
    Insider,
    Oracle,
}

/// Reader-facing dimensions, independent of any model's tokenization or runtime budget.
pub const HOOK_MAX_CHARS: usize = 140;
pub const BODY_MAX_CHARS: usize = 1200;
pub const ORACLE_READING_MAX_CHARS: usize = BODY_MAX_CHARS;

#[derive(Debug)]
pub struct SurfaceError(pub String);

impl std::fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SurfaceError {}

pub fn validate_body(body: &str) -> anyhow::Result<()> {
    let chars = body.chars().count();
    if body.trim().is_empty() || chars > BODY_MAX_CHARS {
        return Err(SurfaceError(format!(
            "Body has {chars} characters; write a complete body within {BODY_MAX_CHARS}."
        ))
        .into());
    }
    Ok(())
}

pub fn validate_hook(hook: Option<&str>) -> anyhow::Result<()> {
    if let Some(hook) = hook {
        if hook.trim().is_empty() || hook.chars().count() > HOOK_MAX_CHARS {
            return Err(SurfaceError(format!(
                "Write the main finding as a hook within {HOOK_MAX_CHARS} characters."
            ))
            .into());
        }
    }
    Ok(())
}

/// Compose the system instruction from the character and shared form.
pub fn compose(character: &str, format: CardFormat) -> String {
    let output = match format {
        CardFormat::Scout | CardFormat::Analyst => "Return JSON with headline (the hook) and body (the paragraphs). Preserve paragraph breaks as escaped newlines.",
        CardFormat::Influencer => "Return JSON with headline (the hook), body (the paragraphs) and score (an integer from 1 to 100). Preserve paragraph breaks as escaped newlines.",
        CardFormat::Journalist => "Return JSON with narratives, headline and card_score. Each narrative has a short specific title that names this entity as supplied, a body following the shared form, and articles containing its supporting input article numbers. Select relevant stories, most consequential first; an empty narratives array is valid. The headline is the hook for the whole edition. The card_score is an integer from 1 to 99. Preserve paragraph breaks inside body strings as escaped newlines.",
        CardFormat::Insider => "Return JSON with read containing the body, headline containing the hook, and score containing an integer from 1 to 99. Preserve paragraph breaks inside read as escaped newlines.",
        CardFormat::Oracle => "Return JSON with reading containing the body, headline containing the hook, and score containing an integer from 1 to 100. Open the reading with this entity's supplied name and speak directly about its circumstances as one interpretation. The reading is body only: do not describe its hook or headline, the evidence structure, its speakers, computation or JSON fields. Preserve paragraph breaks inside reading as escaped newlines.",
    };
    let canvas = format!("Card surface: hook ≤{HOOK_MAX_CHARS} characters; body ≤{BODY_MAX_CHARS}, including spaces. These are ceilings, not targets. Choose what earns the space; finish your sentences. Multiple narrative bodies share the body allowance.");
    format!("{character}\n\n{canvas}\n\n{CLAIM_SELECTION}\n\n{STORY_FORM}\n\n{WIRE_COPY}\n\n{HOOK}\n\n{output}")
}

/// Fold line wrapping and whitespace while retaining the claim paragraphs.
pub fn normalize_body(body: &str) -> String {
    let mut paragraphs = Vec::new();
    let mut paragraph = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            if !paragraph.is_empty() {
                paragraphs.push(paragraph.join(" "));
                paragraph.clear();
            }
        } else {
            paragraph.extend(line.split_whitespace());
        }
    }
    if !paragraph.is_empty() {
        paragraphs.push(paragraph.join(" "));
    }
    paragraphs.join("\n\n")
}

#[derive(serde::Deserialize)]
pub struct CardReply {
    pub headline: String,
    pub body: String,
    pub score: Option<i32>,
}

/// Shape only. The surface belongs in the prompt and post-decode validation;
/// constraining a string's maximum length can force a word to end midway.
pub fn card_schema(scored: bool) -> serde_json::Value {
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": {
            "headline": {"type":"string", "description":"The read's main finding, stated as a sentence naming the entity."},
            "body": {"type":"string", "description":format!("The character's interpretation of the evidence, within {BODY_MAX_CHARS} characters.")}
        },
        "required": ["headline", "body"]
    });
    if scored {
        schema["properties"]["score"] =
            serde_json::json!({"type":"integer", "minimum":1, "maximum":100});
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!("score"));
    }
    schema
}

pub fn narratives_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "narratives": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title":    { "type": "string" },
                        "body":     { "type": "string" },
                        "articles": { "type": "array", "items": { "type": "integer" } }
                    },
                    "required": ["title", "body", "articles"]
                }
            },
            "headline": { "type": "string" },
            "card_score": { "type": "integer", "minimum": 1, "maximum": 99 }
        },
        "required": ["narratives", "headline", "card_score"]
    })
}

pub fn insider_score_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "read":     { "type": "string" },
            "headline": { "type": "string" },
            "score":    { "type": "integer", "minimum": 1, "maximum": 99 }
        },
        "required": ["read", "headline", "score"]
    })
}

pub fn oracle_format_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "reading": {
                "type": "string",
                "description": "Unified prose about the entity and its circumstances, without card, field, computation or score commentary"
            },
            "headline": { "type": "string" },
            "score": { "type": "integer", "minimum": 1, "maximum": 100 }
        },
        "required": ["reading", "headline", "score"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::insider;
    use crate::studio::{analyst, influencer, journalist, oracle, scout};

    #[test]
    fn every_live_character_uses_the_shared_form_without_retired_outlines() {
        let characters = [
            (scout::CHARACTER, scout::RATING_SYSTEM_PROMPT.as_str()),
            (
                analyst::prompt::CHARACTER,
                analyst::prompt::MOMENTUM_SYSTEM_PROMPT.as_str(),
            ),
            (
                influencer::CHARACTER,
                influencer::VIBE_SYSTEM_PROMPT.as_str(),
            ),
            (
                journalist::CHARACTER,
                journalist::NARRATIVES_SYSTEM_PROMPT.as_str(),
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
            for shared in [STORY_FORM, CLAIM_SELECTION, HOOK, WIRE_COPY] {
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
        assert!(narratives_format_schema()["properties"]["narratives"]["maxItems"].is_null());
        assert!(influencer::VIBE_SYSTEM_PROMPT.contains("Return JSON with headline"));
        assert!(journalist::NARRATIVES_SYSTEM_PROMPT
            .contains("title that names this entity as supplied"));
        assert!(oracle::ORACLE_SYSTEM_PROMPT
            .contains("Open the reading with this entity's supplied name"));
        // Character ceilings guide composition and are checked after decoding. A grammar
        // maxLength can force a string closed in the middle of a word or sentence.
        assert!(oracle_format_schema()["properties"]["reading"]["maxLength"].is_null());
    }

    #[test]
    fn surface_counts_characters_without_cutting_prose() {
        assert!(validate_body(&"é".repeat(BODY_MAX_CHARS)).is_ok());
        assert!(validate_body(&"é".repeat(BODY_MAX_CHARS + 1)).is_err());
        assert!(validate_body("  ").is_err());
    }

    #[test]
    fn shared_json_fields_preserve_paragraphs_across_the_card_parsers() {
        use crate::studio::Parser;
        let raw = serde_json::json!({"headline":"Morgan Rogers creates chances at an elite level", "body":"Creation stands out.\n\nThe defensive measures are lower.", "score":60}).to_string();
        assert_eq!(
            scout::RatingParser.parse(&raw).unwrap().unwrap().body,
            analyst::MomentumParser.parse(&raw).unwrap().unwrap().blurb
        );
        let vibe = influencer::VibeParser.parse(&raw).unwrap().unwrap();
        assert_eq!(vibe.sentiment, 60);
        assert!(vibe.vibe_prompt.contains("\n\n"));
        assert!(scout::RatingParser.parse("{\"body\":\"unfinished").is_err());
    }
}

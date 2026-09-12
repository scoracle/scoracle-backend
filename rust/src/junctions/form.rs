//! Shared character form and parser-compatible output contracts.
//!
//! The reader sees a hook header and a body. Transport labels and JSON fields below
//! retain the existing parser/storage contracts; they are not section headings.
//! Character files own voice and judgment. Inputs supply evidence, not an outline.

pub const CLAIM_SELECTION: &str = "Make the claims the evidence reasonably supports. Ordinary, bland, unchanged or middle-of-the-pack can be the finding, just as exceptional performance or genuine ambiguity can. Include each distinct finding that matters; stop when there is nothing more to support.";

pub const STORY_FORM: &str = "The card has a hook header and a body. The body contains one paragraph per claim. Start each paragraph by stating its claim. Follow with the evidence, giving each piece of evidence its own sentence. End by summarizing the claim in light of that evidence. Separate paragraphs with a blank line. The evidence determines the number of claims and supporting sentences; there is no fixed paragraph count. Write the paragraphs as natural prose, without section headings or labels for their parts.";

pub const WIRE_COPY: &str = "Use clear, concise sentences, one idea per sentence. Keep the character's voice in the language and perspective. Write plain prose without Markdown, preambles or commentary about the writing process. Use supplied evidence and preserve uncertainty. Prior readings provide continuity, not new evidence. Use sporting language rather than internal product or system names.";

pub const HOOK: &str = "The hook is one line of at most 140 characters that draws the reader in through a specific claim. Name this entity as supplied, use present tense and let the character's voice carry it. A quiet or ordinary finding can earn the hook. Punctuation is yours.";

#[derive(Clone, Copy, Debug)]
pub enum CardFormat {
    Scout,
    Analyst,
    Influencer,
    Journalist,
    Insider,
    Oracle,
}

pub const ORACLE_READING_MAX_CHARS: usize = 800;

/// Compose the system instruction from the character and shared form.
pub fn compose(character: &str, format: CardFormat) -> String {
    let output = match format {
        CardFormat::Scout => "Return the body as plain paragraphs, followed by a HEADLINE: line containing the hook. The application displays that hook above the body.",
        CardFormat::Analyst => "Return READ: followed by the body, then HEADLINE: followed by the hook. The application displays the hook above the body.",
        CardFormat::Influencer => "Return exactly three labeled fields in this order, with no preamble or text before SCORE:\nSCORE: <integer from 1 to 100>\nHOOK: <hook>\nVIBE: <body>\nThe VIBE field contains only the unlabeled body prose. Preserve blank lines between its paragraphs.",
        CardFormat::Journalist => "Return JSON with narratives, headline and card_score. Each narrative has a short specific title that names this entity as supplied, a body following the shared form, and articles containing its supporting input article numbers. Select relevant stories, most consequential first; an empty narratives array is valid. The headline is the hook for the whole edition. The card_score is an integer from 1 to 99. Preserve paragraph breaks inside body strings as escaped newlines.",
        CardFormat::Insider => "Return JSON with read containing the body, headline containing the hook, and score containing an integer from 1 to 99. Preserve paragraph breaks inside read as escaped newlines.",
        CardFormat::Oracle => "Return JSON with reading containing the body, headline containing the hook, and score containing an integer from 1 to 100. Open the reading with this entity's supplied name and speak directly about its circumstances as one interpretation. The reading is body only: do not describe its hook or headline, the evidence structure, its speakers, computation or JSON fields. Preserve paragraph breaks inside reading as escaped newlines.",
    };
    format!("{character}\n\n{CLAIM_SELECTION}\n\n{STORY_FORM}\n\n{WIRE_COPY}\n\n{HOOK}\n\n{output}")
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
                "maxLength": ORACLE_READING_MAX_CHARS,
                "description": "Unified prose about the entity and its circumstances, without card, field, computation or score commentary"
            },
            "headline": { "type": "string", "maxLength": 140 },
            "score": { "type": "integer", "minimum": 1, "maximum": 100 }
        },
        "required": ["reading", "headline", "score"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::junctions::{analyst, influencer, insider, journalist, oracle, scout};

    #[test]
    fn every_live_character_uses_the_shared_form_without_retired_outlines() {
        let characters = [
            (
                scout::prompt::CHARACTER,
                scout::RATING_SYSTEM_PROMPT.as_str(),
            ),
            (
                analyst::prompt::CHARACTER,
                analyst::MOMENTUM_SYSTEM_PROMPT.as_str(),
            ),
            (
                influencer::prompt::CHARACTER,
                influencer::VIBE_SYSTEM_PROMPT.as_str(),
            ),
            (
                journalist::prompt::CHARACTER,
                journalist::NARRATIVES_SYSTEM_PROMPT.as_str(),
            ),
            (
                insider::prompt::CHARACTER,
                insider::INSIDER_SCORE_SYSTEM_PROMPT.as_str(),
            ),
            (
                oracle::prompt::CHARACTER,
                oracle::ORACLE_SYSTEM_PROMPT.as_str(),
            ),
        ];
        for (brief, system) in characters {
            assert!((100..=200).contains(&brief.split_whitespace().count()));
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
        assert!(influencer::VIBE_SYSTEM_PROMPT.contains(
            "with no preamble or text before SCORE:\nSCORE: <integer from 1 to 100>\nHOOK: <hook>\nVIBE: <body>"
        ));
        assert!(journalist::NARRATIVES_SYSTEM_PROMPT
            .contains("title that names this entity as supplied"));
        assert!(oracle::ORACLE_SYSTEM_PROMPT
            .contains("Open the reading with this entity's supplied name"));
        assert_eq!(
            oracle_format_schema()["properties"]["headline"]["maxLength"],
            140
        );
        assert_eq!(
            oracle_format_schema()["properties"]["reading"]["maxLength"],
            800
        );
    }
}

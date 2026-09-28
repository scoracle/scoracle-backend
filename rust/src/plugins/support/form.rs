//! Shared output structure, dimensions, decoding shapes and structural parsers.
//!
//! The reader sees a hook header and a body. Transport labels and JSON fields below
//! retain the existing parser/storage contracts; they are not section headings.
//! Character files own tone; prompt modules own instructions and content direction.

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
            "Body has {chars} characters; allowed range is 1..={BODY_MAX_CHARS} with nonblank content."
        ))
        .into());
    }
    Ok(())
}

pub fn validate_hook(hook: Option<&str>) -> anyhow::Result<()> {
    if let Some(hook) = hook {
        if hook.trim().is_empty() || hook.chars().count() > HOOK_MAX_CHARS {
            return Err(SurfaceError(format!(
                "Headline/title must contain 1..={HOOK_MAX_CHARS} characters with nonblank content."
            ))
            .into());
        }
    }
    Ok(())
}

/// Fold line wrapping and whitespace while retaining the prose paragraphs.
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

/// Permit an explicit pass while preserving the complete card contract.
pub fn with_abstention(schema: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"anyOf": [schema, {"type": "null"}]})
}

/// Shape only. The surface belongs in the prompt and post-decode validation;
/// constraining a string's maximum length can force a word to end midway.
pub fn card_schema(scored: bool) -> serde_json::Value {
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": {
            "headline": {"type":"string"},
            "body": {"type":"string"}
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

/// Structural writing form: fields, types, counts and dimensions only. Content
/// direction and source-to-output mapping belong to the Journalist's prompt.rs.
pub fn journalist_form(report_count: usize) -> serde_json::Value {
    serde_json::json!({
        "headline":"string",
        "narratives":[{"title":"string", "body":"string"}],
        "narrative_count":report_count,
        "max_chars":{
            "headline":HOOK_MAX_CHARS,
            "title":HOOK_MAX_CHARS,
            "all_bodies":BODY_MAX_CHARS
        }
    })
}

/// Structural output contract, owned alongside the writing form and parser.
pub fn journalist_schema(report_count: usize) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "narratives": {
                "type": "array",
                "minItems": report_count,
                "maxItems": report_count,
                "items": {
                    "type": "object",
                    "properties": {
                        "title":    { "type": "string" },
                        "body":     { "type": "string" },
                    },
                    "required": ["title", "body"], "additionalProperties": false
                }
            },
            "headline": { "type": "string" }
        },
        "required": ["narratives", "headline"], "additionalProperties": false
    })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalistReply {
    pub headline: String,
    pub narratives: Vec<JournalistReport>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalistReport {
    pub title: String,
    pub body: String,
}

pub fn parse_journalist(raw: &str, report_count: usize) -> anyhow::Result<JournalistReply> {
    let reply: JournalistReply = serde_json::from_str(raw)?;
    anyhow::ensure!(
        reply.narratives.len() == report_count,
        "articulation omitted or added reports"
    );
    validate_hook(Some(&reply.headline))?;
    validate_body(
        &reply
            .narratives
            .iter()
            .map(|r| r.body.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
    )?;
    for report in &reply.narratives {
        validate_hook(Some(&report.title))?;
        validate_body(&report.body)?;
    }
    Ok(reply)
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
                "type": "string"
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
    use crate::plugins::analyst::cognition as analyst;
    use crate::plugins::influencer::cognition as influencer;
    use crate::plugins::scout::cognition as scout;

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

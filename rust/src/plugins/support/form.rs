//! Shared output structure, dimensions, decoding shapes and structural parsers.
//!
//! The reader sees a hook header and a body. Transport labels and JSON fields below
//! retain the existing parser/storage contracts; they are not section headings.
//! Character files own tone; prompt modules own instructions and content direction.

/// Reader-facing dimensions, independent of any model's tokenization or runtime budget.
pub const HOOK_MAX_CHARS: usize = 140;
pub const PARAGRAPH_MAX_CHARS: usize = 140;
pub const BODY_MAX_CHARS: usize = 1200;
pub const ORACLE_READING_MAX_CHARS: usize = BODY_MAX_CHARS;

/// Shared observation layout. Plugins supply content scope and factual boundaries
/// in their assembly instructions, independently of this writing tool.
pub fn observation_form() -> ObservationForm {
    ObservationForm {
        body: ObservationBodyForm {
            field_type: "string or null",
            paragraphs: "One observation per paragraph. Blank lines separate paragraphs. Short, complete sentences. No headings or repeated conclusion.",
            paragraph_max_chars: PARAGRAPH_MAX_CHARS,
            max_chars: BODY_MAX_CHARS,
            lengths: "Ceilings, not targets; no minimum length.",
            paragraph_breaks: "escaped newlines",
        },
    }
}

/// Serialize the layout before dimensions. Live SmolLM3 controls found that
/// alphabetically sorting these fields substantially changed output behavior.
#[derive(serde::Serialize)]
pub struct ObservationForm {
    body: ObservationBodyForm,
}

#[derive(serde::Serialize)]
struct ObservationBodyForm {
    #[serde(rename = "type")]
    field_type: &'static str,
    paragraphs: &'static str,
    paragraph_max_chars: usize,
    max_chars: usize,
    lengths: &'static str,
    paragraph_breaks: &'static str,
}

/// Nullable, score-free observation body. Domain-specific parsing stays with the
/// consuming plugin; this schema describes transport shape only.
pub fn observation_schema() -> serde_json::Value {
    serde_json::json!({"type":"object","additionalProperties":false,"required":["body"],
        "properties":{"body":{"type":["null","string"]}}})
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationReply {
    pub body: Option<String>,
}

/// Decode the shared nullable body and enforce its structural contract. Plugins
/// apply their publication guards to the resulting prose before accepting it.
pub fn parse_observation(raw: &str) -> anyhow::Result<Option<ObservationReply>> {
    let frame: serde_json::Value = serde_json::from_str(raw)?;
    anyhow::ensure!(frame.get("body").is_some(), "missing body field");
    let mut reply: ObservationReply = serde_json::from_value(frame)?;
    let Some(body) = reply.body.as_mut() else {
        return Ok(None);
    };
    *body = normalize_body(&crate::util::strip_markdown_emphasis(body));
    validate_observation_body(body)?;
    Ok(Some(reply))
}

pub struct ObservationParser;

impl crate::studio::Parser<ObservationReply> for ObservationParser {
    fn parse(&self, raw: &str) -> anyhow::Result<Option<ObservationReply>> {
        parse_observation(raw)
    }
}

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

/// Observation form for aligned characters. Count normalized prose, including
/// spaces, without treating a wrapped line as a new paragraph or cutting text.
pub fn validate_observation_body(body: &str) -> anyhow::Result<()> {
    validate_body(body)?;
    for (index, paragraph) in normalize_body(body).split("\n\n").enumerate() {
        let chars = paragraph.chars().count();
        if chars > PARAGRAPH_MAX_CHARS {
            return Err(SurfaceError(format!(
                "Paragraph {} has {chars} characters; maximum is {PARAGRAPH_MAX_CHARS}, including spaces.",
                index + 1
            ))
            .into());
        }
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
    let report_keys = (1..=report_count)
        .map(|index| format!("report_{index}"))
        .collect::<Vec<_>>();
    serde_json::json!({
        "report_count":report_count,
        "report_keys":report_keys,
        "report_fields":["text"]
    })
}

/// Structural output contract, owned alongside the writing form and parser.
pub fn journalist_schema(report_count: usize) -> serde_json::Value {
    let narratives = (1..=report_count)
        .map(|index| {
            (
                format!("report_{index}"),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                    },
                    "required": ["text"],
                    "additionalProperties": false
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let required = (1..=report_count)
        .map(|index| serde_json::Value::String(format!("report_{index}")))
        .collect::<Vec<_>>();
    serde_json::json!({
        "type": "object",
        "properties": {
            "reports": {
                "type": "object",
                "properties": narratives,
                "required": required,
                "additionalProperties": false
            }
        },
        "required": ["reports"], "additionalProperties": false
    })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawJournalistReply {
    reports: std::collections::BTreeMap<String, JournalistReport>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalistReport {
    pub text: String,
}

pub struct JournalistReply {
    pub narratives: Vec<JournalistReport>,
}

pub fn parse_journalist(raw: &str, report_count: usize) -> anyhow::Result<JournalistReply> {
    let mut raw_reply: RawJournalistReply = serde_json::from_str(raw)?;
    anyhow::ensure!(
        raw_reply.reports.len() == report_count,
        "articulation omitted or added reports"
    );
    let narratives = (1..=report_count)
        .map(|index| {
            raw_reply
                .reports
                .remove(&format!("report_{index}"))
                .ok_or_else(|| {
                    anyhow::anyhow!("articulation changed report order or source mapping")
                })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(
        raw_reply.reports.is_empty(),
        "articulation changed report order or source mapping"
    );
    let reply = JournalistReply { narratives };
    validate_body(
        &reply
            .narratives
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
    )?;
    for report in &reply.narratives {
        validate_body(&report.text)?;
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
    fn observation_paragraphs_count_unicode_spaces_and_wrapped_lines() {
        let paragraph = "é".repeat(PARAGRAPH_MAX_CHARS);
        assert!(validate_observation_body(&format!("{paragraph}\n\n{paragraph}")).is_ok());
        assert!(validate_observation_body(&format!("{paragraph}é")).is_err());
        // A single newline wraps one paragraph; only a blank line separates it.
        let halves = "é".repeat(70);
        assert!(validate_observation_body(&format!("{halves}\n{halves}")).is_err());
        assert!(validate_observation_body(&format!("{halves} {halves}")).is_err());
        assert!(validate_observation_body("  ").is_err());
        assert!(validate_observation_body(&vec![paragraph; 9].join("\n\n")).is_err());
    }

    #[test]
    fn shared_json_fields_preserve_paragraphs_across_the_card_parsers() {
        use crate::studio::Parser;
        let raw = serde_json::json!({"headline":"Morgan Rogers creates chances at an elite level", "body":"Creation stands out.\n\nThe defensive measures are lower.", "score":60}).to_string();
        assert_eq!(
            scout::RatingParser.parse(&raw).unwrap().unwrap().body,
            analyst::MomentumParser.parse(&raw).unwrap().unwrap().blurb
        );
        let mut score_free: serde_json::Value = serde_json::from_str(&raw).unwrap();
        score_free.as_object_mut().unwrap().remove("score");
        score_free.as_object_mut().unwrap().remove("headline");
        let vibe = influencer::VibeParser
            .parse(&score_free.to_string())
            .unwrap()
            .unwrap();
        assert!(vibe.body.unwrap().contains("\n\n"));
        assert!(scout::RatingParser.parse("{\"body\":\"unfinished").is_err());
    }
}

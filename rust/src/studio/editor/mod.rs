//! The Editor describes prepared articles in Studio. Storage, fetching and work belong to applications.
use crate::studio::model::GenerateOptions;
use crate::studio::{Extracted, Parser, Studio};
use crate::util::truncate;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
pub mod derive;
pub mod prompt;
pub use prompt::{build_editor_prompt_parts, EDITOR_CONTRACT_VERSION, EDITOR_SYSTEM_PROMPT};
const EDITOR_NUM_PREDICT: i32 = 900;
pub(crate) const EDITOR_NUM_CTX: i32 = 4096;

/// All evidence needed for one read; no storage handles or routing decisions.
pub struct Assignment {
    pub source: String,
    pub title: String,
    pub description: String,
    pub text: String,
    pub hypothesis: Vec<String>,
}

impl Studio<'_> {
    pub async fn read_article(&self, assignment: &Assignment) -> Result<Extracted<EditorRead>> {
        let prompt = build_editor_prompt_parts(
            &assignment.source,
            &assignment.title,
            &assignment.description,
            &assignment.text,
            &assignment.hypothesis,
        );
        self.extract(
            &prompt,
            &editor_opts(),
            &EditorReadParser {
                hypothesis: &assignment.hypothesis,
            },
        )
        .await
    }
}

/// The model budget for one editor read — one definition for the stage and `bin/eval`, so a
/// fixture can never be scored under options production does not send. Temperature 0.2 live;
/// the eval overrides it per case.
///
/// Both schema forms travel together: `format_schema_raw` is what Ollama receives (order-true),
/// `format_schema` is the `Value` the ledger/eval capture path stores.
pub fn editor_opts() -> GenerateOptions {
    GenerateOptions {
        system: Some(EDITOR_SYSTEM_PROMPT.to_string()),
        temperature: Some(0.2),
        num_predict: EDITOR_NUM_PREDICT,
        num_ctx: EDITOR_NUM_CTX,
        json_mode: false,
        format_schema: Some(editor_format_schema()),
        format_schema_raw: Some(EDITOR_FORMAT_SCHEMA_RAW.to_string()),
    }
}

/// Field order is part of the contract. This raw string preserves it on the Ollama wire;
/// a `serde_json::Value` would alphabetize object keys.
///
/// `relevant` is absent because code derives it from the description.
///
/// Prefer schema constraints for shape and prompt prose for meaning.
pub const EDITOR_FORMAT_SCHEMA_RAW: &str = r#"{
    "type": "object",
    "properties": {
        "source_language": { "type": "string", "enum": [
            "af", "ar", "az", "bg", "bn", "bs", "ca", "cs", "cy", "da", "de", "el", "en", "es",
            "et", "eu", "fa", "fi", "fr", "ga", "gl", "he", "hi", "hr", "hu", "hy", "id", "is",
            "it", "ja", "ka", "kk", "ko", "lt", "lv", "mk", "ml", "ms", "mt", "ne", "nl", "no",
            "pl", "pt", "ro", "ru", "sk", "sl", "sq", "sr", "sv", "sw", "ta", "th", "tk", "tl",
            "tr", "ug", "uk", "ur", "uz", "vi", "zh", "unknown"
        ] },
        "page_kind": { "type": "string", "enum": [
            "article", "score_table", "listing_or_schedule", "video_clip", "roundup", "other"
        ] },
        "names": {
            "type": "array",
            "maxItems": 24,
            "items": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "maxLength": 60 },
                    "kind_hint": { "type": "string", "enum": [
                        "person", "club", "national_team", "other"
                    ] },
                    "descriptor": { "type": "string", "maxLength": 48 }
                },
                "required": ["name", "kind_hint", "descriptor"]
            }
        },
        "entity_roles": {
            "type": "array",
            "maxItems": 12,
            "items": {
                "type": "object",
                "properties": {
                    "entity": { "type": "string" },
                    "role": { "type": "string", "enum": [
                        "subject", "opponent", "passing_mention", "absent"
                    ] }
                },
                "required": ["entity", "role"]
            }
        },
        "story_type": { "type": "string", "enum": [
            "transfer", "injury", "suspension", "performance", "fixture", "roster", "contract",
            "general"
        ] },
        "result_line": { "type": "string" },
        "register_phrase": { "type": "string" },
        "register": { "type": "string", "enum": [
            "celebration", "outrage", "resignation", "anticipation", "neutral"
        ] },
        "key_facts": { "type": "array", "items": { "type": "string" }, "maxItems": 8 },
        "caveats": { "type": "string" },
        "evidence_blurb": { "type": "string" }
    },
    "required": ["source_language", "page_kind", "names", "entity_roles", "story_type",
                 "result_line", "register_phrase", "register", "key_facts", "caveats",
                 "evidence_blurb"]
}"#;

/// The ep1 schema as a `Value` — the ledger/eval capture form. Parsed from the raw literal so
/// the two can never disagree on CONTENT; only the raw form carries the order.
pub fn editor_format_schema() -> serde_json::Value {
    serde_json::from_str(EDITOR_FORMAT_SCHEMA_RAW)
        .expect("EDITOR_FORMAT_SCHEMA_RAW is valid JSON (unit-tested)")
}

/// One `names[]` entry — ep1's discovery channel.
#[derive(Clone, Debug, Deserialize)]
pub struct NameMention {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind_hint: String,
    #[serde(default)]
    pub descriptor: String,
}

/// One hypothesis entity's part in the article text.
#[derive(Clone, Debug, Deserialize)]
pub struct EditorEntityRole {
    #[serde(default)]
    pub entity: String,
    #[serde(default)]
    pub role: String,
}

/// The parsed ep1 envelope. `relevant` is DERIVED, never deserialized — the model is not asked
/// (`#[serde(skip)]`, the ar6 discipline carried forward).
#[derive(Clone, Debug, Deserialize)]
pub struct EditorRead {
    #[serde(skip)]
    pub relevant: bool,
    #[serde(default)]
    pub source_language: String,
    #[serde(default)]
    pub page_kind: String,
    #[serde(default)]
    pub names: Vec<NameMention>,
    #[serde(default)]
    pub entity_roles: Vec<EditorEntityRole>,
    #[serde(default)]
    pub story_type: String,
    #[serde(default)]
    pub result_line: String,
    #[serde(default)]
    pub register_phrase: String,
    #[serde(default)]
    pub register: String,
    #[serde(default)]
    pub key_facts: Vec<String>,
    #[serde(default)]
    pub caveats: String,
    pub evidence_blurb: String,
}

impl EditorRead {
    /// Model fields in schema order, persisted as `editor_reads.read`.
    pub fn envelope(&self) -> serde_json::Value {
        json!({
            "source_language": self.source_language,
            "page_kind": self.page_kind,
            "names": self.names.iter().map(|n| json!({
                "name": n.name,
                "kind_hint": n.kind_hint,
                "descriptor": n.descriptor,
            })).collect::<Vec<_>>(),
            "entity_roles": self.entity_roles.iter().map(|r| json!({
                "entity": r.entity,
                "role": r.role,
            })).collect::<Vec<_>>(),
            "story_type": self.story_type,
            "result_line": self.result_line,
            "register_phrase": self.register_phrase,
            "register": self.register,
            "key_facts": self.key_facts,
            "caveats": self.caveats,
            "evidence_blurb": self.evidence_blurb,
        })
    }
}

/// The closed emotional-register vocabulary.
pub const EDITOR_REGISTERS: &[&str] = &[
    "celebration",
    "outrage",
    "resignation",
    "anticipation",
    "neutral",
];

/// Carries the hypothesis entity names (decorated `Name (team 42)`), because relevance cannot be
/// derived without them — only OUR entities get a vote.
pub struct EditorReadParser<'a> {
    pub hypothesis: &'a [String],
}

impl Parser<EditorRead> for EditorReadParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<EditorRead>> {
        let Some(slice) = json_object_slice(raw) else {
            return Ok(None);
        };
        let mut read: EditorRead = serde_json::from_str(slice)
            .with_context(|| format!("parse editor read (raw={:?})", truncate(raw, 200)))?;
        read.evidence_blurb = normalize_space(&read.evidence_blurb);
        read.source_language = normalize_language_code(&read.source_language);
        read.page_kind = normalize_space(&read.page_kind);
        read.story_type = normalize_space(&read.story_type);
        read.caveats = normalize_space(&read.caveats);
        read.result_line = normalize_space(&read.result_line);
        read.key_facts = read
            .key_facts
            .into_iter()
            .map(|s| normalize_space(&s))
            .filter(|s| !s.is_empty())
            .take(8)
            .collect();
        read.names = read
            .names
            .into_iter()
            .map(|mut n| {
                n.name = normalize_space(&n.name);
                n.kind_hint = normalize_space(&n.kind_hint).to_lowercase();
                // The contract says ≤6 words FROM the text; enforce here so the derivation and
                // the archive both see the contract's descriptor, not the model's essay.
                n.descriptor = normalize_space(&n.descriptor)
                    .split_whitespace()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" ");
                if !matches!(
                    n.kind_hint.as_str(),
                    "person" | "club" | "national_team" | "other"
                ) {
                    n.kind_hint = "other".to_string();
                }
                n
            })
            .filter(|n| !n.name.is_empty())
            .take(24)
            .collect();
        read.entity_roles.retain(|r| !r.entity.trim().is_empty());
        // Register is DERIVED-normalized, not trusted (the C2 discipline): the prompt states
        // "an empty register_phrase means neutral", and enforcing it here makes the prompt and
        // the persisted value provably agree. Out-of-enum also falls back to neutral.
        read.register_phrase = normalize_space(&read.register_phrase);
        read.register = normalize_space(&read.register).to_lowercase();
        if read.register_phrase.is_empty() || !EDITOR_REGISTERS.contains(&read.register.as_str()) {
            read.register = "neutral".to_string();
        }
        // The verdict is COMPUTED here, not read (`relevant` is `#[serde(skip)]`).
        read.relevant = derive::derive_relevance(
            &read.page_kind,
            &read.entity_roles,
            self.hypothesis,
            &read.names,
        );
        if read.evidence_blurb.is_empty() {
            if !read.relevant {
                read.evidence_blurb =
                    "Full text is not materially about the hypothesis entities.".to_string();
                return Ok(Some(read));
            }
            return Ok(None);
        }
        Ok(Some(read))
    }
}

fn normalize_language_code(raw: &str) -> String {
    let s = raw.trim().to_lowercase();
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_lowercase()) {
        return s;
    }
    "unknown".to_string()
}

fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn json_object_slice(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if *b == b'\\' {
                esc = true;
            } else if *b == b'"' {
                in_str = false;
            }
            continue;
        }
        match *b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return start.map(|s| &raw[s..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests;

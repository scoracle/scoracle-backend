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

/// The Journalist's paragraph ceiling, decided by replay rather than assumed.
///
/// Influencer's 140 is a writing policy the Journalist does not share: the
/// Journalist has always validated only the body total, and its reports are
/// keyed 1:1 with source reports rather than written as short observations. The
/// value here is what the n94 replay measured; see the alignment plan's F4b.
/// `None` records a documented non-participation rather than a number neither
/// plugin was measured against.
pub const JOURNALIST_PARAGRAPH_MAX_CHARS: Option<usize> = None;

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

/// The Influencer's declared prose contract: one nullable `body` slot.
///
/// A null body is this plugin's abstention, not a dropped slot, and the shared
/// validator keeps the two distinct.
pub fn observation_prose() -> crate::plugins::cognition::prose::Prose {
    crate::plugins::cognition::prose::Prose::new(
        &["body"],
        crate::plugins::cognition::prose::Dimensions::new(
            BODY_MAX_CHARS,
            Some(PARAGRAPH_MAX_CHARS),
        ),
    )
}

/// Nullable, score-free observation body. Domain-specific parsing stays with the
/// consuming plugin; this schema describes transport shape only.
pub fn observation_schema() -> serde_json::Value {
    observation_prose().schema()
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationReply {
    pub body: Option<String>,
}

/// Decode the shared nullable body and enforce its structural contract. Plugins
/// apply their publication guards to the resulting prose before accepting it.
///
/// A thin wrapper over the shared validator: this plugin's slot is `body`, its
/// dimensions are recorded above, and the emphasis/line-wrap cleanup below is
/// the one presentation step that remains its own.
pub fn parse_observation(raw: &str) -> anyhow::Result<Option<ObservationReply>> {
    let prose = observation_prose();
    let mut map = decode_prose_map(raw, &prose.keys)?;
    // Prepare before measuring: emphasis markers are presentation, not prose,
    // so they must not be charged against this plugin's ceilings.
    if let Some(body) = map
        .get("body")
        .map(|text| normalize_body(&crate::util::strip_markdown_emphasis(text)))
    {
        map.replace("body", Some(body));
    }
    // A declined slot is this plugin's abstention, distinct from a violation.
    let Some(body) = map.get("body") else {
        return Ok(None);
    };
    map.validate(prose.dims)?;
    Ok(Some(ObservationReply {
        body: Some(body.to_string()),
    }))
}

pub struct ObservationParser;

impl crate::studio::Parser<ObservationReply> for ObservationParser {
    fn parse(&self, raw: &str) -> anyhow::Result<Option<ObservationReply>> {
        parse_observation(raw)
    }
}

/// One plugin's prose response, decoded against the keys that plugin declared.
///
/// This is the shared surface every character product returns: a map of
/// plugin-named slots to prose. The *shape* is shared; the keys are the
/// plugin's. `get` and `push` exist so a caller can read a slot or fill one
/// without re-parsing the JSON a second time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProseMap {
    /// Plugin-declared key order, with each key's prose. A key is present with
    /// a `None` value when the model declined to fill it; that is distinct from
    /// the key being absent, which is a contract violation.
    slots: Vec<(String, Option<String>)>,
}

/// The map itself, in the plugin's declared order. A derived `Serialize` on the
/// struct would emit `{"slots": ...}` and lose the keyed surface entirely.
impl serde::Serialize for ProseMap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.slots.len()))?;
        for (key, prose) in &self.slots {
            map.serialize_entry(key, prose)?;
        }
        map.end()
    }
}

impl ProseMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a slot in the order the plugin declared its keys.
    pub fn push(&mut self, key: impl Into<String>, prose: Option<String>) {
        self.slots.push((key.into(), prose));
    }

    /// Replace a declared slot's prose, keeping its position. Used when a plugin
    /// prepares its own text between decoding and measuring; appending a second
    /// entry for the same key would silently double-count it.
    pub fn replace(&mut self, key: &str, prose: Option<String>) {
        match self.slots.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = prose,
            None => self.push(key, prose),
        }
    }

    /// The prose for one declared key. A declined slot and a missing key both
    /// read as `None` here; `enforce` is what rejects the missing key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.slots
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, prose)| prose.as_deref())
    }

    /// The keys in the order the plugin declared them.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.slots.iter().map(|(key, _)| key.as_str())
    }

    /// Every filled slot, in declared order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.slots
            .iter()
            .filter_map(|(key, prose)| prose.as_deref().map(|text| (key.as_str(), text)))
    }
}

/// Decode `raw` into exactly `keys`, structurally.
///
/// Separate from [`ProseMap::validate`] so a plugin can prepare its own prose
/// first — the Influencer strips markdown emphasis before anything is measured,
/// and measuring first would change what its ceilings mean.
pub fn decode_prose_map(raw: &str, keys: &[String]) -> anyhow::Result<ProseMap> {
    let frame: serde_json::Value = serde_json::from_str(raw)?;
    let object = frame
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("articulation reply must be a keyed object"))?;
    // `additionalProperties: false` over exactly the requested keys: an
    // undeclared key is a model inventing a field the plugin would ignore, and
    // a missing declared key is a dropped slot.
    for name in object.keys() {
        anyhow::ensure!(
            keys.iter().any(|k| k == name),
            "articulation returned undeclared field `{name}`"
        );
    }
    let mut map = ProseMap::new();
    for key in keys {
        let value = object
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("articulation omitted `{key}`"))?;
        anyhow::ensure!(
            value.is_string() || value.is_null(),
            "articulation field `{key}` must be prose or null"
        );
        map.push(key.clone(), value.as_str().map(normalize_body));
    }
    Ok(map)
}

impl ProseMap {
    /// Enforce the paragraph and body rules. This is the one implementation of
    /// them, over whatever the plugin decided to measure.
    ///
    /// A shared mechanism, not a shared policy: `dims` is supplied per plugin, so
    /// the reader-facing body ceiling can be common while the readability
    /// ceiling is each plugin's own recorded decision.
    pub fn validate(
        &self,
        dims: crate::plugins::cognition::prose::Dimensions,
    ) -> anyhow::Result<()> {
        let mut total = 0usize;
        for (key, prose) in self.iter() {
            anyhow::ensure!(
                !prose.trim().is_empty(),
                "`{key}` is blank; a slot carries prose or is declined"
            );
            if let Some(limit) = dims.paragraph_max_chars {
                // Line wrapping is not a paragraph break and is folded here, so
                // a wrapped long line is measured as the paragraph it is.
                for (index, paragraph) in normalize_body(prose).split("\n\n").enumerate() {
                    let chars = paragraph.chars().count();
                    anyhow::ensure!(
                        chars <= limit,
                        "Paragraph {} of `{key}` has {chars} characters; \
                         maximum is {limit}, including spaces.",
                        index + 1
                    );
                }
            }
            total += prose.chars().count();
        }
        anyhow::ensure!(
            total <= dims.total_max_chars,
            "Prose totals {total} characters across {} slots; allowed range is 1..={}.",
            self.slots.len(),
            dims.total_max_chars
        );
        Ok(())
    }
}

/// Decode and validate in one step, for a plugin with nothing to prepare.
pub fn parse_prose_map(
    raw: &str,
    keys: &[String],
    dims: crate::plugins::cognition::prose::Dimensions,
) -> anyhow::Result<ProseMap> {
    let map = decode_prose_map(raw, keys)?;
    map.validate(dims)?;
    Ok(map)
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

/// The Journalist's declared prose contract: one slot per selected report.
///
/// The keys are the plugin's own `report_key` values, in source order. History is
/// attached per report inside the package rather than presented as a parallel
/// array, so no pairing decision reaches the model.
pub fn journalist_prose(report_count: usize) -> crate::plugins::cognition::prose::Prose {
    let keys = (1..=report_count)
        .map(|index| format!("report_{index}"))
        .collect::<Vec<_>>();
    crate::plugins::cognition::prose::Prose::new_owned(
        keys,
        crate::plugins::cognition::prose::Dimensions::new(
            BODY_MAX_CHARS,
            JOURNALIST_PARAGRAPH_MAX_CHARS,
        ),
    )
}

/// Structural writing form: fields, types, counts and dimensions only. Content
/// direction and source-to-output mapping belong to the Journalist's prompt.rs.
///
/// `history` states where history lives. It describes placement only; the
/// manual explains the package.
pub fn journalist_form(report_count: usize) -> serde_json::Value {
    let mut form = serde_json::json!({
        "report_count":report_count,
        "report_keys":(1..=report_count).map(|index| format!("report_{index}")).collect::<Vec<_>>(),
        "report_fields":["text"],
        "history":"attached per report"
    });
    // Serde emits the map in declaration order; keep the shared dimensions
    // alongside the plugin's own keys rather than hiding them in a constant.
    if let Some(object) = form.as_object_mut() {
        object.insert("max_chars".into(), serde_json::json!(BODY_MAX_CHARS));
    }
    form
}

/// Structural output contract, owned alongside the writing form and parser.
///
/// Generated from the same declaration the validator enforces, so the grammar
/// offered to the model and the surface actually accepted cannot drift.
pub fn journalist_schema(report_count: usize) -> serde_json::Value {
    journalist_prose(report_count).schema()
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct JournalistReport {
    pub text: String,
}

pub struct JournalistReply {
    pub narratives: Vec<JournalistReport>,
}

/// Decode the keyed reports and enforce this plugin's dimensions.
///
/// The keyed surface is the shared one, so decoding delegates. The report-order
/// and source-mapping binding stays here because it is this plugin's evidence
/// contract rather than shared mechanics: `report_N` is a request-local slot
/// bound to a selected source report, and the index is what ties the returned
/// prose back to publication.
pub fn parse_journalist(raw: &str, report_count: usize) -> anyhow::Result<JournalistReply> {
    let prose = journalist_prose(report_count);
    let map = parse_prose_map(raw, &prose.keys, prose.dims)?;
    Ok(JournalistReply {
        narratives: map
            .iter()
            .map(|(_, text)| JournalistReport {
                text: text.to_string(),
            })
            .collect(),
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

    /// The assembled/articulate test, checkable on the rendered package. A
    /// reader who cannot tell which history belongs to which claim without the
    /// model has not been handed an assembled world.
    #[test]
    fn the_journalist_declares_history_as_attached_per_report() {
        let form = journalist_form(1);
        assert_eq!(form["history"], serde_json::json!("attached per report"));
        assert_eq!(form["report_keys"], serde_json::json!(["report_1"]));
        assert_eq!(form["max_chars"], serde_json::json!(BODY_MAX_CHARS));
    }

    #[test]
    fn the_two_character_plugins_share_a_validator_and_not_a_policy() {
        // One implementation, two declarations. The Influencer opts into the
        // 140-character readability rule; the Journalist records that it does
        // not, and both are held to the shared body ceiling.
        let influencer = observation_prose();
        let journalist = journalist_prose(1);
        assert_eq!(
            influencer.dims.total_max_chars,
            journalist.dims.total_max_chars
        );
        assert_eq!(influencer.dims.paragraph_max_chars, Some(140));
        assert_eq!(
            journalist.dims.paragraph_max_chars,
            JOURNALIST_PARAGRAPH_MAX_CHARS
        );
        // The grammar offered to the model comes from the enforced declaration.
        assert_eq!(journalist_schema(2), journalist_prose(2).schema());
    }

    #[test]
    fn a_declined_slot_and_a_missing_slot_stay_distinct_through_the_wrapper() {
        // A null body is this plugin's abstention and yields no reading; `{}` is
        // a dropped slot and is a contract violation. They must not collapse.
        assert!(parse_observation(r#"{"body":null}"#).unwrap().is_none());
        assert!(parse_observation("{}").is_err());
        assert!(parse_observation(r#"{"body":""}"#).is_err());
        assert!(parse_observation(r#"{"body":null,"score":3}"#).is_err());
    }

    #[test]
    fn emphasis_is_stripped_before_the_ceilings_are_measured() {
        // A bold run is presentation, not prose. If it were measured first, a
        // body that fits would be rejected for its markers.
        let text = "Cedar won.";
        let bold = format!("**{text}**");
        assert!(
            parse_observation(&serde_json::json!({"body":bold}).to_string())
                .unwrap()
                .unwrap()
                .body
                .as_deref()
                == Some(text)
        );
    }

    #[test]
    fn the_journalist_refuses_a_changed_report_count_order_or_mapping() {
        let one = |raw: &str| parse_journalist(raw, 1);
        assert!(one(r#"{"report_1":"Cedar won."}"#).is_ok());
        // Omitted, added, and remapped slots are all contract violations.
        assert!(one(r#"{"report_2":"Cedar won."}"#).is_err());
        assert!(one(r#"{"report_1":"a","report_2":"b"}"#).is_err());
        assert!(one(r#"{"report_1":"a","text":"b"}"#).is_err());
        assert!(one(r#"{"report_1":""}"#).is_err());
    }
}

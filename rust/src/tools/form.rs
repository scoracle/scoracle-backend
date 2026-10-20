//! Shared output structure, decoding shapes and structural parsers.
//!
//! The reader sees a hook header and a body. Transport labels and JSON fields below
//! retain the existing parser/storage contracts; they are not section headings.
//! Voice modules own tone; prompt modules own instructions and content direction.

use serde_json::Value;

/// The Prose slot: a prepared world in, a keyed prose map out.
#[derive(Clone, Debug)]
pub struct Prose {
    /// The plugin's output keys, such as `["body"]` for Scout.
    pub keys: Vec<String>,
}

impl Prose {
    pub fn new(keys: &[&str]) -> Self {
        Self::new_owned(keys.iter().map(|k| (*k).to_string()).collect())
    }

    /// For a plugin whose keys are computed rather than written out.
    pub fn new_owned(keys: Vec<String>) -> Self {
        Self { keys }
    }

    /// The `form` block a world presents, describing structure only.
    pub fn form(&self) -> Value {
        serde_json::json!({ "keys": self.keys, "paragraphs": "concise" })
    }

    /// The permissive JSON shape a response must satisfy. Deliberately
    /// `additionalProperties: false` over exactly the declared keys so the model
    /// cannot invent a field the plugin would have to ignore.
    pub fn schema(&self) -> Value {
        let mut properties = serde_json::Map::new();
        for key in &self.keys {
            properties.insert(key.clone(), serde_json::json!({"type": ["null", "string"]}));
        }
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": self.keys,
        })
    }
}

/// Reader-facing dimensions, independent of any model's tokenization or runtime budget.
pub const HOOK_MAX_CHARS: usize = 140;
pub const JOURNALIST_REPORT_MAX_CHARS: usize = 1200;

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
/// Separate from [`ProseMap::validate`] so a plugin can apply its own surface
/// preparation and field-specific acceptance rules.
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
    /// Validate nonblank prose; preserve the model’s paragraphs.
    pub fn validate(&self) -> anyhow::Result<()> {
        for (key, prose) in self.iter() {
            anyhow::ensure!(
                !prose.trim().is_empty(),
                "`{key}` is blank; a slot carries prose or is declined"
            );
        }
        Ok(())
    }
}

/// Decode and validate in one step, for a plugin with nothing to prepare.
pub fn parse_prose_map(raw: &str, keys: &[String]) -> anyhow::Result<ProseMap> {
    let map = decode_prose_map(raw, keys)?;
    map.validate()?;
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
    if body.trim().is_empty() {
        return Err(SurfaceError("Body must contain nonblank prose.".into()).into());
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

/// Up to three narratives, each bound to the fresh headlines that support it.
pub fn journalist_schema(report_count: usize) -> serde_json::Value {
    let keys: Vec<_> = (1..=report_count).map(|i| format!("report_{i}")).collect();
    serde_json::json!({"type":"object","additionalProperties":false,"required":["narratives"],
        "properties":{"narratives":{"type":"array","maxItems":report_count.min(3),"items":{
            "type":"object","additionalProperties":false,"required":["headline","body","report_keys"],
            "properties":{"headline":{"type":"string","minLength":1,"maxLength":HOOK_MAX_CHARS},
                "body":{"type":"string","minLength":1,"maxLength":JOURNALIST_REPORT_MAX_CHARS},
                "report_keys":{"type":"array","minItems":1,"maxItems":report_count,"uniqueItems":true,
                    "items":{"type":"string","enum":keys}}}}}}})
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalistReport {
    pub headline: String,
    pub body: String,
    pub report_keys: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalistReply {
    pub narratives: Vec<JournalistReport>,
}

pub fn parse_journalist(raw: &str, report_count: usize) -> anyhow::Result<JournalistReply> {
    let mut reply: JournalistReply = serde_json::from_str(raw)?;
    anyhow::ensure!(
        reply.narratives.len() <= report_count.min(3),
        "too many narratives"
    );
    for narrative in &mut reply.narratives {
        narrative.headline = normalize_body(&narrative.headline);
        narrative.body = normalize_body(&narrative.body);
        narrative.headline = crate::tools::guards::clean_served_prose(&narrative.headline);
        narrative.body = crate::tools::guards::clean_served_prose(&narrative.body);
        for prose in [&narrative.headline, &narrative.body] {
            anyhow::ensure!(
                crate::tools::guards::first_product_name(prose).is_none()
                    && !crate::tools::guards::has_bookkeeping_citation(prose)
                    && !crate::tools::guards::has_foreign_script(prose),
                "Journalist violates served prose guard"
            );
        }
        validate_hook(Some(&narrative.headline))?;
        validate_body(&narrative.body)?;
        anyhow::ensure!(
            narrative.body.chars().count() <= JOURNALIST_REPORT_MAX_CHARS,
            "Journalist narrative exceeds the reading budget"
        );
        let mut seen = std::collections::HashSet::new();
        anyhow::ensure!(
            !narrative.report_keys.is_empty(),
            "narrative has no fresh evidence"
        );
        for key in &narrative.report_keys {
            let valid = (1..=report_count).any(|i| key == &format!("report_{i}"));
            anyhow::ensure!(
                valid && seen.insert(key),
                "invalid or duplicate narrative report key"
            );
        }
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Parser;
    use crate::plugins::analyst::parser as analyst;
    use crate::plugins::influencer;
    use crate::plugins::scout::parser as scout;

    #[test]
    fn body_validation_preserves_prose_and_refuses_blank_content() {
        assert!(validate_body(&"é".repeat(2400)).is_ok());
        assert!(validate_body("  ").is_err());
    }

    #[test]
    fn observations_preserve_long_paragraphs_and_header_limit() {
        let paragraph = "é".repeat(300);
        let reply = influencer::VibeParser.parse(
            &serde_json::json!({"score":75,"headline":"Hope","body":format!("{paragraph}\n\n{paragraph}")}).to_string(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(reply.body, format!("{paragraph}\n\n{paragraph}"));
        assert!(validate_hook(Some(&"é".repeat(HOOK_MAX_CHARS))).is_ok());
        assert!(validate_hook(Some(&"é".repeat(HOOK_MAX_CHARS + 1))).is_err());
    }

    /// The Scout moved to the shared keyed map in Window 4, so it no longer
    /// shares Analyst's card shape. What this axis now holds is that every
    /// character parser preserves paragraph breaks and refuses a truncated
    /// reply, and that a declared contract rejects a field the plugin never
    /// asked for. The last part is new: the Scout used to accept a `score` here
    /// and score-free it by hand, which is the thing the shared decoder exists
    /// to stop.
    #[test]
    fn every_character_parser_preserves_paragraphs_and_refuses_what_it_did_not_declare() {
        use crate::harness::Parser;
        let paragraphs = "Creation stands out.\n\nThe defensive measures are lower.";
        let card = serde_json::json!({
            "headline": "Morgan Rogers creates chances at an elite level",
            "body": paragraphs,
            "score": 60
        })
        .to_string();
        // Analyst now declares one blurb, so an old scored card is rejected.
        assert!(analyst::MomentumParser.parse(&card).is_err());
        let analyst_reply = serde_json::json!({"blurb": paragraphs}).to_string();
        assert_eq!(
            analyst::MomentumParser
                .parse(&analyst_reply)
                .unwrap()
                .unwrap()
                .blurb,
            paragraphs
        );
        let keyed = serde_json::json!({"body": paragraphs}).to_string();
        assert_eq!(
            scout::RatingParser.parse(&keyed).unwrap().unwrap().body,
            paragraphs
        );
        assert!(influencer::VibeParser
            .parse(&card)
            .unwrap()
            .unwrap()
            .body
            .contains("\n\n"));
        // A score the Scout never declared is a violation, not something to strip.
        assert!(scout::RatingParser.parse(&card).is_err());
        assert!(influencer::VibeParser
            .parse(&serde_json::json!({"headline":"Hope","body":paragraphs}).to_string())
            .is_err());
        // Truncation is an error everywhere, never a partial card.
        for truncated in [
            "{\"body\":\"unfinished",
            "{\"headline\":\"A read\",\"body\":\"unfinished",
        ] {
            let refused = analyst::MomentumParser.parse(truncated).is_err()
                || scout::RatingParser.parse(truncated).is_err()
                || influencer::VibeParser.parse(truncated).is_err();
            assert!(refused, "a truncated reply was accepted: {truncated}");
        }
    }

    #[test]
    fn a_declined_slot_and_a_missing_slot_stay_distinct() {
        let keys = Prose::new(&["body"]).keys;
        let decode = |raw: &str| decode_prose_map(raw, &keys);
        assert!(decode(r#"{"body":null}"#).unwrap().get("body").is_none());
        assert!(decode("{}").is_err());
        assert!(decode(r#"{"body":""}"#).unwrap().validate().is_err());
        assert!(decode(r#"{"body":null,"score":3}"#).is_err());
    }

    #[test]
    fn emphasis_cleanup_preserves_the_reading() {
        // Markdown emphasis is presentation, not part of the reading.
        let text = "Cedar won.";
        let bold = format!("**{text}**");
        assert!(
            influencer::VibeParser
                .parse(&serde_json::json!({"score":75,"headline":"Hope","body":bold}).to_string())
                .unwrap()
                .unwrap()
                .body
                == text
        );
    }

    #[test]
    fn journalist_rejects_product_names_and_internal_score_citations() {
        for bad in [
            "The Insider confirms it.",
            "The report (sentiment=30) is positive.",
        ] {
            let raw=serde_json::json!({"narratives":[{"headline":"Cedar update","body":bad,"report_keys":["report_1"]}]}).to_string();
            assert!(parse_journalist(&raw, 1).is_err());
        }
    }

    #[test]
    fn narratives_require_fresh_unique_source_keys_and_bounded_prose() {
        let good = serde_json::json!({"narratives":[{"headline":"Talks remain uncertain","body":"Wire reports talks, followed by a denial.","report_keys":["report_1","report_2"]}]});
        assert!(parse_journalist(&good.to_string(), 2).is_ok());
        for keys in [
            serde_json::json!([]),
            serde_json::json!(["report_3"]),
            serde_json::json!(["report_1", "report_1"]),
        ] {
            let mut changed = good.clone();
            changed["narratives"][0]["report_keys"] = keys;
            assert!(parse_journalist(&changed.to_string(), 2).is_err());
        }
        let mut changed = good;
        changed["narratives"][0]["body"] =
            serde_json::json!("x".repeat(JOURNALIST_REPORT_MAX_CHARS + 1));
        assert!(parse_journalist(&changed.to_string(), 2).is_err());
        assert!(parse_journalist(r#"{"narratives":[]}"#, 2).is_ok());
        assert!(parse_journalist(r#"{"report_1":"Old per-headline shape"}"#, 2).is_err());
    }
}

#[cfg(test)]
mod prose_tests {
    use super::*;

    #[test]
    fn output_keys_belong_to_the_plugins() {
        let influencer = Prose::new(&["body"]);
        let journalist = Prose::new(&["report_1"]);
        assert_eq!(influencer.schema()["required"], serde_json::json!(["body"]));
        assert_eq!(
            journalist.schema()["required"],
            serde_json::json!(["report_1"])
        );
    }

    #[test]
    fn a_response_may_not_carry_a_field_the_plugin_did_not_declare() {
        let prose = Prose::new(&["headline", "body"]);
        let schema = prose.schema();
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        let keys = schema["properties"].as_object().unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains_key("headline") && keys.contains_key("body"));
    }
}

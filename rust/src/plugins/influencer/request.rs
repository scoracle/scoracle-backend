//! One entity/daily packet; production and replay share this entry point.
use super::character_instructions::INSTRUCTIONS;
use crate::harness::model::GenerateOptions;
use crate::harness::route::RouteKey;
use crate::tools::meta::EntityMeta;
use crate::tools::source::SourceContext;
use crate::util::hash_components;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MODEL: RouteKey = RouteKey::new("vibe-logic", "VIBE_LOGIC");
pub const VIBE_PROMPT_VERSION: &str = "vibe-frame-v48-paragraphs";
pub const SCORE_VERSION: &str = "emotional-valence-v1";
pub const VIBE_TEMPERATURE: f64 = 0.0;
pub const VIBE_NUM_PREDICT: i32 = 3500;
pub const BODY_MAX_CHARS: usize = 12000;
// Legacy byte-size reference for tests; the provider admits complete input in tokens.
pub const SOURCE_BUDGET_BYTES: usize = 24000;

pub const SCHEMA_RAW: &str = r#"{"oneOf":[{"type":"object","additionalProperties":false,"required":["body","score","headline"],"properties":{"body":{"type":"string","minLength":1,"maxLength":12000},"score":{"type":"integer","minimum":0,"maximum":100},"headline":{"type":"string","minLength":1,"maxLength":140}}},{"type":"object","additionalProperties":false,"required":["body","score","headline"],"properties":{"body":{"type":"null"},"score":{"type":"null"},"headline":{"type":"null"}}}]}"#;

pub fn schema() -> Value {
    serde_json::from_str(SCHEMA_RAW).expect("static Influencer schema")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Day {
    pub start: i64,
    pub end: i64,
    pub cutoff: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    #[serde(default)]
    pub database_record: Option<crate::tools::meta::DatabaseRecord>,
    pub day: Day,
    pub sources: Vec<SourceContext>,
    #[serde(default)]
    pub memories: Vec<crate::plugins::classifier::memories::Headline>,
    #[serde(default)]
    pub excluded: Vec<Value>,
}

impl Parts {
    pub fn hold_reason(&self) -> Option<&str> {
        self.excluded
            .iter()
            .find(|entry| entry["in_day"] == true && entry["reason"] != "known_copy")
            .map(|entry| {
                entry["reason"]
                    .as_str()
                    .unwrap_or("incomplete_reporting_day")
            })
    }

    pub fn assemble(&self) -> String {
        let mut selected = self.clone();
        selected.sources = crate::tools::source::top_articles(&self.sources, "emotion", |s| {
            s.classifier_world.as_ref()
        });
        crate::tools::prompt::assemble(
            &INSTRUCTIONS,
            crate::tools::meta::context(self.subject.for_writing(), self.database_record.as_ref()),
            super::fresh::context(&selected),
        )
    }
}

#[derive(Clone, Debug)]
pub struct Assignment {
    pub parts: Parts,
    pub input_hash: String,
}

impl Assignment {
    pub fn from_parts(mut parts: Parts) -> Result<Self> {
        ensure!(
            (23 * 3600..=25 * 3600).contains(&(parts.day.end - parts.day.start)),
            "Influencer requires one local reporting day"
        );
        parts.sources = crate::tools::source::top_articles(&parts.sources, "emotion", |s| {
            s.classifier_world.as_ref()
        });
        // Capture time and excluded receipts are diagnostic, not generation inputs.
        let input_hash = hash_components(&serde_json::to_string(&(
            &parts.subject,
            parts.day.start,
            parts.day.end,
            &parts.sources,
            parts.assemble(),
            VIBE_PROMPT_VERSION,
            SCORE_VERSION,
            INSTRUCTIONS,
            SCHEMA_RAW,
        ))?);
        Ok(Self { parts, input_hash })
    }
}

pub fn generation_options(temperature: f64, num_ctx: i32, num_predict: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(INSTRUCTIONS.system().to_owned()),
        temperature: Some(temperature),
        num_predict,
        num_ctx: if num_ctx > 0 {
            num_ctx
        } else {
            crate::harness::model::VOICE_NUM_CTX_PACKET
        },
        json_mode: false,
        format_schema: Some(schema()),
        format_schema_raw: Some(SCHEMA_RAW.to_string()),
    }
}

/// The material fingerprint includes the concrete transport request and model settings.
pub fn request_hash(
    backend: &dyn crate::harness::model::Inference,
    assignment: &Assignment,
    num_ctx: i32,
) -> Result<String> {
    Ok(hash_components(&serde_json::to_string(&(
        &assignment.input_hash,
        backend.request_body(
            &assignment.parts.assemble(),
            &generation_options(VIBE_TEMPERATURE, num_ctx, VIBE_NUM_PREDICT),
        ),
    ))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_preserves_compact_system_and_body_score_hook_order() {
        let options = generation_options(VIBE_TEMPERATURE, 4096, VIBE_NUM_PREDICT);
        assert_eq!(options.system.as_deref(), Some(INSTRUCTIONS.system()));
        assert_eq!(options.format_schema, Some(schema()));
        assert_eq!(options.format_schema_raw.as_deref(), Some(SCHEMA_RAW));
        for variant in SCHEMA_RAW.split("\"required\":").skip(1) {
            assert!(variant.starts_with("[\"body\",\"score\",\"headline\"]"));
            let properties = variant.split_once("\"properties\":").unwrap().1;
            assert!(properties.find("\"body\"").unwrap() < properties.find("\"score\"").unwrap());
            assert!(
                properties.find("\"score\"").unwrap() < properties.find("\"headline\"").unwrap()
            );
        }
        assert_eq!(options.temperature, Some(0.0));
        assert_eq!(options.num_predict, 3500);
        assert_eq!(options.num_ctx, 4096);
        assert_eq!(
            generation_options(VIBE_TEMPERATURE, 0, VIBE_NUM_PREDICT).num_ctx,
            7900
        );
    }
}

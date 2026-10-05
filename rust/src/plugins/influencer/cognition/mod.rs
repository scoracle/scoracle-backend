//! Influencer parts and one cognition call: synthesize and articulate the supplied world.
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::meta::EntityMeta;
use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Studio};
use anyhow::Result;
use serde_json::{json, Value};

mod fresh;
mod prompt;
pub use crate::plugins::support::form::{
    observation_schema as schema, ObservationParser as VibeParser, ObservationReply as VibeReply,
};
pub const VIBE_PROMPT_VERSION: &str = "vibe-frame-v6-six-part";
pub const VIBE_SYSTEM_PROMPT: &str = prompt::TASK;
pub const VIBE_TEMPERATURE: f64 = 0.0;
pub const VIBE_NUM_PREDICT: i32 = 600;
pub const SOURCE_BUDGET_BYTES: usize = 6000;
pub const LOOKBACK_SECONDS: i64 = 72 * 3600;

#[derive(Clone, Debug)]
pub struct VibeScore {
    pub sentiment: Option<i32>,
    pub vibe_prompt: Option<String>,
    pub hook: Option<String>,
    pub input_components_json: String,
}
pub type VibeOutput = Generation<VibeScore>;

#[derive(Clone, Debug)]
pub struct Assignment {
    pub subject: EntityMeta,
    pub source: SourceContext,
    pub history: Vec<super::memories::HistoryItem>,
    pub input_components_json: String,
    pub input_hash: String,
}

/// Deterministic source admission precedes articulation.
pub fn source_disposition(text: &str, published_at: i64, now: i64) -> Option<&'static str> {
    if text.trim().is_empty() {
        return Some("empty_source");
    }
    if text.len() > SOURCE_BUDGET_BYTES {
        return Some("source_budget_exceeded");
    }
    if published_at > now || now.saturating_sub(published_at) > LOOKBACK_SECONDS {
        return Some("outside_fresh_window");
    }
    if crate::plugins::support::source::contains_instruction_override(text) {
        return Some("source_instruction_override");
    }
    None
}

/// The same assembled package is used by production and replay.
pub fn assembled_prompt(assignment: &Assignment) -> String {
    assemble(&assignment.subject, &assignment.source, &assignment.history)
}

/// Field order is the wire order. Empty history is omitted, not an empty array.
pub fn assemble(
    subject: &EntityMeta,
    source: &SourceContext,
    history: &[super::memories::HistoryItem],
) -> String {
    use serde::Serialize;
    #[derive(Serialize)]
    struct Input<'a> {
        meta: crate::plugins::meta::WritingIdentity<'a>,
        fresh: crate::plugins::support::source::Reporting<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        memories: Option<&'a [super::memories::HistoryItem]>,
        voice: &'static str,
        form: serde_json::Value,
    }
    let form = crate::plugins::support::form::observation_form();
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        fresh: fresh::prepare(source),
        memories: (!history.is_empty()).then_some(history),
        voice: crate::plugins::influencer::voice::VOICE,
        form,
    })
    .expect("influencer world serializes")
}

/// This plugin's parts, in a form a quality fixture can store.
///
/// A fixture that stores only a rendered prompt cannot detect a changed
/// assembler: the stored string keeps passing while production sends something
/// else. Storing the parts and rebuilding through [`assemble`] makes that a test
/// failure. The type lives here because this plugin owns what its parts are;
/// the harness only chooses the JSON.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    pub source: SourceContext,
    pub history: Vec<super::memories::HistoryItem>,
}

impl Parts {
    pub fn assemble(&self) -> String {
        assemble(&self.subject, &self.source, &self.history)
    }
}

pub fn generation_options(temperature: f64, num_ctx: i32, num_predict: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(VIBE_SYSTEM_PROMPT.into()),
        temperature: Some(temperature),
        num_predict,
        num_ctx,
        json_mode: false,
        format_schema: Some(schema()),
        format_schema_raw: None,
    }
}
pub async fn create(
    studio: &Studio<'_>,
    assignment: &Assignment,
    num_ctx: i32,
) -> Result<(Option<VibeOutput>, Value)> {
    let extracted = studio
        .extract(
            &assembled_prompt(assignment),
            &generation_options(VIBE_TEMPERATURE, num_ctx, VIBE_NUM_PREDICT),
            &VibeParser,
            |_| None,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let receipt = json!({"model_version":extracted.model,"prompt_version":VIBE_PROMPT_VERSION,
        "input_hash":assignment.input_hash,"raw_response":extracted.raw_response,
        "eval_count":extracted.eval_count,"wall_ms":extracted.wall_ms,
        "input_components":serde_json::from_str::<Value>(&assignment.input_components_json)?,
        "request_body":extracted.request_body});
    let Some(reply) = extracted.value else {
        return Ok((None, receipt));
    };
    Ok((
        Some(Generation::called(
            VibeScore {
                sentiment: None,
                hook: Some(fresh::title(&assignment.source, &assignment.subject.name)),
                vibe_prompt: reply.body,
                input_components_json: assignment.input_components_json.clone(),
            },
            extracted.model,
            VIBE_PROMPT_VERSION,
            vec![assignment.source.article_id],
            Some(assignment.input_hash.clone()),
            call,
        )),
        receipt,
    ))
}

#[cfg(test)]
mod tests;

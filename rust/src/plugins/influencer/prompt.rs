//! Influencer source admission, memory selection, and exact model input.
use super::memories;
use crate::plugins::harvester::delivery::SourceContext;
use crate::plugins::meta::EntityMeta;
use crate::studio::model::GenerateOptions;
use crate::util::hash_components;
use anyhow::Result;
use serde_json::{json, Value};
use sqlx::PgPool;

pub const TASK: &str = "Describe what fresh.publisher_excerpt reports about meta, with relevant dated context from memories when supplied. Preserve who said what, attribution, dates and uncertainty. published_at dates a report, not necessarily the events it describes. Absent memories means no history was supplied. Report feelings only when the sources state them; a routine update can remain a routine update.
voice and form are writing instructions, not facts about the subject. Use voice for tone and form for output structure and limits. Return the description in body, or a null body when the supplied evidence supports no description.";

pub const VIBE_PROMPT_VERSION: &str = "vibe-frame-v6-six-part";
pub const VIBE_TEMPERATURE: f64 = 0.0;
pub const VIBE_NUM_PREDICT: i32 = 600;
pub const SOURCE_BUDGET_BYTES: usize = 6000;
pub const LOOKBACK_SECONDS: i64 = 72 * 3600;

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
    if crate::tools::source::contains_instruction_override(text) {
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
        fresh: crate::tools::source::Reporting<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        memories: Option<&'a [super::memories::HistoryItem]>,
        voice: &'static str,
        form: serde_json::Value,
    }
    let form = crate::tools::form::observation_form();
    serde_json::to_string(&Input {
        meta: subject.for_writing(),
        fresh: crate::tools::source::Reporting::new(
            &source.source,
            source.published_at_epoch,
            &source.context,
        ),
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
        system: Some(TASK.into()),
        temperature: Some(temperature),
        num_predict,
        num_ctx,
        json_mode: false,
        format_schema: Some(crate::tools::form::observation_schema()),
        format_schema_raw: None,
    }
}
/// Read-only preparation shared by the worker and evaluation. No generative decisions.
pub async fn prepare_assignment(
    pool: &PgPool,
    subject: EntityMeta,
    source: &SourceContext,
    now: i64,
) -> Result<(Option<Assignment>, Value)> {
    let reason = match source.published_at_epoch {
        Some(published) => source_disposition(&source.context, published, now),
        None => source_disposition(&source.context, now, now),
    };
    if let Some(reason) = reason {
        return Ok((
            None,
            json!({"contract":VIBE_PROMPT_VERSION,"source":source,"reason":reason}),
        ));
    }
    let study = memories::load(pool, &subject, source).await?;
    let history = study
        .as_ref()
        .map(|s| memories::select(s, &subject, source))
        .transpose()?
        .unwrap_or_default();
    let input_components_json = json!({"subject":subject,"source":source,
        "history":history,"memory_study":study,"prompt_version":VIBE_PROMPT_VERSION})
    .to_string();
    let input_hash = hash_components(&input_components_json);
    Ok((
        Some(Assignment {
            subject,
            source: source.clone(),
            history,
            input_components_json,
            input_hash,
        }),
        json!({"contract":VIBE_PROMPT_VERSION}),
    ))
}

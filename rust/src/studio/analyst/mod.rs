//! The Analyst creates a reading from prepared form and mood evidence.
//! No database, queue, sibling character, or model-host knowledge belongs here.

use super::model::GenerateOptions;
use super::{Generation, GenerationCall, Outcome, Parser, Publisher, Studio};
use crate::util::{hash_components, round1};
use anyhow::{anyhow, Result};

mod inputs;
pub mod prompt;
pub use inputs::build_momentum_prompt;
pub use prompt::{MOMENTUM_PROMPT_VERSION, MOMENTUM_SYSTEM_PROMPT};

pub const MOMENTUM_OUTPUT_CONTRACT_VERSION: &str = "momentum-summary-v1";

/// Relevant values from the current statistical read; no Scout prose is carried.
#[derive(Clone, Debug)]
pub struct Form {
    pub notability: i32,
    pub rating_trajectory: String,
    pub rating_trajectory_label: String,
}

#[derive(Clone, Debug)]
pub struct Mood {
    pub sentiment: i32,
}

/// Analytical movement, independent of Oracle's finished-card representation.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub vibe_slope: Option<f64>,
    pub vibe_samples: i32,
    pub rating_slope: Option<f64>,
    pub rating_samples: i32,
    pub momentum_score: Option<f64>,
}

impl Snapshot {
    pub fn empty(&self) -> bool {
        self.vibe_slope.is_none() && self.rating_slope.is_none() && self.momentum_score.is_none()
    }
}

/// Complete materials for one creation. The application owns the durable subject key.
#[derive(Clone, Debug)]
pub struct Assignment {
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
    pub context: MomentumContext,
    pub memory: Option<String>,
    pub voice_num_ctx: i32,
}

/// Keep Momentum on the incumbent stats route until a broader fixture set proves a split.
pub const MOMENTUM_TEMPERATURE: f64 = 0.3;

// A single direction read — two rails and what they are doing to each other — is a handful of
// sentences on a card. Nothing here scales with story count.
pub const MOMENTUM_NUM_PREDICT: i32 = 700;

/// Scores within this band are steady; values at or beyond it rise or fall by sign.
pub const MOMENTUM_STEADY_BAND: f64 = 10.0;

#[derive(Clone, Debug)]
pub struct MomentumContext {
    pub season: i32,
    pub rating: Option<Form>,
    pub vibe: Option<Mood>,
    pub snapshot: Snapshot,
    pub input_components_json: String,
    pub input_hash: String,
}

impl MomentumContext {
    pub fn empty(&self) -> bool {
        self.rating.is_none() && self.vibe.is_none() && self.snapshot.empty()
    }
}

/// Parsed model prose. Direction and conviction are deterministic product fields.
#[derive(Clone, Debug)]
pub struct MomentumReply {
    /// The Analyst's read.
    pub blurb: String,
    /// Optional card title.
    pub headline: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MomentumSummary {
    pub direction: String,
    pub score: i32,
    pub blurb: String,
    /// Model-emitted card title.
    pub headline: Option<String>,
    pub season: i32,
    pub input_components_json: String,
}

pub type MomentumOutput = Generation<MomentumSummary>;

pub struct MomentumParser;

impl Parser<MomentumReply> for MomentumParser {
    fn parse(&self, raw: &str) -> Result<Option<MomentumReply>> {
        // Keep a bounded raw excerpt so malformed output is diagnosable.
        let mut reply = parse_momentum_reply(raw).ok_or_else(|| {
            anyhow!(
                "momentum: invalid response (raw={:?})",
                crate::util::truncate_bytes(raw.trim(), 160)
            )
        })?;
        // Production guards live at the Parser seam; eval can still inspect the raw parse.
        crate::studio::form::validate_body(&reply.blurb)?;
        crate::studio::form::validate_hook(reply.headline.as_deref())?;
        if let Some(p) = crate::studio::guards::first_banned_phrase(
            &reply.blurb,
            crate::studio::guards::MOMENTUM_BANNED_PHRASES,
        ) {
            tracing::warn!(
                guard = "momentum_banned_phrase",
                phrase = p,
                "momentum READ rejected"
            );
            anyhow::bail!("momentum: READ carries banned phrase {p:?}");
        }
        if let Some(p) = crate::studio::guards::first_product_name(&reply.blurb) {
            tracing::warn!(guard = "product_name", name = p, "momentum READ rejected");
            anyhow::bail!("momentum: READ names product {p:?}");
        }
        // Sporting numbers are evidence; internal field citations leak the input contract.
        if crate::studio::guards::has_bookkeeping_citation(&reply.blurb) {
            tracing::warn!(guard = "bookkeeping_citation", "momentum READ rejected");
            anyhow::bail!("momentum: READ carries a bookkeeping citation");
        }
        // A bad optional title degrades to NULL without costing the read.
        reply.headline = crate::studio::guards::settle_title("analyst", reply.headline.as_deref());
        Ok(Some(reply))
    }
}

/// Deterministic direction from the ±100-scale signed slope average. No snapshot means steady.
pub fn momentum_direction_from_score(momentum_score: Option<f64>) -> &'static str {
    match momentum_score {
        Some(s) if s >= MOMENTUM_STEADY_BAND => "rising",
        Some(s) if s <= -MOMENTUM_STEADY_BAND => "falling",
        _ => "steady",
    }
}

/// Deterministic ±5 conviction from `momentum_score`. The steady band maps to zero or a one-point
/// lean; larger absolute scores step through the remaining bands. No snapshot maps to zero.
pub fn momentum_conviction_from_score(momentum_score: Option<f64>) -> i32 {
    let Some(s) = momentum_score else { return 0 };
    let mag = s.abs();
    let sign = if s < 0.0 { -1 } else { 1 };
    let step = if mag < MOMENTUM_STEADY_BAND / 2.0 {
        return 0; // genuinely flat: no measured lean at all
    } else if mag < 20.0 {
        1 // covers the top half of the steady band AND the first rising/falling notch
    } else if mag < 35.0 {
        2
    } else if mag < 55.0 {
        3
    } else if mag < 80.0 {
        4
    } else {
        5
    };
    sign * step
}

pub fn build_momentum_input_components(
    rating: Option<&Form>,
    vibe: Option<&Mood>,
    mom: &Snapshot,
) -> String {
    let mut components = serde_json::Map::new();
    components.insert(
        "prompt_version".into(),
        serde_json::json!(MOMENTUM_PROMPT_VERSION),
    );
    if let Some(r) = rating {
        components.insert("notability".into(), serde_json::json!(r.notability));
        components.insert(
            "rating_trajectory".into(),
            serde_json::json!(r.rating_trajectory),
        );
        if !r.rating_trajectory_label.is_empty() {
            components.insert(
                "rating_trajectory_label".into(),
                serde_json::json!(r.rating_trajectory_label),
            );
        }
    }
    if let Some(v) = vibe {
        // Sentiment only — the vibe felt-read prose stays in the PROMPT but out of the hash
        // (F1, material-only debounce): vibe generates at temp 0.7, so its prose changes on
        // every re-run even when material is byte-identical; hashing it cascaded
        // momentum→sigil→oracle regenerations on zero material change.
        components.insert("vibe_sentiment".into(), serde_json::json!(v.sentiment));
    }
    if let Some(s) = mom.rating_slope {
        components.insert("momentum_rating_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_rating_samples".into(),
            serde_json::json!(mom.rating_samples),
        );
    }
    if let Some(s) = mom.vibe_slope {
        components.insert("momentum_vibe_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_vibe_samples".into(),
            serde_json::json!(mom.vibe_samples),
        );
    }
    if let Some(score) = mom.momentum_score {
        components.insert("momentum_score".into(), serde_json::json!(round1(score)));
    }
    serde_json::Value::Object(components).to_string()
}

pub fn parse_momentum_reply(raw: &str) -> Option<MomentumReply> {
    if let Ok(card) = serde_json::from_str::<crate::studio::form::CardReply>(raw.trim()) {
        return Some(MomentumReply {
            blurb: crate::studio::form::normalize_body(&card.body),
            headline: Some(card.headline),
        });
    }
    let rest = raw.trim().strip_prefix("READ:")?;
    let (body, headline) = match rest.split_once("\nHEADLINE:") {
        Some((body, title)) if !title.contains('\n') => {
            let title = title.trim();
            (body, (!title.is_empty()).then(|| title.to_string()))
        }
        Some(_) => return None,
        None => (rest, None),
    };

    let blurb =
        crate::studio::guards::clean_served_prose(&crate::studio::form::normalize_body(body));
    if blurb.is_empty() {
        return None;
    }
    // Reject a foreign-script generation so the work item retries.
    if crate::studio::guards::has_foreign_script(&blurb) {
        return None;
    }
    Some(MomentumReply { blurb, headline })
}

impl MomentumContext {
    pub fn new(season: i32, rating: Option<Form>, vibe: Option<Mood>, snapshot: Snapshot) -> Self {
        let input_components_json =
            build_momentum_input_components(rating.as_ref(), vibe.as_ref(), &snapshot);
        let input_hash = hash_components(&input_components_json);
        Self {
            season,
            rating,
            vibe,
            snapshot,
            input_components_json,
            input_hash,
        }
    }
}

/// A complete assignment: create, validate, and publish, or finish quietly without a call.
/// Publication errors propagate; the application retains responsibility for durable retries.
pub async fn run<P: Publisher<MomentumSummary>>(
    studio: &Studio<'_>,
    assignment: &Assignment,
    publisher: &P,
) -> Result<Outcome<P::Receipt>> {
    let Some(out) = create(studio, assignment).await? else {
        return Ok(Outcome::NoMaterial);
    };
    Ok(Outcome::Published(publisher.publish(&out).await?))
}

pub async fn create(
    studio: &Studio<'_>,
    assignment: &Assignment,
) -> Result<Option<MomentumOutput>> {
    let ctx = &assignment.context;
    if ctx.empty() {
        return Ok(None);
    }
    let prompt = build_momentum_prompt(
        &assignment.entity_type,
        &assignment.entity_name,
        &assignment.sport,
        ctx.rating.as_ref(),
        ctx.vibe.as_ref(),
        &ctx.snapshot,
        assignment.memory.as_deref(),
    );
    let opts = generation_options(assignment.voice_num_ctx);
    let extracted = studio.extract(&prompt, &opts, &MomentumParser).await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("momentum: parser returned no value"))?;
    let headline = reply.headline.filter(|title| {
        let named = crate::studio::guards::title_names_entity(title, &assignment.entity_name);
        if !named {
            tracing::warn!(seat = "analyst", guard = "title_entity_absent",
                entity = %assignment.entity_name, %title, "headline names no form of the entity; dropped");
        }
        named
    });
    Ok(Some(Generation::called(
        MomentumSummary {
            direction: momentum_direction_from_score(ctx.snapshot.momentum_score).to_string(),
            score: momentum_conviction_from_score(ctx.snapshot.momentum_score),
            blurb: reply.blurb,
            headline,
            season: ctx.season,
            input_components_json: ctx.input_components_json.clone(),
        },
        model,
        MOMENTUM_PROMPT_VERSION,
        Vec::new(),
        Some(ctx.input_hash.clone()),
        call,
    )))
}

/// Preserve the current voice-window reservation; all adapters use these same options.
pub fn generation_options(voice_num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(MOMENTUM_SYSTEM_PROMPT.to_string()),
        temperature: Some(MOMENTUM_TEMPERATURE),
        num_predict: MOMENTUM_NUM_PREDICT,
        num_ctx: voice_num_ctx,
        json_mode: false,
        format_schema: Some(crate::studio::form::card_schema(false)),
        format_schema_raw: None,
    }
}

#[cfg(test)]
mod tests;

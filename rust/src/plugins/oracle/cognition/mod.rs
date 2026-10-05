//! The Oracle creates the terminal crown from five prepared cards.
//!
//! Storage, readiness policy, routing, queue ownership, and publication live in the application.
//! With no available cards, Studio returns a NULL marker without a model call.

use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use crate::util::round1;
use anyhow::{anyhow, bail, Result};

mod parts;
mod prompt;
pub use parts::assemble as assemble_context;
pub use prompt::{ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT};

/// Output contract captured separately from the prompt version in the diagnostic ledger.
pub const ORACLE_OUTPUT_CONTRACT_VERSION: &str = "oracle-reading-v4-prose";

/// Production crown temperature. Fixtures pin zero.
pub const ORACLE_TEMPERATURE: f64 = 0.6;

/// Runtime headroom for the JSON response, not a requested prose length.
pub const ORACLE_NUM_PREDICT: i32 = 700;

/// Output reservation inside the small voice window.
pub const SMALL_WINDOW_NUM_PREDICT: i32 = 700;

pub fn generation_options(temperature: f64, num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(ORACLE_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: ORACLE_NUM_PREDICT,
        num_ctx,
        json_mode: false,
        format_schema: Some(parts::prose().schema()),
        format_schema_raw: None,
    }
}

// ---------------------------------------------------------------------------
// Pillar values.
// ---------------------------------------------------------------------------

/// One narrative from the entity's latest generation. `impact` is widened from a database
/// integer and rendered without a fractional part.
#[derive(Clone, Debug)]
pub struct SynthNarrative {
    pub title: String,
    pub body: String,
    pub impact: f64,
    pub trajectory: String,
    /// Prompt-only corroboration and freshness; excluded from the material hash.
    pub source_count: i32,
    pub source_age_days: Option<i32>,
}

/// The Scout's rating pillar. A latest NULL body suppresses the pillar.
#[derive(Clone, Debug)]
pub struct SynthRating {
    pub body: String,
    pub notability: i32,
    pub rating_trajectory: String,
    pub rating_trajectory_label: String,
}

/// The vibe pillar (P3): the latest felt-read product, distinct from the Momentum trajectory.
#[derive(Clone, Debug)]
pub struct SynthVibe {
    pub sentiment: i32,
    pub prompt: String,
}

/// The momentum pillar (P4): durable trajectory values from `momentum_scores`.
#[derive(Clone, Debug, Default)]
pub struct SynthMomentum {
    pub direction: Option<String>,
    pub blurb: Option<String>,
    pub input_hash: Option<String>,
    pub vibe_slope: Option<f64>,
    pub vibe_samples: i32,
    pub rating_slope: Option<f64>,
    pub rating_samples: i32,
    pub momentum_score: Option<f64>,
}

/// The Insider's finished card. Its score is kept for product math, not prose input.
#[derive(Clone, Debug)]
pub struct SynthInsider {
    pub body: String,
    pub score: i32,
    pub generated_at: Option<String>,
}

/// The five cards handed to the Oracle. Missing cards remain explicit rather than being filled
/// from older products or raw evidence.
#[derive(Clone, Debug, Default)]
pub struct Cards {
    pub narratives: Vec<SynthNarrative>,
    pub rating: Option<SynthRating>,
    pub vibe: Option<SynthVibe>,
    pub momentum: SynthMomentum,
    pub insider: Option<SynthInsider>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pillar {
    Narratives,
    Rating,
    Vibe,
    Momentum,
    Insider,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Empty,
    Partial { missing: Vec<Pillar> },
    Complete,
}

impl Cards {
    pub fn readiness(&self) -> Readiness {
        let mut missing = Vec::new();
        if self.narratives.is_empty() {
            missing.push(Pillar::Narratives);
        }
        if self.rating.is_none() {
            missing.push(Pillar::Rating);
        }
        if self.vibe.is_none() {
            missing.push(Pillar::Vibe);
        }
        if self.momentum.empty() {
            missing.push(Pillar::Momentum);
        }
        if self.insider.is_none() {
            missing.push(Pillar::Insider);
        }
        if missing.len() == 5 {
            Readiness::Empty
        } else if missing.is_empty() {
            Readiness::Complete
        } else {
            Readiness::Partial { missing }
        }
    }
}

/// Subject of an Oracle assignment. Durable ids and work revisions stay outside Studio.
#[derive(Clone, Debug)]
pub struct Subject {
    pub entity_id: i32,
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
}

/// Complete prepared assignment for one crown.
#[derive(Clone, Debug)]
pub struct Assignment {
    pub subject: Subject,
    pub season: i32,
    pub cards: Cards,
    pub input_components_json: String,
    pub input_hash: String,
    pub options: GenerateOptions,
}

impl SynthMomentum {
    /// empty mirrors `synthMomentum.empty()`: no momentum signal at all.
    pub fn empty(&self) -> bool {
        self.direction.is_none()
            && self.blurb.is_none()
            && self.vibe_slope.is_none()
            && self.rating_slope.is_none()
            && self.momentum_score.is_none()
    }
}

/// Complete `sigil_synthesis` row before persistence. The plugin supplies every claim and score.
#[derive(Clone, Debug)]
pub struct SigilSynthesis {
    /// `None` ⇒ no-pillar NULL marker (no model call was made).
    pub score: Option<i32>,
    /// The crown reading — the served voice. `None` ⇒ marker; `Some` ⇒ a scored reading.
    pub reading: Option<String>,
    /// The season this convergence is for (current_season, resolved + stamped). Never NULL.
    pub season: i32,
    /// Canonical input-components JSON persisted as the hash pre-image. `"{}"` for a marker.
    pub input_components_json: String,
    /// Optional model-emitted title.
    pub headline: Option<String>,
    /// Deterministic convergence (1-100) from `pillar_convergence` — NOT model-emitted. `None`
    /// for the marker and when no directional pillar pair exists. NOT part of the `input_hash`.
    pub convergence: Option<i32>,
    /// The computed omen the reading was drawn under (`compute_omen`). `None` for the marker.
    pub omen: Option<&'static str>,
}

pub type SigilOutput = Generation<SigilSynthesis>;

// ---------------------------------------------------------------------------
// Deterministic trend math.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Input components and material-only debounce hash. Upstream model prose is prompt context but
// never part of this key.
// ---------------------------------------------------------------------------

/// Build canonical input-components JSON. Narrative keys are always present; the rest are
/// conditional.
pub fn build_synthesis_input_components(cards: &Cards) -> String {
    let mut narratives: Vec<_> = cards
        .narratives
        .iter()
        .map(|n| {
            serde_json::json!({
                "title": n.title, "body": n.body, "impact": n.impact,
                "trajectory": n.trajectory, "source_count": n.source_count,
                "source_age_days": n.source_age_days,
            })
        })
        .collect();
    narratives.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    serde_json::json!({
        "prompt_version": ORACLE_PROMPT_VERSION,
        "narratives": narratives,
        "rating": cards.rating.as_ref().map(|r| serde_json::json!({
            "body": r.body, "notability": r.notability,
            "trajectory": r.rating_trajectory,
            "trajectory_label": r.rating_trajectory_label,
        })),
        "vibe": cards.vibe.as_ref().map(|v| serde_json::json!({
            "body": v.prompt, "sentiment": v.sentiment,
        })),
        "momentum": serde_json::json!({
            "body": cards.momentum.blurb,
            "direction": cards.momentum.direction,
            "input_hash": cards.momentum.input_hash,
            "rating_slope": cards.momentum.rating_slope.map(round1),
            "rating_samples": cards.momentum.rating_samples,
            "vibe_slope": cards.momentum.vibe_slope.map(round1),
            "vibe_samples": cards.momentum.vibe_samples,
            "score": cards.momentum.momentum_score.map(round1),
        }),
        "insider": cards.insider.as_ref().map(|r| serde_json::json!({
            "body": r.body, "score": r.score, "generated_at": r.generated_at,
        })),
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Prompt assembly.
// ---------------------------------------------------------------------------

/// Deterministic cross-pillar direction comparison handed to the model as a decided fact.
#[derive(Clone, Debug, PartialEq)]
pub struct PillarComparison {
    pub label: String,
    pub agree: bool,
}

/// Reduce a value to a direction sign: `None` = not directional (skip the comparison).
fn trajectory_sign(key: &str) -> Option<i8> {
    match key {
        "rising" | "heating_up" => Some(1),
        "falling" | "cooling_off" => Some(-1),
        _ => None,
    }
}

fn sentiment_sign(sentiment: i32) -> Option<i8> {
    if sentiment >= 60 {
        Some(1)
    } else if sentiment <= 40 {
        Some(-1)
    } else {
        None
    }
}

fn sign_word(s: i8) -> &'static str {
    if s > 0 {
        "positive"
    } else {
        "negative"
    }
}

/// build_pillar_divergence emits one comparison per directional pillar pair that is actually
/// present. Neutral/steady/absent signals produce NO line (a steady lens neither agrees nor
/// disagrees — the system prompt's own convergence rule). Pure and deterministic; the card is
/// prompt-only and derives entirely from values already in the input hash, so it can never
/// trigger a regeneration by itself.
pub fn build_pillar_divergence(
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
) -> Vec<PillarComparison> {
    let mut out = Vec::new();

    // Momentum is the sole direction signal; the Oracle never reads the raw tracker.
    let vibe_sign = vibe.and_then(|v| sentiment_sign(v.sentiment));
    let mom_sign = mom.direction.as_deref().and_then(trajectory_sign);
    // Profile strength: the LEVEL sign (is this an elite or a weak profile), distinct from the
    // direction sign. The classic rails conflict the fixtures measure — "strong profile vs
    // sliding momentum and negative narrative" — is a LEVEL-vs-direction disagreement that
    // direction pairs alone cannot see. (Narrative heating_up/cooling_off is deliberately NOT
    // compared: it measures story intensity, not valence — a negative story heating up must
    // not read as "positive narrative".)
    let strength_sign = rating.and_then(|r| {
        if r.notability >= 70 {
            Some(1i8)
        } else if r.notability <= 35 {
            Some(-1i8)
        } else {
            None
        }
    });
    let strength_word = |s: i8| if s > 0 { "strong" } else { "weak" };
    let mut push = |label: String, a: i8, b: i8| {
        out.push(PillarComparison {
            label,
            agree: (i32::from(a) * i32::from(b)) > 0,
        });
    };

    if let (Some(v), Some(m)) = (vibe_sign, mom_sign) {
        push(
            format!("Vibe ({}) vs Momentum ({})", sign_word(v), sign_word(m)),
            v,
            m,
        );
    }
    if let (Some(s), Some(m)) = (strength_sign, mom_sign) {
        push(
            format!(
                "Profile strength ({}) vs Momentum ({})",
                strength_word(s),
                sign_word(m)
            ),
            s,
            m,
        );
    }
    if let (Some(s), Some(v)) = (strength_sign, vibe_sign) {
        push(
            format!(
                "Profile strength ({}) vs Vibe ({})",
                strength_word(s),
                sign_word(v)
            ),
            s,
            v,
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Deterministic omen and convergence: code decides, the model narrates.
// ---------------------------------------------------------------------------

/// Closed omen set used by the database constraint and served card.
pub const OMENS: [&str; 4] = ["ascendant", "steady", "waning", "crossroads"];

fn direction_sign(key: &str) -> i32 {
    match key {
        "rising" => 1,
        "falling" => -1,
        _ => 0,
    }
}

/// pillar_convergence turns the deterministic pillar comparisons into a 1-100 agreement number —
/// a computed measurement, not a model opinion. `round(100·agree/total)` floored at 1; `None` when no directional pair
/// exists (a quiet spread has nothing to converge on). The floor matches the DB contract
/// (`sigil_synthesis_convergence_check`: NULL or 1-100) — an all-disagree spread rounds to 0,
/// which the check rejects and which carries no product meaning beyond 1 (anything ≤ 50 is
/// already a crossroads to `compute_omen`, faithfully preserving the panel's soft rule).
pub fn pillar_convergence(comparisons: &[PillarComparison]) -> Option<i32> {
    if comparisons.is_empty() {
        return None;
    }
    let agree = comparisons.iter().filter(|c| c.agree).count();
    Some((((agree as f64 / comparisons.len() as f64) * 100.0).round() as i32).max(1))
}

/// compute_omen decides the reading's direction deterministically:
/// - a split spread (convergence ≤ 50 — half or more of the directional pairs disagree) is a
///   `crossroads` regardless of net direction — the contested arc IS the story;
/// - otherwise Momentum decides alone: positive ⇒
///   `ascendant`, negative ⇒ `waning`, nothing directional ⇒ `steady`.
pub fn compute_omen(convergence: Option<i32>, mom: &SynthMomentum) -> &'static str {
    if let Some(c) = convergence {
        if c <= 50 {
            return "crossroads";
        }
    }
    let net = mom.direction.as_deref().map(direction_sign).unwrap_or(0);
    if net > 0 {
        "ascendant"
    } else if net < 0 {
        "waning"
    } else {
        "steady"
    }
}

// ---------------------------------------------------------------------------
// Re-exported for evaluation of served prose.
pub use crate::plugins::support::guards::count_sentences;

struct ReadingParser;

impl Parser<String> for ReadingParser {
    fn parse(&self, raw: &str) -> Result<Option<String>> {
        let prose = parts::prose();
        let map = crate::plugins::support::form::parse_prose_map(raw, &prose.keys, prose.dims)?;
        let Some(reading) = map.get("reading") else {
            return Ok(None);
        };
        let reading = crate::plugins::support::guards::clean_served_prose(
            &crate::plugins::support::form::normalize_body(reading),
        );
        crate::plugins::support::form::validate_body(&reading)?;
        if crate::plugins::support::guards::has_bookkeeping_citation(&reading)
            || crate::plugins::support::guards::first_product_name(&reading).is_some()
            || crate::plugins::support::guards::has_foreign_script(&reading)
        {
            bail!("crown: reading violates served prose guard");
        }
        Ok(Some(reading))
    }
}

/// Create one crown from an explicit, service-free assignment.
pub async fn create(studio: &Studio<'_>, assignment: &Assignment) -> Result<SigilOutput> {
    if assignment.cards.readiness() == Readiness::Empty {
        return Ok(Generation::uncalled(
            SigilSynthesis {
                score: None,
                reading: None,
                headline: None,
                season: assignment.season,
                input_components_json: "{}".to_string(),
                convergence: None,
                omen: None,
            },
            studio.model_name().to_string(),
            ORACLE_PROMPT_VERSION,
            Vec::new(),
            None,
        ));
    }

    let cards = &assignment.cards;
    let comparisons =
        build_pillar_divergence(cards.rating.as_ref(), cards.vibe.as_ref(), &cards.momentum);
    let convergence = pillar_convergence(&comparisons);
    let omen = compute_omen(convergence, &cards.momentum);
    let prompt = parts::assemble(&assignment.subject, cards).render();
    let options = assignment.options.clone();
    let extracted = studio
        .extract(
            &prompt,
            &options,
            &ReadingParser,
            crate::plugins::support::prompt::structured_correction,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reading = extracted
        .value
        .ok_or_else(|| anyhow!("crown: parser returned no value"))?;
    crate::plugins::support::form::validate_body(&reading)?;
    if !crate::plugins::support::guards::title_names_entity(
        &reading,
        &assignment.subject.entity_name,
    ) {
        tracing::warn!(guard = "entity_identity", "crown reading rejected");
        bail!(
            "crown: reading does not name entity {:?}",
            assignment.subject.entity_name
        );
    }

    Ok(Generation::called(
        SigilSynthesis {
            score: Some(crown_score(cards)),
            reading: Some(reading),
            headline: crate::plugins::support::guards::settle_title(
                "oracle",
                Some(&format!(
                    "{}: the current picture",
                    assignment.subject.entity_name
                )),
            ),
            season: assignment.season,
            input_components_json: assignment.input_components_json.clone(),
            convergence,
            omen: Some(omen),
        },
        model,
        ORACLE_PROMPT_VERSION,
        Vec::new(),
        Some(assignment.input_hash.clone()),
        call,
    ))
}

fn crown_score(cards: &Cards) -> i32 {
    let mut signals = Vec::new();
    if let Some(rating) = &cards.rating {
        signals.push(rating.notability.clamp(1, 100));
    }
    if let Some(vibe) = &cards.vibe {
        signals.push(vibe.sentiment.clamp(1, 100));
    }
    if let Some(narrative) = cards
        .narratives
        .iter()
        .max_by(|a, b| a.impact.total_cmp(&b.impact))
    {
        signals.push(narrative.impact.round().clamp(1.0, 100.0) as i32);
    }
    if let Some(insider) = &cards.insider {
        signals.push(insider.score.clamp(1, 100));
    }
    if signals.is_empty() {
        50
    } else {
        (signals.iter().sum::<i32>() as f64 / signals.len() as f64).round() as i32
    }
}

#[cfg(test)]
mod tests;

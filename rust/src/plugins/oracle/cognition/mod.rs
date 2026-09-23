//! The Oracle creates the terminal crown from five prepared cards.
//!
//! Storage, readiness policy, routing, queue ownership, and publication live in the application.
//! With no available cards, Studio returns a NULL marker without a model call.

use crate::studio::model::GenerateOptions;
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use crate::util::{round1, truncate};
use anyhow::{anyhow, bail, Result};

mod brief;
mod inputs;
pub use crate::plugins::support::form::oracle_format_schema;
pub use brief::{CHARACTER, ORACLE_PROMPT_VERSION, ORACLE_SYSTEM_PROMPT};
pub use inputs::{build_crown_prompt, CROWN_CARD_BODY_CAP};

/// Output contract captured separately from the prompt version in the diagnostic ledger.
pub const ORACLE_OUTPUT_CONTRACT_VERSION: &str = "oracle-reading-v2";

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
        format_schema: Some(oracle_format_schema()),
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

/// One prepared transfer-board item. Durable evidence retrieval stays in the application.
#[derive(Clone, Debug)]
pub struct SynthTransfer {
    pub counterparty: String,
    pub heat: i32,
    pub direction: String,
    pub stage: String,
    pub summary: String,
}

/// The five cards handed to the Oracle. Missing cards remain explicit rather than being filled
/// from older products or raw evidence.
#[derive(Clone, Debug, Default)]
pub struct Cards {
    pub narratives: Vec<SynthNarrative>,
    pub rating: Option<SynthRating>,
    pub vibe: Option<SynthVibe>,
    pub momentum: SynthMomentum,
    pub transfers: Vec<SynthTransfer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pillar {
    Narratives,
    Rating,
    Vibe,
    Momentum,
    Transfers,
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
        if self.transfers.is_empty() {
            missing.push(Pillar::Transfers);
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
    pub identity: Option<String>,
    pub input_components_json: String,
    pub input_hash: String,
    pub body_cap: Option<usize>,
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

/// Validated Oracle reply.
#[derive(Clone, Debug)]
pub struct CrownReply {
    /// The reading generated from the available evidence.
    pub reading: String,
    /// Optional title; absence never fails the reading.
    pub headline: Option<String>,
    /// The 1-100 verdict the reading earned, generated LAST. Clamped to 1-100 at parse.
    pub score: i32,
}

/// Complete `sigil_synthesis` row before persistence. The model supplies reading and score;
/// code supplies omen and convergence.
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

/// linear_slope computes the slope of a simple OLS regression on the series [0..N-1] → values.
/// Positive means trending up. Keep this separate from the Scout's mean-centered implementation:
/// their different floating-point accumulation order can move values at bucket boundaries.
#[cfg(test)]
fn linear_slope(vals: &[f64]) -> f64 {
    let n = vals.len() as f64;
    if vals.len() < 2 {
        return 0.0;
    }
    let (mut sum_x, mut sum_y, mut sum_xy, mut sum_xx) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (i, v) in vals.iter().enumerate() {
        let x = i as f64;
        sum_x += x;
        sum_y += *v;
        sum_xy += x * *v;
        sum_xx += x * x;
    }
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-9 {
        return 0.0;
    }
    (n * sum_xy - sum_x * sum_y) / denom
}

/// momentum_score reads the durable signed Momentum trajectory value. It is directional force,
/// not entity quality: positive is rising, negative is sliding, zero is flat.
fn momentum_score(mom: &SynthMomentum) -> Option<i32> {
    mom.momentum_score.map(|s| s.round() as i32)
}

fn momentum_score_label(score: i32) -> &'static str {
    if score >= 3 {
        "surging"
    } else if score >= 1 {
        "rising"
    } else if score <= -3 {
        "falling"
    } else if score <= -1 {
        "sliding"
    } else {
        "steady"
    }
}

// ---------------------------------------------------------------------------
// Input components and material-only debounce hash. Upstream model prose is prompt context but
// never part of this key.
// ---------------------------------------------------------------------------

/// Build canonical input-components JSON. Narrative keys are always present; the rest are
/// conditional.
pub fn build_synthesis_input_components(
    narratives: &[SynthNarrative],
    rating: Option<&SynthRating>,
    vibe: Option<&SynthVibe>,
    mom: &SynthMomentum,
    transfers: &[SynthTransfer],
) -> String {
    let mut titles: Vec<String> = narratives.iter().map(|n| n.title.clone()).collect();
    titles.sort();

    let mut trajectory_pairs: Vec<String> = narratives
        .iter()
        .map(|n| format!("{}:{}", n.title, n.trajectory))
        .collect();
    trajectory_pairs.sort();

    let mut components = serde_json::Map::new();
    components.insert("narrative_titles".into(), serde_json::json!(titles));
    components.insert(
        "narrative_trajectories".into(),
        serde_json::json!(trajectory_pairs),
    );

    if let Some(r) = rating {
        // The Oracle reads the Scout and Analyst outputs, not the raw trajectory marker.
        components.insert("notability".into(), serde_json::json!(r.notability));
    }
    if let Some(v) = vibe {
        // Sentiment only — the vibe felt-read prose is PROMPT-ONLY (F1, material-only
        // debounce): vibe generates at temp 0.7, so hashing its prose flipped this hash on
        // every vibe re-run even when nothing material moved.
        components.insert("vibe_sentiment".into(), serde_json::json!(v.sentiment));
    }
    if let Some(s) = mom.vibe_slope {
        components.insert("momentum_vibe_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_vibe_samples".into(),
            serde_json::json!(mom.vibe_samples),
        );
    }
    if let Some(s) = mom.rating_slope {
        components.insert("momentum_rating_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_rating_samples".into(),
            serde_json::json!(mom.rating_samples),
        );
    }
    if let Some(score) = mom.momentum_score {
        components.insert("momentum_score".into(), serde_json::json!(round1(score)));
    }
    if let Some(direction) = &mom.direction {
        components.insert("momentum_direction".into(), serde_json::json!(direction));
    }
    // momentum_blurb is PROMPT-ONLY (F1, material-only debounce): the blurb is momentum's
    // model prose, so hashing it made every momentum regeneration flip sigil's hash even when
    // the material signals were unchanged. momentum_summary_hash below is momentum's own
    // input_hash — material-only after F1 — so sigil still re-runs when momentum's INPUTS
    // genuinely move.
    if let Some(input_hash) = &mom.input_hash {
        components.insert(
            "momentum_summary_hash".into(),
            serde_json::json!(input_hash),
        );
    }

    // Transfer heat is conditional and sorted into a stable canonical shape.
    if !transfers.is_empty() {
        let mut lines: Vec<String> = transfers
            .iter()
            .map(|t| format!("{}:{}:{}:{}", t.counterparty, t.heat, t.direction, t.stage))
            .collect();
        lines.sort();
        components.insert("transfer_heat".into(), serde_json::json!(lines));
    }
    serde_json::Value::Object(components).to_string()
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
// Output parsing — the crown reply is a bare {reading, score} object under format_schema.
// ---------------------------------------------------------------------------

/// parse_crown_score coerces the emitted score to an integer 1-100. `format_schema` makes it an
/// integer on the live route; the coercions keep offline/no-schema eval tolerant. Clamped 1-100.
fn parse_crown_score(v: &serde_json::Value) -> Option<i32> {
    let n = if let Some(i) = v.as_i64() {
        i
    } else if let Some(f) = v.as_f64() {
        if !f.is_finite() {
            return None;
        }
        f.round() as i64
    } else if let Some(s) = v.as_str() {
        let head = s.split_whitespace().next()?;
        let head = head.split_once('/').map(|(n, _)| n).unwrap_or(head).trim();
        match head.parse::<i64>() {
            Ok(n) => n,
            Err(_) => head.parse::<f64>().ok().filter(|f| f.is_finite())?.round() as i64,
        }
    } else {
        return None;
    };
    Some(n.clamp(1, 100) as i32)
}

/// parse_crown_reply extracts `{reading, score}` from the JSON reply. On the ollama path
/// `format_schema` makes a bare object the only thing the live route emits; the balanced-brace
/// salvage keeps the offline/eval path tolerant of a prose-wrapped object. Reading whitespace is
/// collapsed to one clean paragraph. `None` when there is no non-empty reading or no coercible
/// score (fail-closed → the item backs off).
///
/// Parse strict JSON first; salvage wrapped JSON and literal string control characters.
pub fn parse_crown_reply(raw: &str) -> Option<CrownReply> {
    let trimmed = raw.trim();
    let parsed: Option<serde_json::Value> = serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        let span = escape_string_controls(&trimmed[start..=end]);
        serde_json::from_str(&span).ok()
    });
    let v = parsed?;
    let score = parse_crown_score(v.get("score")?)?;
    let reading = v.get("reading")?.as_str()?.trim();
    let reading = crate::plugins::support::form::normalize_body(reading);
    // Served prose takes the shared scrub.
    let reading = crate::plugins::support::guards::clean_served_prose(&reading);
    if reading.is_empty() {
        return None;
    }
    // Fold the optional title to one line; absence never fails the generation.
    let headline = v
        .get("headline")
        .and_then(|h| h.as_str())
        .map(|h| h.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|h| !h.is_empty());
    Some(CrownReply {
        reading,
        headline,
        score,
    })
}

// Preserve paragraph breaks when an unconstrained backend emits literal newlines in JSON strings.
fn escape_string_controls(span: &str) -> String {
    let mut out = String::with_capacity(span.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in span.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if in_string && ch == '\\' {
            out.push(ch);
            escaped = true;
        } else if ch == '"' {
            out.push(ch);
            in_string = !in_string;
        } else if in_string && ch.is_control() {
            out.push_str(&format!("\\u{:04x}", ch as u32));
        } else {
            out.push(ch);
        }
    }
    out
}

// Re-exported for callers that treat it as part of the Oracle surface.
pub use crate::plugins::support::guards::count_sentences;

/// CrownParser is the crown stage's `Parser` plug-in behind the `Parser<T>` seam. It never returns
/// the fail-closed `Ok(None)` — the crown's only fail-closed path is the pre-model no-pillar marker;
/// an unparseable reply (no reading or no score) is a genuine failure → `Err` → the item backs off.
pub struct CrownParser;

impl Parser<CrownReply> for CrownParser {
    fn parse(&self, raw: &str) -> Result<Option<CrownReply>> {
        match parse_crown_reply(raw) {
            Some(mut r) => {
                crate::plugins::support::form::validate_body(&r.reading)?;
                crate::plugins::support::form::validate_hook(r.headline.as_deref())?;
                // Global served-prose invariants fail closed and retry the item.
                if crate::plugins::support::guards::has_bookkeeping_citation(&r.reading) {
                    tracing::warn!(guard = "bookkeeping_citation", "reading rejected");
                    bail!("crown: reading carries a bookkeeping citation");
                }
                // Optional titles fail open: salvage or drop, never reject the reading.
                r.headline =
                    crate::plugins::support::guards::settle_title("oracle", r.headline.as_deref());
                if let Some(p) = crate::plugins::support::guards::first_product_name(&r.reading) {
                    tracing::warn!(guard = "product_name", name = p, "reading rejected");
                    bail!("crown: reading names product {p:?}");
                }
                if crate::plugins::support::guards::has_foreign_script(&r.reading) {
                    tracing::warn!(guard = "foreign_script", "reading rejected");
                    bail!("crown: reading carries a foreign-script run");
                }
                Ok(Some(r))
            }
            None => bail!(
                "crown: could not parse reading+score from response (raw={:?})",
                truncate(raw, 200)
            ),
        }
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
    let prompt = build_crown_prompt(
        &assignment.subject.entity_type,
        &assignment.subject.entity_name,
        &assignment.subject.sport,
        &cards.narratives,
        cards.rating.as_ref(),
        cards.vibe.as_ref(),
        &cards.momentum,
        &cards.transfers,
        omen,
        assignment.body_cap,
        assignment.identity.as_deref(),
    );
    let extracted = studio
        .extract(&prompt, &assignment.options, &CrownParser)
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("crown: parser returned no value"))?;
    if !crate::plugins::support::guards::title_names_entity(
        &reply.reading,
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
            score: Some(reply.score),
            reading: Some(reply.reading),
            headline: reply.headline,
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

#[cfg(test)]
mod tests;

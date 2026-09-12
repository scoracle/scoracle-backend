//! Rating stage — The Scout's statistical report.
//!
//! Rust owns both rating shapes: the per-entity core here, and a `RatingHandler` queue stage for
//! current-season need-based rating work. `cmd/statcommentary` remains the operator/batch entry
//! point: nightly mode enqueues durable work, while explicit backfill can run the core inline.
//!
//! Postgres owns composite and percentile calculations. Rust selects and labels the evidence,
//! computes notability and trajectory, and asks the model only to narrate decided facts.
//!
//! FAIL CLOSED: rating's ONLY marker is the PRE-model no-stats path (no usable rating row → a
//! NULL-body marker, like vibe's no-corpus marker). There is no post-model fail-closed marker — an
//! empty model body is a hard error (the work fails + retries), never a served row.
//!
//! The labeled tier is authoritative; the model never maps percentile to quality itself.

use crate::harness::{Generation, GenerationCall, Harness, Parser};
use crate::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{hash_components, round1};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

mod inputs;
pub mod prompt;
pub use inputs::{build_stat_prompt, render_availability_reports, render_personnel_block};
pub use prompt::{RATING_PROMPT_VERSION, RATING_SYSTEM_PROMPT};

/// Output contract captured separately in the diagnostic ledger.
pub const RATING_OUTPUT_CONTRACT_VERSION: &str = "rating-commentary-v1";

const RATING_LEDGER: LedgerSpec = LedgerSpec {
    stage: "rating",
    lens: "rating",
    role: Role::StatsLogic,
    product_table: "stat_summaries",
    output_contract_version: RATING_OUTPUT_CONTRACT_VERSION,
};

/// Production rating temperature.
pub const RATING_TEMPERATURE: f64 = 0.6;

/// Token cap for the compact scouting card.
pub const RATING_NUM_PREDICT: i32 = 350;

/// Durable queue version prefix. The entity/sport queue key stays seasonless, so the version
/// carries the season explicitly.
const RATING_WORK_PREFIX: &str = "rating:s";

/// maxStatFacts bounds the breakdown datapoints fed to the prompt.
const MAX_STAT_FACTS: usize = 14;

/// Entity whose rating profile should be narrated. `sport` is already uppercased by the caller.
#[derive(Clone, Debug)]
pub struct RatingReq {
    pub entity_type: String, // "player" | "team"
    pub entity_id: i32,
    pub entity_name: String,
    pub sport: String,       // UPPER ("NBA" | "NFL" | "FOOTBALL")
    pub season: Option<i32>, // None = the entity's latest season
    pub trigger_type: String,
}

/// One `rating_breakdown` datapoint. `pct` is based on `sign*z`, so higher is always better.
/// Explicit JSON nulls default to zero values for sparse datapoints.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RatingDatapoint {
    #[serde(default, deserialize_with = "null_to_default")]
    pub label: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub value: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub z: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub pct: f64,
    #[serde(default, deserialize_with = "null_to_default")]
    pub in_comp: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    pub in_spec: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    pub sign: i32,
    #[serde(default, deserialize_with = "null_to_default")]
    pub facet: String,
    #[serde(default, deserialize_with = "null_tolerant_map")]
    pub scoped_pct: HashMap<String, f64>,
}

/// Map a present JSON null to `T::default`.
fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Map a null `scoped_pct` to empty and null values inside it to zero.
fn null_tolerant_map<'de, D>(d: D) -> Result<HashMap<String, f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<HashMap<String, Option<f64>>> = Option::deserialize(d)?;
    Ok(opt
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, v.unwrap_or(0.0)))
        .collect())
}

/// Scrubbed rating profile. `composite_score` comes from a numeric column cast to float8; the
/// breakdown/scoped/modes are JSONB. The breakdown's ARRAY ORDER is preserved (jsonb keeps array
/// order), which `input_components` relies on while the prompt sorts by percentile.
#[derive(Clone, Debug)]
pub struct RatingProfile {
    pub entity_type: String,
    pub season: i32,
    pub position: String, // players only ("" for teams)
    pub composite_score: Option<f64>,
    pub breakdown: Vec<RatingDatapoint>,
    pub scoped_ranks: HashMap<String, f64>,
    pub rate_modes: HashMap<String, Vec<RatingDatapoint>>,
}

/// A per-x (per_36 / per_90 / …) standout — an elite rate-adjusted datapoint. Mirrors `rateStandout`.
#[derive(Clone, Debug)]
pub struct RateStandout {
    pub mode: String,
    pub label: String,
    pub pct: f64,
}

#[derive(Clone, Debug)]
pub struct RatingTrajectory {
    pub key: String,
    pub label: Option<String>,
    pub components: serde_json::Value,
}

impl RatingTrajectory {
    fn steady(reason: &str) -> Self {
        Self {
            key: "steady".to_string(),
            label: None,
            components: serde_json::json!({ "reason": reason }),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RatingExclusions {
    pub budget_truncated_stat_labels: Vec<String>,
    /// Breakdown labels dropped because their facet is the other side of the ball from the
    /// player's position (NFL only — see `drop_off_facet_datapoints`).
    pub off_facet_stat_labels: Vec<String>,
    /// Zero-value, near-average-z usage artifacts (see `drop_degenerate_zero_datapoints`).
    pub degenerate_zero_stat_labels: Vec<String>,
    /// Display-tier datapoints — retired from the rating equation (`in_comp=false AND
    /// in_spec=false`) — excluded from the AI context (see `drop_display_tier_datapoints`).
    pub display_tier_stat_labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutingDecisionFact {
    pub label: String,
    pub evidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutingDecision {
    pub primary_strength_to_stop: Option<ScoutingDecisionFact>,
    pub secondary_strengths: Vec<ScoutingDecisionFact>,
    pub primary_weakness_to_exploit: Option<ScoutingDecisionFact>,
    pub no_standout_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Loader.
// ---------------------------------------------------------------------------

/// load_rating_profile reads the entity's rating row for `season` (None = latest). Prefers the
/// unscoped row, falling back to the richest league row (FOOTBALL is league-scoped). Numeric scores
/// are cast to float8 and JSONB to text for sqlx/serde decoding.
/// Returns `None` when there is no rating row at all.
pub async fn load_rating_profile(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<RatingProfile>> {
    // Rate modes exist only for player/minutes stats; teams receive an empty object.
    let (id_col, table, pos_select, modes_select) = match entity_type {
        "player" => (
            "player_id",
            "player_stats",
            "COALESCE(position, '')",
            "COALESCE(rating_modes, '{}'::jsonb)::text",
        ),
        "team" => ("team_id", "team_stats", "''::text", "'{}'::text"),
        _ => bail!("unknown entity type {entity_type:?}"),
    };
    // The unscoped row first (NBA/NFL carry league_id 0/NULL), else the richest league row (the
    // most-datapoints row is the main competition — domestic league over a cup).
    let q = format!(
        r#"
        SELECT season, {pos_select},
               rating_score::float8,
               COALESCE(rating_breakdown, '[]'::jsonb)::text,
               COALESCE(rating_scoped_ranks, '{{}}'::jsonb)::text,
               {modes_select}
        FROM public.{table}
        WHERE sport = $1 AND {id_col} = $2 AND ($3::int IS NULL OR season = $3)
        ORDER BY season DESC,
                 (COALESCE(league_id, 0) = 0) DESC,
                 jsonb_array_length(COALESCE(rating_breakdown, '[]'::jsonb)) DESC,
                 COALESCE(league_id, 0) ASC
        LIMIT 1
        "#
    );
    let Some(row) = sqlx::query(&q)
        .bind(sport)
        .bind(entity_id)
        .bind(season)
        .fetch_optional(pool)
        .await
        .context("load rating profile")?
    else {
        return Ok(None);
    };

    let season: i32 = row.get(0);
    let position: String = row.get(1);
    let composite_score: Option<f64> = row.get(2);
    let breakdown_raw: String = row.get(3);
    let scoped_raw: String = row.get(4);
    let modes_raw: String = row.get(5);

    let breakdown: Vec<RatingDatapoint> =
        serde_json::from_str(&breakdown_raw).context("unmarshal rating_breakdown")?;
    // Cohort framing and per-x modes are optional enrichment.
    let scoped_ranks: HashMap<String, f64> = serde_json::from_str(&scoped_raw).unwrap_or_default();
    let rate_modes = parse_rate_modes(&modes_raw);

    Ok(Some(RatingProfile {
        entity_type: entity_type.to_string(),
        season,
        position,
        composite_score,
        breakdown,
        scoped_ranks,
        rate_modes,
    }))
}

/// parse_rate_modes reads `rating_modes` — a per-x bundle per mode (`{"per_36": {"breakdown": [...]}}`),
/// keeping only non-empty breakdowns. A parse error yields no optional modes.
fn parse_rate_modes(raw: &str) -> HashMap<String, Vec<RatingDatapoint>> {
    #[derive(Deserialize)]
    struct ModeWrap {
        #[serde(default)]
        breakdown: Vec<RatingDatapoint>,
    }
    let parsed: HashMap<String, ModeWrap> = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };
    parsed
        .into_iter()
        .filter(|(_, m)| !m.breakdown.is_empty())
        .map(|(name, m)| (name, m.breakdown))
        .collect()
}

// ---------------------------------------------------------------------------
// Deterministic prompt shaping over stored derived stats.
// ---------------------------------------------------------------------------

/// compute_notability returns the deterministic distinctiveness score (0-100) + its components. The
/// model NEVER sees the formula — only the resulting length guidance (rendered into the prompt, so it
/// reaches the prompt). Order-independent because the rate-mode loop only takes a maximum.
pub fn compute_notability(p: &RatingProfile) -> (i32, serde_json::Value) {
    let mut top_pct = 0.0_f64;
    let mut elite_count = 0_i64;
    for d in &p.breakdown {
        if d.pct > top_pct {
            top_pct = d.pct;
        }
        if d.pct >= 85.0 {
            elite_count += 1;
        }
    }
    // The per-x lens counts toward the top percentile (an elite-per-36 limited-minutes player
    // earns a fuller read) but NOT toward elite_count (avoid double-counting one skill across modes).
    for dps in p.rate_modes.values() {
        for d in dps {
            if d.pct > top_pct {
                top_pct = d.pct;
            }
        }
    }
    let comp = p.composite_score.unwrap_or(50.0); // average T-score anchor when no composite
    let score = 0.6 * top_pct
        + (elite_count as f64 * 10.0).min(30.0)
        + clamp_f(-10.0, 10.0, (comp - 50.0) * 0.4);
    let n = clamp_f(0.0, 100.0, score).round() as i32;
    let comps = serde_json::json!({
        // key renamed from "peak_pct" at s19 (PEAK retirement); formula unchanged.
        "top_pct": round1(top_pct),
        "elite_count": elite_count,
        "composite": round1(comp),
    });
    (n, comps)
}

/// pct_band maps a percentile to its quality TIER — the L8 breakthrough done in code so the model
/// never maps percentile→quality itself (it just verbalizes the labeled tier). Transient
/// prompt-shaping (like sigil's trendDir), NOT a stored derived stat. Mirrors `pctBand`.
pub fn pct_band(pct: f64) -> &'static str {
    if pct >= 90.0 {
        "elite"
    } else if pct >= 75.0 {
        "strong"
    } else if pct >= 60.0 {
        "above average"
    } else if pct >= 50.0 {
        "average"
    } else if pct >= 35.0 {
        "below average"
    } else {
        "poor"
    }
}

/// trim_float renders a datapoint value compactly — integers without a decimal, small fractions
/// (< 1) with two places, everything else with one ("3" / "0.38" / "10.7").
fn trim_float(f: f64) -> String {
    if f == f.trunc() {
        format!("{f:.0}")
    } else if f.abs() < 1.0 {
        format!("{f:.2}")
    } else {
        format!("{f:.1}")
    }
}

/// Return at most `MAX_STAT_FACTS` spanning the full percentile range. Keep both ends and sample
/// the middle evenly so the prompt does not become a top-N highlight reel.
fn ordered_facts(breakdown: &[RatingDatapoint]) -> Vec<RatingDatapoint> {
    let facts = ordered_facts_unbounded(breakdown);
    if facts.len() <= MAX_STAT_FACTS {
        return facts;
    }
    const ENDS: usize = 5; // the top and bottom five: elite edges and real liabilities
    let middle_slots = MAX_STAT_FACTS - (ENDS * 2);
    let mut keep: Vec<usize> = (0..ENDS).collect();
    // Even stride across the interior, endpoints excluded (they are already taken).
    let lo = ENDS;
    let hi = facts.len() - ENDS;
    if hi > lo && middle_slots > 0 {
        let span = hi - lo;
        for i in 0..middle_slots {
            // +1/(middle_slots+1) spacing keeps the samples off both seams.
            let idx = lo + ((i + 1) * span) / (middle_slots + 1);
            if !keep.contains(&idx) {
                keep.push(idx);
            }
        }
    }
    keep.extend((facts.len() - ENDS)..facts.len());
    keep.sort_unstable();
    keep.dedup();
    keep.into_iter().map(|i| facts[i].clone()).collect()
}

fn ordered_facts_unbounded(breakdown: &[RatingDatapoint]) -> Vec<RatingDatapoint> {
    let mut facts = breakdown.to_vec();
    facts.sort_by(|a, b| {
        b.pct
            .partial_cmp(&a.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    facts
}

fn budget_truncated_stat_labels(
    breakdown: &[RatingDatapoint],
    decision: &ScoutingDecision,
) -> Vec<String> {
    let mut decision_labels = HashSet::new();
    if let Some(f) = &decision.primary_strength_to_stop {
        decision_labels.insert(f.label.as_str());
    }
    for f in &decision.secondary_strengths {
        decision_labels.insert(f.label.as_str());
    }
    if let Some(f) = &decision.primary_weakness_to_exploit {
        decision_labels.insert(f.label.as_str());
    }

    let mut facts = breakdown.to_vec();
    facts.sort_by(|a, b| {
        b.pct
            .partial_cmp(&a.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    facts
        .into_iter()
        .skip(MAX_STAT_FACTS)
        .filter(|d| !decision_labels.contains(d.label.as_str()))
        .map(|d| d.label)
        .collect()
}

/// collect_rate_standouts surfaces, per rate mode, the elite (pct ≥ 80) per-x datapoints — the lens
/// that reveals a limited-minutes player producing at an elite rate. Modes sorted for stable output
/// with byte-wise string ordering; at most five per mode.
/// Used by BOTH the prompt's rate-adjusted section AND `input_components`' rate_standouts (same output).
fn collect_rate_standouts(p: &RatingProfile) -> Vec<RateStandout> {
    let mut modes: Vec<&String> = p.rate_modes.keys().collect();
    modes.sort();

    let mut out = Vec::new();
    for m in modes {
        let mut dps = p.rate_modes[m].clone();
        dps.sort_by(|a, b| {
            b.pct
                .partial_cmp(&a.pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut cnt = 0;
        for d in &dps {
            if d.pct < 80.0 {
                break;
            }
            out.push(RateStandout {
                mode: m.clone(),
                label: d.label.clone(),
                pct: d.pct,
            });
            cnt += 1;
            if cnt >= 5 {
                break;
            }
        }
    }
    out
}

fn is_strong_or_elite(d: &RatingDatapoint) -> bool {
    d.pct >= 75.0
}

/// signed_z is the sign-adjusted z — the one number where "+" is always the good direction
/// (`format_datapoint_evidence` renders the same value).
fn signed_z(d: &RatingDatapoint) -> f64 {
    d.sign as f64 * d.z
}

/// A named weakness must be MATERIALLY bad, not merely low-percentile. Distributions that clump
/// at zero (giveaways, ground yards for a WR) map tiny raw differences onto extreme percentiles:
/// Drake London's 1 giveaway sat at the 5th percentile with a sign-adjusted z of just -0.2 —
/// statistically "poor", practically average — while Stafford's genuine giveaway problem carried
/// z -4.9. Percentile finds the candidate; z-magnitude confirms it is real.
fn is_weakness(d: &RatingDatapoint) -> bool {
    d.pct < 50.0 && signed_z(d) <= -0.5
}

/// nfl_position_side maps an NFL position to the side of the ball it plays. Both the
/// abbreviated and spelled-out forms appear in `player_stats.position` ("QB" and
/// "Quarterback"). `None` — kickers/punters/returners/long snappers, "Unknown", and the
/// teams' empty string — means no side can be inferred, and the off-facet filter fails
/// open (keeps everything), matching the harness's fail-closed-by-omission posture.
fn nfl_position_side(position: &str) -> Option<&'static str> {
    match position.trim().to_lowercase().as_str() {
        "qb" | "quarterback" | "rb" | "running back" | "fb" | "fullback" | "wr"
        | "wide receiver" | "te" | "tight end" | "c" | "center" | "g" | "guard" | "ot"
        | "offensive tackle" => Some("offense"),
        "cb" | "cornerback" | "s" | "safety" | "lb" | "linebacker" | "de" | "defensive end"
        | "dt" | "defensive tackle" => Some("defense"),
        _ => None,
    }
}

/// drop_degenerate_zero_datapoints removes datapoints that are a zero VALUE with a near-average
/// sign-adjusted z (|z| < 0.5): the entity simply does not do this thing, and not doing it
/// barely moves the needle — a usage artifact, not scoutable evidence (a WR's "Ground Yards
/// Responsible: 0 · 1st pct" is not a liability). A zero with a STRONGLY negative z stays: that
/// is a real absence (a starting QB with zero touchdowns is a finding, not an artifact).
/// Returns the dropped labels for the exclusions ledger.
fn drop_degenerate_zero_datapoints(p: &mut RatingProfile) -> Vec<String> {
    let degenerate = |d: &RatingDatapoint| d.value == 0.0 && signed_z(d).abs() < 0.5;
    let dropped: Vec<String> = p
        .breakdown
        .iter()
        .filter(|d| degenerate(d))
        .map(|d| d.label.clone())
        .collect();
    p.breakdown.retain(|d| !degenerate(d));
    for dps in p.rate_modes.values_mut() {
        dps.retain(|d| !degenerate(d));
    }
    dropped
}

/// drop_off_facet_datapoints removes breakdown and rate-mode datapoints from the OTHER side
/// of the ball than the player's position. An offensive player's defensive stat sheet (and
/// vice versa) is structural noise, not scoutable evidence: a QB's 0th-percentile Tackling is
/// not a "primary weakness to exploit", it is a category he does not play. Only NFL breakdowns
/// carry offense/defense facets (NBA and FOOTBALL emit facet="all"), so this no-ops for every
/// other sport, for teams, and for facet-less rows by construction. Returns the dropped
/// breakdown labels for the exclusions ledger — the selection is provable, not silent.
fn drop_off_facet_datapoints(p: &mut RatingProfile) -> Vec<String> {
    let Some(side) = nfl_position_side(&p.position) else {
        return Vec::new();
    };
    let off_facet =
        |d: &RatingDatapoint| (d.facet == "offense" || d.facet == "defense") && d.facet != side;
    let dropped: Vec<String> = p
        .breakdown
        .iter()
        .filter(|d| off_facet(d))
        .map(|d| d.label.clone())
        .collect();
    p.breakdown.retain(|d| !off_facet(d));
    for dps in p.rate_modes.values_mut() {
        dps.retain(|d| !off_facet(d));
    }
    dropped
}

/// Remove display-only datapoints (`!in_comp && !in_spec`) from the breakdown and rate modes.
/// Return dropped labels for provenance.
fn drop_display_tier_datapoints(p: &mut RatingProfile) -> Vec<String> {
    let display_tier = |d: &RatingDatapoint| !d.in_comp && !d.in_spec;
    let dropped: Vec<String> = p
        .breakdown
        .iter()
        .filter(|d| display_tier(d))
        .map(|d| d.label.clone())
        .collect();
    p.breakdown.retain(|d| !display_tier(d));
    for dps in p.rate_modes.values_mut() {
        dps.retain(|d| !display_tier(d));
    }
    dropped
}

fn format_datapoint_evidence(d: &RatingDatapoint) -> String {
    // User-facing evidence calls the standardized value a rating, never a z-score.
    // Sign-adjust so positive always means good.
    let dz = d.sign as f64 * d.z;
    // Commas keep copied evidence grammatical; the interpunct remains a prose tripwire.
    let mut s = format!(
        "{}: {}, {:.0}th pct ({}), rating {:+.1}",
        d.label,
        trim_float(d.value),
        d.pct,
        pct_band(d.pct),
        dz
    );
    if let Some(pos) = d.scoped_pct.get("position") {
        s.push_str(&format!(" [position: {:.0}th, {}]", pos, pct_band(*pos)));
    }
    s
}

fn decision_fact(d: &RatingDatapoint) -> ScoutingDecisionFact {
    ScoutingDecisionFact {
        label: d.label.clone(),
        evidence: format_datapoint_evidence(d),
    }
}

pub fn build_scouting_decision(p: &RatingProfile) -> ScoutingDecision {
    const MAX_SECONDARY_STRENGTHS: usize = 5;

    let facts = ordered_facts_unbounded(&p.breakdown);
    let primary = facts.first().filter(|d| is_strong_or_elite(d));

    let mut primary_strength_to_stop = primary.map(decision_fact);
    // Put per-rate corroboration on the primary strength rather than in a separate section.
    if let Some(f) = primary_strength_to_stop.as_mut() {
        if let Some(r) = collect_rate_standouts(p)
            .iter()
            .find(|r| r.label == f.label)
        {
            f.evidence.push_str(&format!(
                " (corroborated {}: {:.0}th pct — the edge is real, not a minutes artifact)",
                r.mode.replace('_', "-"),
                r.pct
            ));
        }
    }
    let secondary_strengths = facts
        .iter()
        .skip(1)
        .filter(|d| is_strong_or_elite(d))
        .take(MAX_SECONDARY_STRENGTHS)
        .map(decision_fact)
        .collect();

    let primary_weakness_to_exploit = p
        .breakdown
        .iter()
        .filter(|d| is_weakness(d))
        .min_by(|a, b| {
            a.pct
                .partial_cmp(&b.pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(decision_fact);

    let no_standout_reason = if primary.is_none() {
        Some(match facts.first() {
            Some(d) => format!(
                "Highest datapoint is {}; {} is not strong/elite, so no strong/elite datapoint exists.",
                format_datapoint_evidence(d),
                pct_band(d.pct)
            ),
            None => "No skill datapoint is available, so no strong/elite datapoint exists."
                .to_string(),
        })
    } else {
        None
    };

    ScoutingDecision {
        primary_strength_to_stop,
        secondary_strengths,
        primary_weakness_to_exploit,
        no_standout_reason,
    }
}

/// Fetch the cross-season stats memory card: prior-season skill read, confirmed moves, and
/// reliability-framed matchup edges.
/// `None` = no memory, no prompt section. Model-facing enrichment only — the relational
/// layer is never user-exposed.
pub async fn load_stat_memory(
    pool: &PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
    season: i32,
) -> Result<Option<String>> {
    let row: (Option<String>,) = sqlx::query_as("SELECT stat_context_for_entity($1, $2, $3, $4)")
        .bind(sport)
        .bind(entity_type)
        .bind(entity_id)
        .bind(season)
        .fetch_one(pool)
        .await
        .context("stat_context_for_entity")?;
    Ok(row.0)
}

/// Per-skill percentile movement needed for "improved" or "slipped".
const Z_MEMORY_MOVE_PCT_POINTS: f64 = 8.0;
/// Cap on movement lines rendered into the prompt (top by current pct — the A5 rule does not
/// apply: unmatched skills are new-season datapoints, not dropped evidence).
const Z_MEMORY_MAX_LINES: usize = 10;

/// Render season-over-season movement for matching skill labels, carrying both percentiles,
/// both tiers, and a DECIDED movement word — the L8/ScoutingDecision discipline applied to
/// trajectory (the model voices a decided move, it never infers direction from raw numbers).
/// Pure for testability; `None` when no skill matches across seasons.
pub fn build_z_memory_lines(current: &RatingProfile, prior: &RatingProfile) -> Option<String> {
    let prior_by_label: HashMap<&str, f64> = prior
        .breakdown
        .iter()
        .map(|d| (d.label.as_str(), d.pct))
        .collect();
    let mut facts: Vec<(&RatingDatapoint, f64)> = current
        .breakdown
        .iter()
        .filter_map(|d| prior_by_label.get(d.label.as_str()).map(|p| (d, *p)))
        .collect();
    facts.sort_by(|a, b| {
        b.0.pct
            .partial_cmp(&a.0.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    facts.truncate(Z_MEMORY_MAX_LINES);
    if facts.is_empty() {
        return None;
    }
    let mut out = String::new();
    for (d, prior_pct) in facts {
        let delta = d.pct - prior_pct;
        let movement = if delta >= Z_MEMORY_MOVE_PCT_POINTS {
            "improved"
        } else if delta <= -Z_MEMORY_MOVE_PCT_POINTS {
            "slipped"
        } else {
            "held"
        };
        out.push_str(&format!(
            "{}: {:.0}th pct ({}) — last season {:.0}th ({}); {}\n",
            d.label,
            d.pct,
            pct_band(d.pct),
            prior_pct,
            pct_band(prior_pct),
            movement
        ));
    }
    Some(out)
}

/// One adjudicated availability event, as the DB describes it — the injury/suspension half of
/// the personnel record (mig 229), alongside [`PersonnelChange`]'s transfers.
///
/// A SEPARATE struct from `PersonnelChange` on purpose, and this is the same judgement mig 229
/// made in the schema: a transfer is a MOVE (one club to another) and an availability event is a
/// SPAN (out, then back, or the record withdrawn). Folding a span into the move shape is what
/// makes a retracted false report and a genuine three-week absence indistinguishable — the exact
/// corruption `returned_at` and `reverted_at` exist as separate columns to prevent. They render
/// into one "since our last read" block because that is what the Scout needs to see; they are
/// two fact shapes in code because that is what they are.
///
/// **T4 holds by construction.** Every field is a date, an id resolved to a name, or one of the
/// two enums. `revert_reason` is prose and is deliberately never selected; `body_part` ships
/// empty and is never guessed, so it is not read here either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailabilityChange {
    /// `opened` — newly ruled out; `returned` — availability resumed (a real-world outcome);
    /// `reverted` — the RECORD was wrong and has been withdrawn (a correction, never a return).
    pub kind: String,
    /// When the thing that is NEW happened — the apply, the return, or the withdrawal.
    pub date_label: String,
    /// `injury` or `suspension`, the adjudicated enum. Never model prose.
    pub event_kind: String,
    pub player_name: String,
    /// The club the player was at when it happened; `None` when unattached or unresolved.
    pub team_name: Option<String>,
    pub team_id: Option<i32>,
    /// The day the player became unavailable — carried even on a return, because "out Aug 20,
    /// back Aug 30" is the fact, not "back Aug 30".
    pub event_date_label: String,
    /// The prognosis AS REPORTED. Renderable; never ground truth (mig 229).
    pub expected_return_label: Option<String>,
}

/// How many availability lines render before the block starts naming drops instead.
///
/// Four, against personnel's six, and the two budgets are deliberately separate but summed
/// against the same ceiling: the rating prompt lives inside one 4,096-token window, and a
/// deadline-day squad churn plus a treatment-table update must not between them crowd out the
/// datapoints the report is actually built on.
const MAX_AVAILABILITY_LINES: usize = 4;

/// One adjudicated personnel change, as the DB describes it — dates already labeled by
/// `to_char` (the `Mon DD` convention the memory card and 7.10's storyline lens use), names
/// resolved, nothing rendered. The sentence is built in code (T2: describe, then derive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonnelChange {
    /// `applied` — the move is in force; `reverted` — an earlier applied move was undone.
    pub kind: String,
    pub date_label: String,
    /// The adjudicated event label (`transfer`, `rumor`, …). Never model prose — it is the
    /// Insider's structured `event_type` column.
    pub event_type: Option<String>,
    pub player_name: String,
    pub old_team: Option<String>,
    pub new_team: Option<String>,
    /// Carried so a TEAM read can tell an arrival from a departure by id rather than by
    /// comparing rendered names, which collide across leagues.
    pub old_team_id: Option<i32>,
    pub new_team_id: Option<i32>,
}

/// How many personnel lines the block renders before it starts naming drops instead. Six is
/// ~140 tokens — a deadline-day squad churn cannot crowd out the datapoints inside 4,096.
const MAX_PERSONNEL_LINES: usize = 6;
/// The lookback when this entity has never been read: a first brief still deserves recent
/// personnel facts, but not a year of them.
const PERSONNEL_FIRST_READ_DAYS: i32 = 30;
/// The hard ceiling on the lookback however stale the last read is — an entity nobody has
/// scouted since preseason gets the recent moves, not its whole transfer history.
const PERSONNEL_MAX_DAYS: i32 = 180;

/// Load adjudicated transfers since the entity's last read. Unlike slow memory, this includes
/// departures, source clubs, and reverts. Only structured facts reach the Scout. Returns the
/// newest rows plus the pre-cap total so exclusions are explicit.
pub async fn load_personnel_changes(
    pool: &PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<(Vec<PersonnelChange>, usize)> {
    if entity_type != "player" && entity_type != "team" {
        return Ok((Vec::new(), 0));
    }
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<i32>,
        Option<i32>,
    )> = sqlx::query_as(
        r#"
        WITH since AS (
            SELECT greatest(
                       COALESCE(
                           (SELECT max(s.generated_at) FROM public.stat_summaries s
                             WHERE s.entity_type = $2 AND s.entity_id = $3 AND s.sport = $1
                               AND s.body IS NOT NULL),
                           now() - make_interval(days => $4)),
                       now() - make_interval(days => $5)) AS at
        ),
        changes AS (
            SELECT 'applied'::text AS kind, a.applied_at AS at, a.event_type,
                   a.player_id, a.old_team_id, a.new_team_id
              FROM public.transfer_identity_applications a
             WHERE a.sport = $1 AND a.status = 'applied' AND a.reverted_at IS NULL
               AND a.applied_at IS NOT NULL AND a.applied_at > (SELECT at FROM since)
            UNION ALL
            -- A revert is dated by WHEN IT WAS UNDONE: that is the fact that is new since the
            -- last read, whatever the original move's date was.
            SELECT 'reverted'::text, a.reverted_at, a.event_type,
                   a.player_id, a.old_team_id, a.new_team_id
              FROM public.transfer_identity_applications a
             WHERE a.sport = $1 AND a.reverted_at IS NOT NULL
               AND a.reverted_at > (SELECT at FROM since)
        )
        SELECT c.kind,
               to_char(c.at, 'Mon DD') AS date_label,
               c.event_type,
               COALESCE(pl.name, 'a player') AS player_name,
               told.name AS old_team,
               tnew.name AS new_team,
               c.old_team_id,
               c.new_team_id
          FROM changes c
          JOIN public.players pl ON pl.id = c.player_id AND pl.sport = $1
          LEFT JOIN public.teams told ON told.id = c.old_team_id AND told.sport = $1
          LEFT JOIN public.teams tnew ON tnew.id = c.new_team_id AND tnew.sport = $1
         WHERE ($2 = 'player' AND c.player_id = $3)
            OR ($2 = 'team' AND ($3 = c.new_team_id OR $3 = c.old_team_id))
         ORDER BY c.at DESC
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(PERSONNEL_FIRST_READ_DAYS)
    .bind(PERSONNEL_MAX_DAYS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load personnel changes {entity_type}/{entity_id}"))?;

    let total = rows.len();
    let changes = rows
        .into_iter()
        .take(MAX_PERSONNEL_LINES)
        .map(
            |(
                kind,
                date_label,
                event_type,
                player_name,
                old_team,
                new_team,
                old_team_id,
                new_team_id,
            )| PersonnelChange {
                kind,
                date_label,
                event_type,
                player_name,
                old_team,
                new_team,
                old_team_id,
                new_team_id,
            },
        )
        .collect();
    Ok((changes, total))
}

/// Load adjudicated availability changes since the last read. Three distinct kinds are retained:
/// `opened` (newly ruled out),
/// `returned` (`returned_at` — availability actually resumed, a real-world outcome), and
/// `reverted` (`reverted_at` — the RECORD was wrong, a correction). Rendering a revert as a
/// return would tell the Scout a player is fit when what actually happened is that we withdrew
/// the claim that he was ever hurt.
///
/// The `since` window is the personnel window exactly — same clamp, same first-read floor — so
/// the two halves of one block cannot disagree about what "since our last read" means.
///
/// Returns newest-first plus the TOTAL that qualified, so the renderer names what the cap
/// dropped (the A5 rule) instead of silently truncating.
pub async fn load_availability_changes(
    pool: &PgPool,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<(Vec<AvailabilityChange>, usize)> {
    if entity_type != "player" && entity_type != "team" {
        return Ok((Vec::new(), 0));
    }
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<i32>,
        String,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        WITH since AS (
            SELECT greatest(
                       COALESCE(
                           (SELECT max(s.generated_at) FROM public.stat_summaries s
                             WHERE s.entity_type = $2 AND s.entity_id = $3 AND s.sport = $1
                               AND s.body IS NOT NULL),
                           now() - make_interval(days => $4)),
                       now() - make_interval(days => $5)) AS at
        ),
        changes AS (
            -- Newly ruled out. Dated by the APPLY, not the event: an injury adjudicated today
            -- for a knock last Saturday is new information today.
            SELECT 'opened'::text AS kind, a.applied_at AS at, a.kind AS event_kind,
                   a.player_id, a.team_id, a.event_date, a.expected_return
              FROM public.player_availability a
             WHERE a.sport = $1 AND a.status = 'applied' AND a.reverted_at IS NULL
               AND a.applied_at IS NOT NULL AND a.applied_at > (SELECT at FROM since)
            UNION ALL
            -- Came back. A real-world outcome, and the propensity denominator.
            SELECT 'returned', a.returned_at::timestamptz, a.kind,
                   a.player_id, a.team_id, a.event_date, a.expected_return
              FROM public.player_availability a
             WHERE a.sport = $1 AND a.status = 'applied' AND a.reverted_at IS NULL
               AND a.returned_at IS NOT NULL
               AND a.returned_at > (SELECT at FROM since)::date
            UNION ALL
            -- The record was withdrawn. Dated by WHEN IT WAS UNDONE — that is what is new,
            -- whatever the original event's date was (the personnel read's own convention).
            SELECT 'reverted', a.reverted_at, a.kind,
                   a.player_id, a.team_id, a.event_date, a.expected_return
              FROM public.player_availability a
             WHERE a.sport = $1 AND a.reverted_at IS NOT NULL
               AND a.reverted_at > (SELECT at FROM since)
        )
        SELECT c.kind,
               to_char(c.at, 'Mon DD') AS date_label,
               c.event_kind,
               COALESCE(pl.name, 'a player') AS player_name,
               t.name AS team_name,
               c.team_id,
               to_char(c.event_date, 'Mon DD') AS event_date_label,
               CASE WHEN c.expected_return IS NOT NULL
                    THEN to_char(c.expected_return, 'Mon DD') END AS expected_return_label
          FROM changes c
          JOIN public.players pl ON pl.id = c.player_id AND pl.sport = $1
          LEFT JOIN public.teams t ON t.id = c.team_id AND t.sport = $1
         WHERE ($2 = 'player' AND c.player_id = $3)
            OR ($2 = 'team' AND c.team_id = $3)
         ORDER BY c.at DESC
        "#,
    )
    .bind(sport)
    .bind(entity_type)
    .bind(entity_id)
    .bind(PERSONNEL_FIRST_READ_DAYS)
    .bind(PERSONNEL_MAX_DAYS)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load availability changes {entity_type}/{entity_id}"))?;

    let total = rows.len();
    let changes = rows
        .into_iter()
        .take(MAX_AVAILABILITY_LINES)
        .map(
            |(
                kind,
                date_label,
                event_kind,
                player_name,
                team_name,
                team_id,
                event_date_label,
                expected_return_label,
            )| AvailabilityChange {
                kind,
                date_label,
                event_kind,
                player_name,
                team_name,
                team_id,
                event_date_label,
                expected_return_label,
            },
        )
        .collect();
    Ok((changes, total))
}

/// How many reported-availability claims reach the brief. Six, matching the personnel cap: the
/// 4,096 window still binds, and a busy treatment table must not crowd out the datapoints the
/// report is actually built on.
const MAX_AVAILABILITY_CLAIMS: usize = 6;

/// Load the Editor's injury/suspension claims for this entity — evidence the Scout weighs rather
/// than adjudicated facts it simply reports.
///
/// `Voice::Scout` selects only injury and suspension claims. `mark_contested` identifies both
/// sides of a contradiction without filtering or deciding it.
pub async fn load_availability_reports(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<crate::junctions::editor::render::MarkedClaim>> {
    use crate::junctions::editor::render::{mark_contested, slice_claims, Voice};

    if entity_type != "player" && entity_type != "team" {
        return Ok(Vec::new());
    }
    let loaded = crate::junctions::editor::packet::load_packets_for_entity(
        pool,
        entity_type,
        entity_id,
        sport,
        crate::junctions::journalist::PACKET_LOOKBACK_HOURS,
        MAX_AVAILABILITY_CLAIMS as i64,
    )
    .await
    .with_context(|| format!("load availability reports {entity_type}/{entity_id}"))?;

    let mut claims = Vec::new();
    for (view, _) in loaded {
        claims.extend(slice_claims(&view.claims, Voice::Scout));
    }
    claims.truncate(MAX_AVAILABILITY_CLAIMS);
    // Contest-marking runs across the WHOLE set, after the merge — two storylines reporting the
    // same knock differently is precisely the pair worth marking, and marking per-packet would
    // miss it.
    Ok(mark_contested(&claims))
}

// ---------------------------------------------------------------------------
// Canonical material-input JSON and debounce hash. Datapoints retain stored order.
// ---------------------------------------------------------------------------

/// Return canonical input-components JSON; its SHA-256 is `input_hash`.
/// `season`/`datapoints` are ALWAYS present; the rest (rate_standouts, composite_score, position)
/// are conditional. Percentiles are rounded to one decimal.
pub fn input_components(p: &RatingProfile) -> String {
    let datapoints: Vec<serde_json::Value> = p
        .breakdown
        .iter()
        .map(|d| serde_json::json!({"label": d.label, "pct": round1(d.pct)}))
        .collect();

    let mut components = serde_json::Map::new();
    components.insert("datapoints".into(), serde_json::json!(datapoints));
    components.insert(
        "prompt_version".into(),
        serde_json::json!(RATING_PROMPT_VERSION),
    );
    components.insert("season".into(), serde_json::json!(p.season));

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        let rates: Vec<serde_json::Value> = rs
            .iter()
            .map(|r| serde_json::json!({"label": r.label, "mode": r.mode, "pct": round1(r.pct)}))
            .collect();
        components.insert("rate_standouts".into(), serde_json::json!(rates));
    }
    if let Some(c) = p.composite_score {
        components.insert("composite_score".into(), serde_json::json!(round1(c)));
    }
    if !p.position.is_empty() {
        components.insert("position".into(), serde_json::json!(p.position));
    }
    serde_json::Value::Object(components).to_string()
}

fn clamp_f(lo: f64, hi: f64, v: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// linear_slope — mean-centered OLS slope over [0..N-1] → values.
///
/// DO NOT merge with `sigil::linear_slope` — different accumulation order (this mean-centered
/// form vs sigil's sum form), mathematically equivalent but not FP-bit-identical. See plan A6 /
/// E3: this slope's `round1`'d output feeds rating's `input_components` JSON → the `input_hash`
/// debounce, so a changed accumulation could flip boundary values and cause spurious regens.
fn linear_slope(vals: &[f64]) -> f64 {
    let n = vals.len();
    if n < 2 {
        return 0.0;
    }
    let n_f = n as f64;
    let mean_x = (n_f - 1.0) / 2.0;
    let mean_y = vals.iter().sum::<f64>() / n_f;
    let mut num = 0.0;
    let mut den = 0.0;
    for (i, y) in vals.iter().enumerate() {
        let dx = i as f64 - mean_x;
        num += dx * (y - mean_y);
        den += dx * dx;
    }
    if den.abs() < 1e-9 {
        0.0
    } else {
        num / den
    }
}

fn trajectory_key(slope: f64) -> &'static str {
    if slope > 0.25 {
        "rising"
    } else if slope < -0.25 {
        "falling"
    } else {
        "steady"
    }
}

/// User-facing trajectory label shared by Scout and Analyst prompts.
fn z_trajectory_label(key: &str) -> String {
    match key {
        "rising" => "overall scores trending up over recent games".to_string(),
        "falling" => "overall scores trending down over recent games".to_string(),
        _ => "overall scores holding steady over recent games".to_string(),
    }
}

fn rounded_series(vals: &[f64]) -> Vec<f64> {
    vals.iter().copied().map(round1).collect()
}

/// Fraction of scored events in the trajectory window, clamped to the minimum sample size and a
/// recent upper bound.
const TRAJECTORY_WINDOW_PCT: f64 = 0.10;
const TRAJECTORY_WINDOW_MIN: i64 = 3;
const TRAJECTORY_WINDOW_MAX: i64 = 16;

async fn load_rating_trajectory(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    profile: &RatingProfile,
) -> Result<RatingTrajectory> {
    let (table, id_col) = match entity_type {
        "player" => ("event_box_scores", "player_id"),
        "team" => ("event_team_stats", "team_id"),
        _ => return Ok(RatingTrajectory::steady("unknown_entity_type")),
    };

    // Scale "recent" to the entity's number of scored events this season.
    let count_q = format!(
        r#"
        SELECT COUNT(*)
        FROM public.{table} e
        WHERE e.{id_col} = $1 AND e.sport = $2 AND e.season = $3
          AND e.rating IS NOT NULL
        "#
    );
    let events_played: i64 = sqlx::query_scalar(&count_q)
        .bind(entity_id)
        .bind(sport)
        .bind(profile.season)
        .fetch_one(pool)
        .await
        .with_context(|| format!("count trajectory events {entity_type}/{entity_id}"))?;
    let window_size = ((events_played as f64 * TRAJECTORY_WINDOW_PCT).round() as i64)
        .clamp(TRAJECTORY_WINDOW_MIN, TRAJECTORY_WINDOW_MAX);

    if events_played < TRAJECTORY_WINDOW_MIN {
        let mut out = RatingTrajectory::steady("sparse_recent_events");
        out.components = serde_json::json!({
            "reason": "sparse_recent_events",
            "events_played": events_played,
            "window_pct": TRAJECTORY_WINDOW_PCT,
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
        });
        return Ok(out);
    }

    let q = format!(
        r#"
        SELECT e.rating::float8
        FROM public.{table} e
        JOIN public.fixtures f ON f.id = e.fixture_id
        WHERE e.{id_col} = $1
          AND e.sport = $2
          AND e.season = $3
          AND e.rating IS NOT NULL
        ORDER BY f.start_time DESC
        LIMIT $4
        "#
    );
    let composite_desc: Vec<f64> = sqlx::query_scalar(&q)
        .bind(entity_id)
        .bind(sport)
        .bind(profile.season)
        .bind(window_size)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load rating trajectory {entity_type}/{entity_id}"))?;

    if composite_desc.len() < TRAJECTORY_WINDOW_MIN as usize {
        let mut out = RatingTrajectory::steady("sparse_z_score_events");
        out.components = serde_json::json!({
            "reason": "sparse_z_score_events",
            "events_played": events_played,
            "window_pct": TRAJECTORY_WINDOW_PCT,
            "window_size": window_size,
            "sample_size": composite_desc.len(),
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
        });
        return Ok(out);
    }

    let mut composite_chrono = composite_desc.clone();
    composite_chrono.reverse();
    let composite_slope = linear_slope(&composite_chrono);
    let key = trajectory_key(composite_slope).to_string();
    let label = Some(z_trajectory_label(&key));

    Ok(RatingTrajectory {
        key,
        label,
        components: serde_json::json!({
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
            "events_played": events_played,
            "window_pct": TRAJECTORY_WINDOW_PCT,
            "window_size": window_size,
            "sample_size": composite_desc.len(),
            "rating_z_slope": round1(composite_slope),
            "latest_rating_z": composite_desc.first().copied().map(round1),
            "recent_rating_z": rounded_series(&composite_desc),
        }),
    })
}

// ---------------------------------------------------------------------------
// Output parsing — the current body plus HEADLINE contract.
// ---------------------------------------------------------------------------

/// RatingReply is the parsed model output: the cleaned body plus the optional card title.
#[derive(Clone, Debug)]
pub struct RatingReply {
    pub body: String,
    /// The card title (s20, mig 226): twelve words or fewer, contracted as the brief's
    /// closing line. `None` when absent — NULL renders downstream as "no headline".
    pub headline: Option<String>,
}

/// RatingParser cleans the body and splits its headline. It never returns `Ok(None)`:
/// an empty body is a hard error the caller raises, and the only marker is the pre-model no-stats path.
/// Since the eval→guard migration (2026-08-19) it DOES fail closed (`Err` → retry) on the brief's
/// global invariants: bullet/Markdown decoration, product names, foreign script.
pub struct RatingParser;

/// parse_rating_body is the shape-only view. The eval gate
/// parses through THIS so a guard-violating reply still shows its prose in the side-by-side
/// and scores red on the invariant checks; production goes through [`RatingParser`], which
/// adds the fail-closed guards on top. The s20 HEADLINE line is split off here too, so the
/// shape view never mistakes a title for a section.
pub fn parse_rating_body(raw: &str) -> String {
    let (_headline, body) = split_rating_headline(raw);
    clean_commentary(&body)
}

/// split_rating_headline lifts the s20 card-title line out of a raw brief: the FIRST line
/// beginning `HEADLINE:` is captured (whitespace-folded; empty ⇒ None) and removed, and the
/// remaining lines are returned in order. Position-tolerant — order drift is a shape quirk,
/// never a failed generation. Markdown decoration is deliberately NOT stripped before the
/// match: a decorated title fails the brief's own plain-text guard downstream.
fn split_rating_headline(raw: &str) -> (Option<String>, String) {
    let mut headline: Option<String> = None;
    let mut kept: Vec<&str> = Vec::new();
    for line in raw.lines() {
        if headline.is_none() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed
                .strip_prefix("HEADLINE:")
                .or_else(|| trimmed.strip_prefix("Headline:"))
            {
                let folded = rest.split_whitespace().collect::<Vec<_>>().join(" ");
                if !folded.is_empty() {
                    headline = Some(folded);
                }
                continue;
            }
        }
        kept.push(line);
    }
    (headline, kept.join("\n"))
}

impl Parser<RatingReply> for RatingParser {
    fn parse(&self, raw: &str) -> Result<Option<RatingReply>> {
        // Split the card title off FIRST so the body checks never grade it as prose.
        let (headline, body_only) = split_rating_headline(raw);
        let body = clean_commentary(&body_only);
        if let Some(p) = crate::guards::first_banned_phrase(&body, crate::guards::RATING_BODY_BANS)
        {
            tracing::warn!(
                guard = "rating_body_ban",
                phrase = p,
                "rating body rejected"
            );
            anyhow::bail!("rating: body carries banned {p:?}");
        }
        if let Some(p) = crate::guards::first_product_name(&body) {
            tracing::warn!(guard = "product_name", name = p, "rating body rejected");
            anyhow::bail!("rating: body names product {p:?}");
        }
        if crate::guards::has_foreign_script(&body) {
            tracing::warn!(guard = "foreign_script", "rating body rejected");
            anyhow::bail!("rating: body carries a foreign-script run");
        }
        // Optional titles fail open: salvage or drop without throwing away the report.
        let headline = crate::guards::settle_title("scout", headline.as_deref());
        Ok(Some(RatingReply { body, headline }))
    }
}

/// Normalize the served prose and remove an accidental wrapping code fence.
fn clean_commentary(raw: &str) -> String {
    let mut s = raw.trim();
    s = s.trim_matches('`');
    s = s.trim();
    crate::guards::clean_served_prose(s)
}

// ---------------------------------------------------------------------------
// The composition: build (deterministic) → generate (model) → persist.
// ---------------------------------------------------------------------------

/// RatingBuild is the deterministic prefix of a generation. `NoStats` ⇒ no usable rating row (no
/// composite + empty breakdown) → a NULL-body marker with no model call.
pub enum RatingBuild {
    NoStats { season: i32 },
    Ready(Box<RatingReady>),
}

/// Assembled model inputs and deterministic context required for persistence.
pub struct RatingReady {
    pub season: i32,
    pub notability: i32,
    pub notability_components: serde_json::Value,
    pub rating_trajectory: RatingTrajectory,
    pub input_components: String, // the canonical JSON (also the hash pre-image)
    pub input_hash: String,
    pub exclusions: RatingExclusions,
    pub opts: GenerateOptions,
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub model_configured: String,
}

/// Build the deterministic rating request without a model call. `with_enrichment` adds prompt-only
/// memory, personnel, and availability blocks without changing `input_components`.
pub async fn build_rating_request(
    hx: &Harness,
    req: &RatingReq,
    temperature: f64,
    with_enrichment: bool,
) -> Result<RatingBuild> {
    let Some(mut profile) = load_rating_profile(
        &hx.pool,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        req.season,
    )
    .await?
    else {
        return Ok(RatingBuild::NoStats {
            season: req.season.unwrap_or(0),
        });
    };
    // Filter once before every downstream consumer so all derived fields share one signal view.
    let off_facet_stat_labels = drop_off_facet_datapoints(&mut profile);
    let degenerate_zero_stat_labels = drop_degenerate_zero_datapoints(&mut profile);
    let display_tier_stat_labels = drop_display_tier_datapoints(&mut profile);
    // No usable rating (no composite + empty breakdown) → the NULL-body marker path.
    if profile.composite_score.is_none() && profile.breakdown.is_empty() {
        return Ok(RatingBuild::NoStats {
            season: profile.season,
        });
    }

    let input_components = input_components(&profile);
    let input_hash = hash_components(&input_components);
    let (notability, notability_components) = compute_notability(&profile);
    let decision = build_scouting_decision(&profile);
    let exclusions = RatingExclusions {
        budget_truncated_stat_labels: budget_truncated_stat_labels(&profile.breakdown, &decision),
        off_facet_stat_labels,
        degenerate_zero_stat_labels,
        display_tier_stat_labels,
    };
    let rating_trajectory = load_rating_trajectory(
        &hx.pool,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        &profile,
    )
    .await?;
    // Memory-load failure degrades to an unenriched prompt (the n8/v12 discipline): the
    // rating profile is the primary signal, memory is enrichment.
    let memory = if with_enrichment {
        match load_stat_memory(
            &hx.pool,
            &req.sport,
            &req.entity_type,
            req.entity_id,
            profile.season,
        )
        .await
        {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    error = %e,
                    "rating: cross-season memory load failed (continuing without memory)"
                );
                None
            }
        }
    } else {
        None
    };
    // Personnel and availability are best-effort prompt enrichment outside the material hash.
    // Load them independently so one failure cannot erase the other block.
    let personnel = if with_enrichment {
        let (changes, total) =
            match load_personnel_changes(&hx.pool, &req.sport, &req.entity_type, req.entity_id)
                .await
            {
                Ok(loaded) => loaded,
                Err(e) => {
                    tracing::warn!(
                        entity_type = %req.entity_type,
                        entity_id = req.entity_id,
                        sport = %req.sport,
                        error = %e,
                        "rating: personnel-change load failed (continuing without the block)"
                    );
                    (Vec::new(), 0)
                }
            };
        let (avail, avail_total) =
            match load_availability_changes(&hx.pool, &req.sport, &req.entity_type, req.entity_id)
                .await
            {
                Ok(loaded) => loaded,
                Err(e) => {
                    tracing::warn!(
                        entity_type = %req.entity_type,
                        entity_id = req.entity_id,
                        sport = %req.sport,
                        error = %e,
                        "rating: availability load failed (continuing without those lines)"
                    );
                    (Vec::new(), 0)
                }
            };
        inputs::render_personnel_block(
            &req.entity_type,
            req.entity_id,
            &changes,
            total,
            &avail,
            avail_total,
        )
    } else {
        None
    };
    // The Editor's TAGGED reports — claims, not record. Same enrichment discipline as everything
    // else here: best-effort, prompt-only, outside `input_components`/`input_hash`.
    let availability_reports = if with_enrichment {
        match load_availability_reports(&hx.pool, &req.entity_type, req.entity_id, &req.sport).await
        {
            Ok(claims) => inputs::render_availability_reports(&claims),
            Err(e) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    error = %e,
                    "rating: availability-report load failed (continuing without the block)"
                );
                None
            }
        }
    } else {
        None
    };
    // Season-over-season movement is decided in code and added as prompt-only enrichment.
    let z_memory = if with_enrichment {
        match load_rating_profile(
            &hx.pool,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            Some(profile.season - 1),
        )
        .await
        {
            Ok(Some(mut prior)) => {
                // The same signal-only view the current profile gets: off-facet, degenerate-zero,
                // and display-tier datapoints never enter a movement comparison.
                let _ = drop_off_facet_datapoints(&mut prior);
                let _ = drop_degenerate_zero_datapoints(&mut prior);
                let _ = drop_display_tier_datapoints(&mut prior);
                build_z_memory_lines(&profile, &prior)
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    error = %e,
                    "rating: prior-season profile load failed (continuing without movement lines)"
                );
                None
            }
        }
    } else {
        None
    };
    // The recent-form marker rides the same enrichment flag: shading context in production,
    // absent on the parity/eval bare shape.
    let form_trend = if with_enrichment {
        rating_trajectory.label.clone()
    } else {
        None
    };
    // Identity card: house records, dated — degrades to absent like memory.
    let identity =
        crate::corpus::load_identity_card(&hx.pool, &req.entity_type, req.entity_id, &req.sport)
            .await
            .unwrap_or_default();
    let built_prompt = build_stat_prompt(
        req,
        &profile,
        notability,
        memory.as_deref(),
        personnel.as_deref(),
        z_memory.as_deref(),
        form_trend.as_deref(),
        availability_reports.as_deref(),
        identity.as_deref(),
    );
    let opts = GenerateOptions {
        system: Some(RATING_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        // The Scout's reservation follows the window like every other voice (7.12): 2,000
        // inside a 4,096 window leaves ~2,000 for a ~1,370-token system prompt plus the stats
        // context plus the memory card, which is the silent system-prompt eviction this rule
        // exists to prevent. Its report gets shorter at 4096; that is the honest trade, and the
        // diet is what buys the length back.
        num_predict: if crate::route::small_voice_window(hx.voice_num_ctx) {
            crate::junctions::oracle::SMALL_WINDOW_NUM_PREDICT
        } else {
            RATING_NUM_PREDICT
        },
        num_ctx: hx.voice_num_ctx,
        json_mode: false,
        format_schema: None,
        format_schema_raw: None,
    };
    let backend = hx.router.for_role(Role::StatsLogic);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(RatingBuild::Ready(Box::new(RatingReady {
        season: profile.season,
        notability,
        notability_components,
        rating_trajectory,
        input_components,
        input_hash,
        exclusions,
        opts,
        built_prompt,
        request_body,
        model_configured,
    })))
}

/// The un-persisted result of one generation. The production handler persists it to
/// `stat_summaries`, and the ledger records the prompt/request/evidence envelope.
#[derive(Clone, Debug)]
pub struct RatingProduct {
    pub season: i32,
    pub skipped_no_stats: bool,
    pub skipped_unchanged: bool,
    pub body: Option<String>, // None for a marker
    /// The card title (s20). `None` for a marker and when the reply omitted the line.
    pub headline: Option<String>,
    pub notability: Option<i32>,
    pub notability_components: serde_json::Value,
    pub rating_trajectory: Option<String>,
    pub rating_trajectory_label: Option<String>,
    pub rating_trajectory_components: serde_json::Value,
    pub input_components: String, // "{}" for a marker
    pub exclusions: RatingExclusions,
}

pub type RatingOutput = Generation<RatingProduct>;

/// generate_rating runs the full per-entity generation (the analog of `RatingGenerator.Generate`,
/// minus persistence): `build_rating_request` → (skip-unchanged debounce) → `extract(StatsLogic)` →
/// parse → clean. The per-entity core the Step-3 batch bin loops over; also the parity `--vet` path.
/// `skip_unchanged` short-circuits (no model call) when the entity-season's last commentary was built
/// from the same rating snapshot (matching input_hash) — the nightly "work only on new data" gate.
pub async fn generate_rating(
    hx: &Harness,
    req: &RatingReq,
    temperature: f64,
    skip_unchanged: bool,
    with_enrichment: bool,
) -> Result<RatingOutput> {
    let ready = match build_rating_request(hx, req, temperature, with_enrichment).await? {
        RatingBuild::NoStats { season } => {
            // Keep configured-model provenance on the NULL-body marker.
            let model = hx.router.for_role(Role::StatsLogic).model().to_string();
            return Ok(Generation::uncalled(
                RatingProduct {
                    season,
                    skipped_no_stats: true,
                    skipped_unchanged: false,
                    body: None,
                    headline: None,
                    notability: None,
                    notability_components: serde_json::json!({}),
                    rating_trajectory: None,
                    rating_trajectory_label: None,
                    rating_trajectory_components: serde_json::json!({}),
                    input_components: "{}".to_string(),
                    exclusions: RatingExclusions::default(),
                },
                model,
                RATING_PROMPT_VERSION,
                Vec::new(),
                None,
            ));
        }
        RatingBuild::Ready(r) => *r,
    };

    if skip_unchanged {
        if let Some(last_hash) = last_commentary_input_hash(
            hx,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            ready.season,
        )
        .await?
        {
            if last_hash == ready.input_hash {
                return Ok(Generation::uncalled(
                    RatingProduct {
                        season: ready.season,
                        skipped_no_stats: false,
                        skipped_unchanged: true,
                        body: None,
                        headline: None,
                        notability: None,
                        notability_components: serde_json::json!({}),
                        rating_trajectory: Some(ready.rating_trajectory.key.clone()),
                        rating_trajectory_label: ready.rating_trajectory.label.clone(),
                        rating_trajectory_components: ready.rating_trajectory.components.clone(),
                        input_components: ready.input_components,
                        exclusions: ready.exclusions,
                    },
                    ready.model_configured,
                    RATING_PROMPT_VERSION,
                    Vec::new(),
                    Some(ready.input_hash),
                ));
            }
        }
    }

    let extracted = hx
        .extract(
            Role::StatsLogic,
            &ready.built_prompt,
            &ready.opts,
            &RatingParser,
        )
        .await?;
    let call = GenerationCall::from(&extracted);
    let model = extracted.model.clone();
    let reply = extracted
        .value
        .ok_or_else(|| anyhow!("rating: parser returned None (RatingParser never fails closed)"))?;
    if reply.body.is_empty() {
        bail!(
            "rating: empty commentary ({}/{} {})",
            req.entity_type,
            req.entity_id,
            req.sport
        );
    }
    // The hook doctrine's naming rule as a floor, at the one site that knows the entity
    // (the parser is stateless by design). Measured 2026-08-26: 55% of live team headlines
    // named an invented club — the worked example copied verbatim ("Harborview…") or
    // remixed ("Rovers…") onto real clubs' cards. Integrity, not style: degrade to no
    // title, the same state an absent HEADLINE line already ships, never a retry — the
    // report under it is fine.
    let headline = reply.headline.filter(|t| {
        let named = crate::guards::title_names_entity(t, &req.entity_name);
        if !named {
            tracing::warn!(seat = "scout", guard = "title_entity_absent",
                entity = %req.entity_name, title = %t,
                "headline names no form of the entity; dropped");
        }
        named
    });

    Ok(Generation::called(
        RatingProduct {
            season: ready.season,
            skipped_no_stats: false,
            skipped_unchanged: false,
            body: Some(reply.body),
            headline,
            notability: Some(ready.notability),
            notability_components: ready.notability_components,
            rating_trajectory: Some(ready.rating_trajectory.key),
            rating_trajectory_label: ready.rating_trajectory.label,
            rating_trajectory_components: ready.rating_trajectory.components,
            input_components: ready.input_components,
            exclusions: ready.exclusions,
        },
        model,
        RATING_PROMPT_VERSION,
        Vec::new(),
        Some(ready.input_hash),
        call,
    ))
}

/// Durable queue fingerprint for a rating-card demand. The input hash already includes the prompt
/// version, so the queue needs only season and hash (or an explicit marker).
pub fn rating_work_input_version(season: i32, input_hash: Option<&str>) -> String {
    format!(
        "{RATING_WORK_PREFIX}{season}:{}",
        input_hash.filter(|s| !s.is_empty()).unwrap_or("no-stats")
    )
}

/// The marker that says this rating row was opened by a transfer crossing the concrete
/// threshold, not by the stats moving. Sits in the `input_hash` slot of the work row's
/// `input_version`, so `rating_work_season` still parses the season out of the prefix.
const RATING_WORK_TRANSFER_MARK: &str = "xfer";

/// The marker for a rating opened by an applied injury or suspension (mig 229). Same slot and
/// same purpose as [`RATING_WORK_TRANSFER_MARK`].
///
/// **Both marks are safe to distinguish from a real `input_hash` by prefix** because that slot
/// otherwise holds a hex digest or the literal `no-stats`: `x` and `v` are not hex digits, so
/// neither mark can collide with a hash however the digest comes out.
const RATING_WORK_AVAIL_MARK: &str = "avail";

/// The prefix mig 225's `enqueue_voices_on_packet` stamps on a voice's work row:
/// `'pk:' || COALESCE(slice_fingerprints->>stage, id::text)`.
///
/// For the `rating` stage that slice is the injury/suspension claim hash, so a `pk:` rating row
/// means one thing only — the Editor tagged this entity because its availability news moved.
const PACKET_WORK_PREFIX: &str = "pk:";

/// Version for a rating opened by an adjudicated transfer. The application ID reopens the work
/// row even though stats did not move; the handler also bypasses the stats-only debounce.
pub fn rating_work_input_version_for_transfer(season: i32, application_id: i64) -> String {
    format!("{RATING_WORK_PREFIX}{season}:{RATING_WORK_TRANSFER_MARK}{application_id}")
}

/// Version for a rating opened by an applied injury or suspension. Keying by event day reopens
/// unchanged stats while collapsing multiple same-day events into one work row.
///
/// `day` must be the event's `player_availability.event_date` rendered `YYYY-MM-DD`. It is a
/// DATE in the schema on purpose — a timestamp, or a date taken from a local zone rather than a
/// fixed one, splits one event day across two versions and the collapse silently stops
/// collapsing.
pub fn rating_work_input_version_for_availability(season: i32, day: &str) -> String {
    format!("{RATING_WORK_PREFIX}{season}:{RATING_WORK_AVAIL_MARK}{day}")
}

/// The marker token sitting in the `input_version`'s `input_hash` slot, if the version parses.
/// Returns the raw slot contents — a mark, a hex digest, or `no-stats`.
fn rating_work_mark(input_version: Option<&str>) -> Option<&str> {
    input_version
        .and_then(|raw| raw.strip_prefix(RATING_WORK_PREFIX))
        .and_then(|rest| rest.rsplit_once(':'))
        .map(|(_, mark)| mark)
}

/// True when this work row was opened by a concrete transfer rather than by moved stats.
fn rating_work_is_transfer_triggered(input_version: Option<&str>) -> bool {
    rating_work_mark(input_version).is_some_and(|h| h.starts_with(RATING_WORK_TRANSFER_MARK))
}

/// True when this work row was opened by an applied injury or suspension.
fn rating_work_is_availability_triggered(input_version: Option<&str>) -> bool {
    rating_work_mark(input_version).is_some_and(|h| h.starts_with(RATING_WORK_AVAIL_MARK))
}

/// Classify what woke the Scout for `stat_summaries.trigger_type`.
fn rating_trigger_type(input_version: Option<&str>) -> &'static str {
    // A `pk:` rating version hashes availability claims and is therefore an availability trigger.
    if rating_work_is_packet_triggered(input_version) {
        return "availability";
    }
    match rating_work_mark(input_version) {
        Some(h) if h.starts_with(RATING_WORK_TRANSFER_MARK) => "transfer",
        Some(h) if h.starts_with(RATING_WORK_AVAIL_MARK) => "availability",
        _ => "periodic",
    }
}

/// Whether the Editor's availability packet opened this row.
fn rating_work_is_packet_triggered(input_version: Option<&str>) -> bool {
    input_version.is_some_and(|raw| raw.starts_with(PACKET_WORK_PREFIX))
}

/// True when the `skip_unchanged` debounce must be turned OFF for this item.
///
/// Non-statistical triggers bypass the stats-only material hash so the Scout sees their prompt
/// enrichment after the work row reopens.
fn rating_work_bypasses_debounce(input_version: Option<&str>) -> bool {
    rating_work_is_transfer_triggered(input_version)
        || rating_work_is_availability_triggered(input_version)
        || rating_work_is_packet_triggered(input_version)
}

fn rating_work_season(input_version: Option<&str>) -> Option<i32> {
    let raw = input_version?;
    let rest = raw.strip_prefix(RATING_WORK_PREFIX)?;
    let (season, _) = rest.split_once(':')?;
    season.parse::<i32>().ok().filter(|s| *s > 0)
}

/// Return the input hash from the entity-season's latest commentary. Take
/// the latest row regardless of nullability; a no-stats marker has a NULL input_hash → None →
/// the next run never wrongly skips against an older real commentary the marker superseded.
async fn last_commentary_input_hash(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        r#"
        SELECT input_hash FROM stat_summaries
        WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4
        ORDER BY generated_at DESC LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(&hx.pool)
    .await
    .with_context(|| format!("last commentary provenance {entity_type}/{entity_id}"))?;
    Ok(row.and_then(|(hash,)| hash.filter(|h| !h.is_empty())))
}

/// persist_stat_summary writes ONE row to the LIVE stat_summaries table — the scored commentary and
/// the no-stats marker, which differ only in the bound values (body/notability NULL for
/// the marker; model_version is set for both).
pub async fn persist_stat_summary(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    trigger_type: &str,
    trigger_payload: &serde_json::Value,
    out: &RatingOutput,
) -> Result<()> {
    let season: Option<i32> = (out.season > 0).then_some(out.season);
    let notability: Option<i16> = out.notability.map(|n| n as i16);
    let prov = &out.provenance;
    let trigger_json = trigger_payload.to_string();
    let ncomp_json = out.notability_components.to_string();
    let trajectory_components_json = out.rating_trajectory_components.to_string();

    let row = sqlx::query(
        r#"
        INSERT INTO stat_summaries (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            body, headline, notability, notability_components, input_components, input_hash,
            model_version, prompt_version, generated_at,
            rating_trajectory, rating_trajectory_label, rating_trajectory_components
        ) VALUES ($1,$2,$3,$4,$5,$6::jsonb, $7,$8,$9,$10::jsonb,$11::jsonb,$12, $13,$14,NOW(),
                  $15,$16,$17::jsonb)
        RETURNING id
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .bind(trigger_type)
    .bind(&trigger_json)
    .bind(out.body.as_deref())
    .bind(out.headline.as_deref())
    .bind(notability)
    .bind(&ncomp_json)
    .bind(&out.input_components)
    .bind(prov.input_hash.as_deref())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .bind(out.rating_trajectory.as_deref())
    .bind(out.rating_trajectory_label.as_deref())
    .bind(&trajectory_components_json)
    .fetch_one(pool)
    .await
    .context("persist stat summary")?;
    let product_row_id = row.get("id");
    let included_evidence = serde_json::json!({
        "input_components": serde_json::from_str::<serde_json::Value>(&out.input_components)
            .unwrap_or_else(|_| serde_json::json!({
                "raw_input_components": &out.input_components
            })),
        "notability": out.notability,
        "notability_components": &out.notability_components,
        "rating_trajectory": &out.rating_trajectory,
        "rating_trajectory_label": &out.rating_trajectory_label,
        "rating_trajectory_components": &out.rating_trajectory_components,
    });
    let mut excluded = Vec::new();
    if out.skipped_no_stats {
        excluded.push(serde_json::json!({"reason": "no_usable_rating_profile"}));
    }
    if out.skipped_unchanged {
        excluded.push(serde_json::json!({"reason": "input_hash_unchanged"}));
    }
    if !out.exclusions.budget_truncated_stat_labels.is_empty() {
        let labels = &out.exclusions.budget_truncated_stat_labels;
        excluded.push(serde_json::json!({
            "reason": "budget_truncated_stat_facts",
            "dropped_count": labels.len(),
            "dropped_stat_labels": labels,
            "limit": MAX_STAT_FACTS,
        }));
    }
    for (reason, labels) in [
        (
            "off_facet_position_mismatch",
            &out.exclusions.off_facet_stat_labels,
        ),
        (
            "degenerate_zero_usage_artifact",
            &out.exclusions.degenerate_zero_stat_labels,
        ),
        (
            "display_tier_retired_from_equation",
            &out.exclusions.display_tier_stat_labels,
        ),
    ] {
        if !labels.is_empty() {
            excluded.push(serde_json::json!({
                "reason": reason,
                "dropped_count": labels.len(),
                "dropped_stat_labels": labels,
            }));
        }
    }
    insert_generation_ledger_best_effort(
        pool,
        out,
        RATING_LEDGER,
        LedgerEvent {
            entity_type,
            entity_id,
            sport,
            pair_entity: None,
            trigger_type,
            trigger_payload: trigger_payload.clone(),
            product_row_ids: vec![product_row_id],
            included_evidence,
            excluded_evidence: serde_json::json!(excluded),
            context_budget: out.context_budget(serde_json::json!({
                "num_predict": out.request_body()
                    .and_then(|b| b.pointer("/options/num_predict"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(RATING_NUM_PREDICT as i64),
            })),
            parser_outcome: if out.skipped_no_stats {
                "no_call"
            } else {
                "parsed"
            },
        },
    )
    .await;
    Ok(())
}

/// Best-effort Scout trigger when a transfer becomes a roster fact. Offer the player and both
/// clubs; the nightly batch remains the backstop.
pub async fn enqueue_rating_for_applied_transfer(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    old_team_id: Option<i32>,
    new_team_id: Option<i32>,
    application_id: i64,
) -> Result<()> {
    let sport = sport.to_uppercase();
    let season = current_season(pool, &sport).await?;
    let input_version = rating_work_input_version_for_transfer(season, application_id);

    let mut targets: Vec<(&str, i64)> = vec![("player", i64::from(player_id))];
    for team in [old_team_id, new_team_id].into_iter().flatten() {
        targets.push(("team", i64::from(team)));
    }

    for (entity_type, entity_id) in targets {
        let item = Item {
            stage: Stage::Rating,
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.clone(),
            input_version: Some(input_version.clone()),
            attempts: 0,
        };
        if let Err(e) = crate::work::enqueue(pool, &item).await {
            warn!(
                application_id,
                entity_type,
                entity_id,
                sport = %sport,
                "rating: could not enqueue on applied transfer: {e:#}"
            );
        }
    }
    Ok(())
}

/// Best-effort Scout trigger when reported unavailability becomes a roster fact.
///
/// **`event_day` must arrive as `event_date::text` straight from Postgres** — `YYYY-MM-DD`, the
/// DATE the schema stores. This crate carries no date library and does not parse one here on
/// purpose: the day never leaves Postgres as a timestamp, so there is no local zone to render it
/// through and no way for one event day to split across two `input_version`s. That split is the
/// only thing that can break Scott's once-per-day rule, and this signature is what forecloses it.
///
/// Two targets, not the transfer path's three: the player and the club they are at. An injury has
/// no old/new club — the squad that loses availability is one squad.
///
/// Best-effort by design, exactly like the transfer trigger: a failure to enqueue must never fail
/// the adjudication that earned it, and the nightly batch remains the backstop.
pub async fn enqueue_rating_for_applied_availability(
    pool: &PgPool,
    sport: &str,
    player_id: i32,
    team_id: Option<i32>,
    event_day: &str,
) -> Result<()> {
    let sport = sport.to_uppercase();
    let season = current_season(pool, &sport).await?;
    let input_version = rating_work_input_version_for_availability(season, event_day);

    let mut targets: Vec<(&str, i64)> = vec![("player", i64::from(player_id))];
    if let Some(team) = team_id {
        targets.push(("team", i64::from(team)));
    }

    for (entity_type, entity_id) in targets {
        let item = Item {
            stage: Stage::Rating,
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.clone(),
            input_version: Some(input_version.clone()),
            attempts: 0,
        };
        if let Err(e) = crate::work::enqueue(pool, &item).await {
            warn!(
                event_day,
                entity_type,
                entity_id,
                sport = %sport,
                "rating: could not enqueue on applied availability: {e:#}"
            );
        }
    }
    Ok(())
}

async fn current_season(pool: &PgPool, sport: &str) -> Result<i32> {
    sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("current season {sport}"))
}

/// Queue-owned rating handler: generate and persist the scouting card only when
/// the rating input hash moved, then enqueue Momentum as the downstream consumer of the fresh
/// rating pillar.
pub struct RatingHandler;

impl RatingHandler {
    pub fn new() -> Self {
        RatingHandler
    }
}

impl Default for RatingHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for RatingHandler {
    fn stage(&self) -> Stage {
        Stage::Rating
    }

    // Two slots keep a long Scout decode from taking the group from The Editor.
    fn max_in_flight(&self) -> usize {
        2
    }
    fn slot_group(&self) -> Option<(&'static str, usize)> {
        Some(crate::stage::ARCHBOX_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let season = match rating_work_season(item.input_version.as_deref()) {
            Some(season) => season,
            None => current_season(&hx.pool, &sport).await?,
        };
        let name =
            crate::corpus::lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &sport)
                .await?;
        // A move that crossed the concrete threshold — or an applied injury or suspension — is
        // its own trigger, and it must not be debounced away: the stats have not changed, so the
        // input_hash has not changed, and the ordinary `skip_unchanged` gate would short-circuit
        // before the model call. What changed is the personnel block, and it reaches the prompt
        // through `with_enrichment`, outside the hash pre-image.
        let bypass = rating_work_bypasses_debounce(item.input_version.as_deref());
        let req = RatingReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: sport.clone(),
            trigger_type: rating_trigger_type(item.input_version.as_deref()).to_string(),
            season: Some(season),
        };

        let out = generate_rating(hx, &req, RATING_TEMPERATURE, !bypass, true).await?;
        if out.skipped_unchanged {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                season = out.season,
                "rating: skipped unchanged rating input"
            );
            return Ok(());
        }

        persist_stat_summary(
            &hx.pool,
            &item.entity_type,
            entity_id,
            &sport,
            &req.trigger_type,
            &serde_json::json!({}),
            &out,
        )
        .await?;
        crate::junctions::analyst::enqueue_momentum_if_needed(
            hx,
            &item.entity_type,
            entity_id,
            &sport,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

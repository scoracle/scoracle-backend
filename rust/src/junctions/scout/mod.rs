//! Rating stage — the stats-rail scouting report.
//!
//! Rust owns both rating shapes: the per-entity core here, and a `RatingHandler` queue stage for
//! current-season need-based rating work. `cmd/statcommentary` remains the operator/batch entry
//! point: nightly mode enqueues durable rating work, while explicit backfill can still run the
//! core inline for historical seasons. (s19 PEAK retirement: the specialist lens is gone — the
//! rating is the z-score synthesis, and the Scout's brief surfaces specialists as prose, not as a
//! divined label. The queue stage is `rating` everywhere since mig 221 — Wave B is done.)
//!
//! Composition (Plan §1.2 + §4): `route(StatsLogic) + extract + persist`. Rating is the FIRST
//! `Role::StatsLogic` consumer (vibe/transfers are `EmotionalNews`). The deterministic parts stay
//! where they belong — composite / T-score / the `rating_breakdown` percentiles (`pct`/`z`) are
//! Postgres-computed stored derived stats, READ here, never recomputed. The transient prompt-shaping
//! (notability, `pctBand`, `trimFloat`, ordered facts) is mirrored in Rust byte-for-byte: it is NOT a
//! stored derived stat, so it lives in the Rust stage beside the model call. The L8 BREAKTHROUGH is preserved: the
//! percentile→tier mapping (`pctBand`) is done DETERMINISTICALLY in code and fed to the model as a
//! labeled FACT, and the model only VERBALIZES the labeled tier — it never maps percentile→quality
//! itself (some local models invert this, e.g. calling a 37th-pct skill "above average").
//!
//! FAIL CLOSED: rating's ONLY marker is the PRE-model no-stats path (no usable rating row → a
//! NULL-body marker, like vibe's no-corpus marker). There is no post-model fail-closed marker — an
//! empty model body is a hard error (the work fails + retries), never a served row (Go returns an
//! error too). So `RatingParser` never returns `Ok(None)` (like `VibeParser`).
//!
//! The deterministic profile assembly, input hash, and parser stay byte-stable. The s14 prompt is
//! Rust-owned and model-neutral, with the same core invariant: the labeled tier is the truth.

use crate::harness::{Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{go_json_float, go_json_string, hash_components, round1};
use crate::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

// This junction's contract with its model — system prompt, contract version, and prompt
// builder — lives in `prompt.rs`, so a change to what this character is asked is a one-file
// diff. Re-exported here so call sites and the ledger keep reading it from the stage module.
pub mod prompt;
pub use prompt::{
    build_stat_prompt, render_availability_reports, render_personnel_block, RATING_PROMPT_VERSION,
    RATING_SYSTEM_PROMPT,
};

/// Output contract captured separately in the Phase 2 diagnostic ledger.
pub const RATING_OUTPUT_CONTRACT_VERSION: &str = "rating-commentary-v1"; // was peak-commentary-v2; s19 PEAK retirement — body-only output, no divined label

/// Production rating temperature.
pub const RATING_TEMPERATURE: f64 = 0.6;

/// Token cap for the scouting brief.
// A card, not a report: three short labelled lines plus a twelve-word title is ~200 tokens, and
// 350 leaves room to finish a sentence rather than truncate mid-clause. Was 2000, sized when the
// rail ran a 14B and the Summary allowance was eight sentences — that budget alone could ask for
// half again the whole 4,096 window.
pub const RATING_NUM_PREDICT: i32 = 350;

/// Durable rating queue input_version prefix. The queue key is entity/sport-scoped for
/// historical compatibility; the season is carried in the version so the handler can drain
/// explicit current-season demands without re-resolving the wrong season. (mig 221 renamed
/// the stage and this prefix from the retired "peak".)
const RATING_WORK_PREFIX: &str = "rating:s";

/// maxStatFacts bounds the breakdown datapoints fed to the prompt.
const MAX_STAT_FACTS: usize = 14;

/// The entity whose rating profile to narrate — the Rust analog of `RatingRequest`'s parity-relevant
/// fields. `sport` is UPPER-cased by the caller (the Go CLI passes `sportUpper`); the header line uses
/// it verbatim, so it must already be upper for byte parity.
#[derive(Clone, Debug)]
pub struct RatingReq {
    pub entity_type: String, // "player" | "team"
    pub entity_id: i32,
    pub entity_name: String,
    pub sport: String,       // UPPER ("NBA" | "NFL" | "FOOTBALL")
    pub season: Option<i32>, // None = the entity's latest season
    pub trigger_type: String,
}

/// One element of the `rating_breakdown` JSONB (migration 030/043). `pct` is the percentile of
/// `sign*z`, so HIGHER IS ALWAYS BETTER (a high pct in turnovers = commits few). Field names match the
/// Go `ratingDatapoint` json tags exactly so serde reads the same JSONB. `pct`/`z`/`value` come from
/// JSONB text → identical f64 across Go/Rust (no DB numeric cast involved). Mirrors `ratingDatapoint`.
///
/// Every field is `null_to_default`: Go's `json.Unmarshal` treats an explicit `null` as the zero value
/// (a sparse datapoint may carry `"value": null`, e.g. "Penalties Won"); plain `#[serde(default)]`
/// only covers a MISSING field, not a present null, so without this serde errors where Go tolerates —
/// the parity break the L12 gate surfaced.
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

/// null_to_default maps a present-`null` (and a missing field, via the companion `#[serde(default)]`)
/// to `T::default` — reproducing Go's `encoding/json`, which keeps the zero value for a null rather
/// than erroring. Applied to every `RatingDatapoint` scalar so a null in the breakdown matches Go.
fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// null_tolerant_map is the map analog for `scoped_pct`: a null map → empty, and a null VALUE inside
/// → 0.0 — Go unmarshals `{"position": null}` into `map[string]float64` as 0.0 (the key kept), so this
/// matches. (scoped_pct feeds only the prompt's "[position: …]" suffix, never the input_hash.)
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

/// The entity's scrubbed rating profile — mirrors `ratingProfile`. `composite_score` comes from
/// the numeric/float8 COLUMN (cast `::float8` on read — the sqlx numeric landmine); the
/// breakdown/scoped/modes are JSONB. The breakdown's ARRAY ORDER is preserved (jsonb keeps array
/// order), which `input_components` relies on (it walks the breakdown in stored order, unlike the
/// prompt which sorts by pct). (s19: the specialist columns — peak_score/peak_label — are retired;
/// the breakdown IS the full z-score surface.)
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
// Loader — the SQL `rating.go::loadRatingProfile` runs (same query ⇒ same row).
// ---------------------------------------------------------------------------

/// load_rating_profile reads the entity's rating row for `season` (None = latest). Prefers the
/// unscoped row, falling back to the richest league row (FOOTBALL is league-scoped). SQL VERBATIM
/// from Go (only `::float8` casts added on the numeric score columns — sqlx has no numeric decode
/// without the decimal feature; `::text` on the JSONB so serde parses it, array order preserved).
/// Returns `None` when there is no rating row at all.
pub async fn load_rating_profile(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<RatingProfile>> {
    // `team_stats` has NO `rating_modes` column — per-x rate modes (per_36 / per_90) are a
    // player/minutes concept — so the loader selects it ONLY for players; teams get an empty-modes
    // literal. This is a DELIBERATE divergence from Go's verbatim loader, which `SELECT`s rating_modes
    // from BOTH tables and therefore ERRORS on every team — the latent bug that left team rating
    // commentary dormant (Go's cmd/statcommentary silently fails each team every run; 0 team rows in
    // stat_summaries). Fixing it HERE — the cutover's single cognition home — is new Rust-only
    // capability: team rating now loads + generates. It is validated by quality-eval, NOT Go
    // byte-parity (Go has no team baseline to match); the player path is byte-identical to before, so
    // player parity is untouched.
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
    // Tolerant: the cohort framing + the per-x modes are optional (Go ignores their parse errors).
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
/// keeping only non-empty breakdowns. Tolerant (a parse error ⇒ no modes), mirroring Go's
/// `if err == nil` guard. The per-x lens is the reveal — elite rate production the raw totals hide.
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
// Deterministic helpers — mirrored byte-for-byte from rating.go (transient prompt-shaping; NOT
// stored derived stats, so they live in the stage exactly as Go does it — the transfers precedent).
// ---------------------------------------------------------------------------

/// compute_notability returns the deterministic distinctiveness score (0-100) + its components. The
/// model NEVER sees the formula — only the resulting length guidance (rendered into the prompt, so it
/// IS implicitly a parity axis via built_prompt). Mirrors `computeNotability`. Order-independent (the
/// rate-mode loop only takes a max), so the HashMap iteration order does not affect the result.
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
/// (< 1) with two places, everything else with one ("3" / "0.38" / "10.7"). Mirrors `trimFloat`
/// (Go `%.0f` / `%.2f` / `%.1f`; Rust's `{:.N}` rounds half-to-even identically).
fn trim_float(f: f64) -> String {
    if f == f.trunc() {
        format!("{f:.0}")
    } else if f.abs() < 1.0 {
        format!("{f:.2}")
    } else {
        format!("{f:.1}")
    }
}

/// ordered_facts returns a datapoint set that SPANS the entity's percentile range, bounded to
/// MAX_STAT_FACTS and presented in pct DESC order.
///
/// s21. This used to sort by pct DESC and truncate, which is top-N: on a forty-facet team the
/// Scout saw the fourteen things the entity does best and never the bottom of its own
/// distribution. The only weaknesses that reached him arrived through the decision card as a
/// finished verdict rather than as evidence he could weigh, so a report meant to be thorough was
/// built on the top third of the range.
///
/// Scott's brief for this seat: a front-office evaluator writing "a detailed, unbiased report on
/// the target entity... thorough, which is why we have it analyze the z-score range and not just
/// top and bottom scores." Top-N cannot produce that, and neither can top-plus-bottom: the middle
/// is where an average team is actually average, and saying so is a finding.
///
/// The cap stays — the voices are pinned to a 4,096 window and the datapoint block is the
/// biggest thing in this prompt — so the budget is SPENT differently instead of raised. Both
/// ends are taken whole, because that is where the decisions live, and the remainder is an even
/// stride through the middle so the shape of the distribution survives the sampling.
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
/// (Go `sort.Strings` == Rust `str` Ord, both byte-wise); ≤5 per mode. Mirrors `collectRateStandouts`.
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

/// drop_display_tier_datapoints removes datapoints the rating engine has RETIRED from its
/// equation: `in_comp=false AND in_spec=false` — the display tier (migs 060/062: metrics that
/// reward reactive volume or re-skin other signals; kept for the stats-page z-pizza, out of
/// the composite and specialist pools). Session D (North Star #5): the scouting context is
/// z-score-backed signal only, so the display tier stops leaking into prompts — before this
/// filter a display-tier metric could even be CROWNED (367 FOOTBALL players' PEAK line was
/// one, e.g. Dan Burn's "PEAK: Clearances", the exact case mig 062 called perverse). Runs on
/// the breakdown AND the per-x rate modes; returns the dropped breakdown labels for the
/// exclusions ledger. Flags are present on every stored breakdown element (verified across
/// all sports/seasons 2026-07-17), so a missing-flag row cannot be silently emptied.
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
    // "rating", never "z". Scott, 2026-08-23: "the z-score is our house rating... z-score is
    // going to be meaningless for 99% of our users. Rating will work for everyone."
    //
    // This is also the seventh place the input-shouting law has bitten: the crown died live on
    // `reading carries banned vocabulary "z-score"` because THIS line handed the Scout a `z`, he
    // dutifully cited it, and the Oracle read his card. Renaming the label at its source is the
    // fix that holds; oracle::prompt::descrub_z stays only as a backstop for rows banked before
    // this bump.
    let dz = d.sign as f64 * d.z; // sign-adjusted so + is always the good direction
    let mut s = format!(
        "{}: {} · {:.0}th pct ({}) · rating {:+.1}",
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
    // Per-x corroboration rides the strength line itself (s14): echo-prone models speak the
    // card but skipped the separate rate-standouts section (gate rounds 1-2), so the proof
    // the edge is real at low minutes must sit where the primary-strength evidence is.
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

fn render_scouting_decision(d: &ScoutingDecision) -> String {
    let mut b = String::new();
    // s18: the card's own labels stopped saying "PEAK"/"SCOUTING DECISION" — the s13-analyst
    // lesson is that an output ban cannot beat a word the input keeps shouting, so the input
    // stopped shouting it. (s19 retired the divined label outright: the card carries strengths
    // and weaknesses; specialist-ness is something the brief SAYS when true, not a field.)
    b.push_str("\nDECISION CARD\n");
    match &d.primary_strength_to_stop {
        Some(f) => b.push_str(&format!("Headline strength: {}\n", f.evidence)),
        None => {
            b.push_str("Headline strength: None; no strong/elite skill exists.\n");
        }
    }
    if d.secondary_strengths.is_empty() {
        b.push_str("Secondary strengths: None supplied.\n");
    } else {
        let strengths = d
            .secondary_strengths
            .iter()
            .map(|f| f.evidence.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        b.push_str(&format!("Secondary strengths: {strengths}\n"));
    }
    match &d.primary_weakness_to_exploit {
        Some(f) => b.push_str(&format!("Headline limitation: {}\n", f.evidence)),
        // The card says the words the model must speak (s14): echo-prone local models
        // reliably recite the card, so "no clean exploit" lives HERE, not "None supplied"
        // (which they echoed verbatim instead of the contract phrase — gate round 2).
        None => {
            b.push_str("Headline limitation: None — this profile offers no clean exploit.\n")
        }
    }
    if let Some(reason) = &d.no_standout_reason {
        b.push_str(&format!("Why no standout: {reason}\n"));
    }
    b
}

/// build_stat_prompt assembles the user prompt. s9 reframes this as a deterministic opposing-scout
/// decision card plus supporting datapoints: the model explains the prepared decisions instead of
/// inferring the structured label from the list. The `·` (U+00B7) and `—` (U+2014) are significant
/// bytes; the tier labels are pctBand's deterministic output.
/// load_stat_memory fetches the cross-season stats memory card (`stat_context_for_entity`,
/// mig 164): prior-season top-skill read, confirmed moves, reliability-framed matchup edges.
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
    // mig 221 rewrote stat_context_for_entity to render the retired vocabulary out at
    // source, which retires the s18 Rust-side descrub shim: the card arrives clean.
    Ok(row.0)
}

/// Season-over-season movement threshold (s19): a per-skill percentile move of at least this
/// many points earns "improved"/"slipped"; anything smaller is "held".
const Z_MEMORY_MOVE_PCT_POINTS: f64 = 8.0;
/// Cap on movement lines rendered into the prompt (top by current pct — the A5 rule does not
/// apply: unmatched skills are new-season datapoints, not dropped evidence).
const Z_MEMORY_MAX_LINES: usize = 10;

/// build_z_memory_lines renders the per-skill season-over-season movement block (s19): for each
/// current datapoint with a matching prior-season label, one line carrying both percentiles,
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

/// load_personnel_changes reads the adjudicated personnel record (7.7) — `applied` and
/// `reverted` rows of `transfer_identity_applications`, the SAME chain the Insider's
/// adjudication writes and the `transfer_ground_truth` view is built over — for everything that
/// moved since this entity was last read.
///
/// **Why a second road at all, when the memory card already carries "confirmed moves":** that
/// card reads `transfer_ground_truth`, which is `DISTINCT ON (sport, player_id, team_id)` over
/// non-reverted applications on a fixed 180-day window, LIMIT 3. Four facts an opposing scout
/// needs never survive it — (1) a TEAM's departures (the view's team branch matches
/// `new_team_id` only, so a club losing a player sees nothing), (2) the club a player came
/// FROM, (3) a REVERT (the view filters `reverted_at IS NULL`, so a correction to a move the
/// last brief was written around is invisible), and (4) the since-last-read framing that makes
/// any of it new information. The memory card keeps its slow cross-season arc lines; this block
/// is the delta.
///
/// **T4 holds by construction:** no prose reaches the Scout. Every field here is a date, an id
/// resolved to a name, or the adjudicated `event_type` enum — the `reason`, `evidence` and
/// `adjudication_raw` columns of that table are deliberately never selected.
///
/// Returns the changes newest-first plus the TOTAL that qualified, so the renderer can name
/// what the cap dropped (the A5 rule) instead of silently truncating.
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

/// load_availability_changes reads the adjudicated availability record (mig 229) for everything
/// that moved since this entity was last read — the injury/suspension arm of the personnel block.
///
/// **Why this exists at all:** `load_personnel_changes` selects `transfer_identity_applications`
/// ONLY, so before this a correctly-woken Scout — one enqueued by
/// [`enqueue_rating_for_applied_availability`], with the debounce deliberately bypassed and the
/// model call deliberately made — arrived at a card containing ZERO availability facts. His s21
/// rule ("Availability is part of the profile… never speculate past what is recorded") then
/// correctly forbade him from inventing any, so the run produced a card no different from the
/// periodic one it had just paid to regenerate. The trigger is the last step of Scott's chain;
/// this is the step that makes the trigger worth pulling.
///
/// **Three kinds, because mig 229 kept three columns apart.** `opened` (newly ruled out),
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

/// load_availability_reports pulls the Editor's injury/suspension claims for this entity — the
/// evidence the Scout WEIGHS, as opposed to the adjudicated record he simply reports.
///
/// **This is the read Scott's ruling opened** (2026-08-23: *"Editor notices injury/suspension and
/// tags the Scout → the Scout decides the legitimacy of the report"*). It composes the loader and
/// the renderer itself rather than calling `render_packets_for_entity`, following the precedent
/// that doc names: *"Voices whose contract needs the claims as data instead… compose the loader
/// and the renderer themselves."* The block form would hand him the headline, the role line and
/// the continuity line — general packet prose, which is broader than the slice this seat is meant
/// to read.
///
/// Two things stay CODE's, and they are the reason a model can be trusted with the rest:
///
/// * **The slice** — `Voice::Scout` admits injury- and suspension-typed claims and nothing else.
///   No model is asked what it should be allowed to read (E1).
/// * **The contest marker** — `mark_contested` flags claims that say opposite things about the
///   same subject, mechanically, and marks BOTH (T3/D6). It is a POINTER, never a filter: the
///   disagreement is exactly what the Scout is being asked to judge, so collapsing it would be
///   deciding for him.
///
/// What he does with a marked pair — believe the better source, report the dispute, or leave it
/// out — is his call, which is the whole point of tagging him rather than adjudicating for him.
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
// Input components + hash — the debounce key (Provenance.input_hash), the 5th parity axis.
//
// Reproduces Go's `(*ratingProfile).inputComponents` + `hashComponents`: the canonical JSON is
// emitted exactly as `json.Marshal(map[string]any{...})` would (sorted keys, HTML-escaped strings,
// Go's shortest float form). Through s13 the bytes were IDENTICAL to the Go-era pre-image (keeping
// the cutover clean — no spurious nightly regens vs the Go-written rows); s14 deliberately ends
// that byte-parity by folding `prompt_version` into the pre-image (the narratives M4 / vibe v13 /
// momentum s6 pattern), so a version bump regenerates the fleet once through the hash itself. The
// datapoints walk the breakdown in STORED order (NOT pct-sorted — unlike the prompt). Built with a
// tiny Go-JSON value emitter over the shared `util::go_json_*` leaf encoders (the structure is
// nested: arrays of objects).
// ---------------------------------------------------------------------------

/// GoJson is a minimal JSON value whose emit reproduces Go `encoding/json` byte-for-byte for our
/// domain: object keys SORTED at emit (Go marshals maps with sorted keys), strings/floats via the
/// shared `util::go_json_*`, ints as-is, no whitespace. Only the shapes `input_components` needs.
enum GoJson {
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<GoJson>),
    Obj(Vec<(String, GoJson)>),
}

impl GoJson {
    fn emit(&self, out: &mut String) {
        match self {
            GoJson::Int(i) => out.push_str(&i.to_string()),
            GoJson::Float(f) => out.push_str(&go_json_float(*f)),
            GoJson::Str(s) => out.push_str(&go_json_string(s)),
            GoJson::Arr(items) => {
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    it.emit(out);
                }
                out.push(']');
            }
            GoJson::Obj(entries) => {
                let mut sorted: Vec<&(String, GoJson)> = entries.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0)); // Go marshals map keys in sorted order
                out.push('{');
                for (i, (k, v)) in sorted.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&go_json_string(k));
                    out.push(':');
                    v.emit(out);
                }
                out.push('}');
            }
        }
    }
}

/// input_components returns the canonical input-components JSON (its SHA-256 is `input_hash`).
/// `season`/`datapoints` are ALWAYS present; the rest (rate_standouts, composite_score, position)
/// are conditional. `pct` values are `round1`'d. Wave 5 deliberately removed the old
/// `is_specialty` flag from the pre-image, s19 removed the specialist entries
/// (`peak_label`/`peak_score`), and mig 221 dropped the flag from the stored breakdown itself — the pre-image is the z-score surface
/// the Scout actually reads. (The Go-parity note is historical: since the Step-3 cutover Rust is
/// the sole producer, and the s19 hash change regenerates every entity once, by design.)
pub fn input_components(p: &RatingProfile) -> String {
    let datapoints: Vec<GoJson> = p
        .breakdown
        .iter()
        .map(|d| {
            GoJson::Obj(vec![
                ("label".to_string(), GoJson::Str(d.label.clone())),
                ("pct".to_string(), GoJson::Float(round1(d.pct))),
            ])
        })
        .collect();

    let mut top: Vec<(String, GoJson)> = vec![
        // prompt_version joins the pre-image at s14 (the narratives M4 / vibe v13 / momentum s6
        // pattern): an s-bump changes every entity's hash once, forcing one regen as the
        // nightly next touches it.
        (
            "prompt_version".to_string(),
            GoJson::Str(RATING_PROMPT_VERSION.to_string()),
        ),
        ("season".to_string(), GoJson::Int(p.season as i64)),
        ("datapoints".to_string(), GoJson::Arr(datapoints)),
    ];

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        let rates: Vec<GoJson> = rs
            .iter()
            .map(|r| {
                GoJson::Obj(vec![
                    ("mode".to_string(), GoJson::Str(r.mode.clone())),
                    ("label".to_string(), GoJson::Str(r.label.clone())),
                    ("pct".to_string(), GoJson::Float(round1(r.pct))),
                ])
            })
            .collect();
        top.push(("rate_standouts".to_string(), GoJson::Arr(rates)));
    }
    if let Some(c) = p.composite_score {
        top.push(("composite_score".to_string(), GoJson::Float(round1(c))));
    }
    if !p.position.is_empty() {
        top.push(("position".to_string(), GoJson::Str(p.position.clone())));
    }

    let mut out = String::new();
    GoJson::Obj(top).emit(&mut out);
    out
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

/// s15/or9 descrub, simplified at s19 (composite-only): this label renders into the ANALYST's
/// prompt ("Form trend: …") and the SCOUT's context line, and its old wording ("Composite and
/// PEAK z-scores trending up") was the measured source of bookkeeping vocabulary leaking into
/// served prose — two banned-word attempts failed against it (the analyst s13 postmortem: a rule
/// cannot beat a phrase sitting in the data). The label speaks the sport's words at the source.
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

/// Fraction of the entity's scored events that forms the trajectory window (s19). The old fixed
/// `LIMIT 8` was an NBA-calibrated constant (~10% of an 82-game season) that read far too wide
/// for NFL/FOOTBALL calendars. The window now scales with how much the entity actually plays:
/// 10% of its scored events this season, clamped to [3, 16] — 3 is the slope minimum, 16 keeps
/// the read recent on long calendars.
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

    // The dynamic window (s19): how many scored events the entity has this season decides how
    // many "recent" means for it — a marker, not a verdict (the Analyst leans on it; the Scout
    // reads it as context; the Oracle is blind to it and reads their outputs instead).
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
// Output parsing — the body-only rating-commentary-v1 contract (s19), with the
// legacy marker strip kept as a serving guard.
// ---------------------------------------------------------------------------

/// RatingReply is the parsed model output: the cleaned identity-analysis body plus the
/// optional card title. The `T` in `Parser<T>`. (The divined PEAK label this used to carry
/// retired at s19; any marker line a model still emits is stripped and discarded so
/// bookkeeping vocabulary can never serve.)
#[derive(Clone, Debug)]
pub struct RatingReply {
    pub body: String,
    /// The card title (s20, mig 226): twelve words or fewer, contracted as the brief's
    /// closing line. `None` when absent — NULL renders downstream as "no headline".
    pub headline: Option<String>,
}

/// RatingParser strips any legacy marker line and cleans the body. It NEVER returns
/// `Ok(None)` (like `VibeParser`): rating has no post-model fail-closed marker — an empty body is a
/// hard error the caller raises, and the only marker is the PRE-model no-stats path.
/// Since the eval→guard migration (2026-08-19) it DOES fail closed (`Err` → retry) on the brief's
/// global invariants: bullet/Markdown decoration, product names, foreign script.
pub struct RatingParser;

/// parse_rating_body is the shape-only view (marker stripped, body cleaned). The eval gate
/// parses through THIS so a guard-violating reply still shows its prose in the side-by-side
/// and scores red on the invariant checks; production goes through [`RatingParser`], which
/// adds the fail-closed guards on top. The s20 HEADLINE line is split off here too, so the
/// shape view never mistakes a title for a section.
pub fn parse_rating_body(raw: &str) -> String {
    let (_legacy_label, raw_body) = parse_rating_commentary(raw);
    let (_headline, body) = split_rating_headline(&raw_body);
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
            if let Some(rest) = trimmed.strip_prefix("HEADLINE:").or_else(|| trimmed.strip_prefix("Headline:")) {
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
        let (_legacy_label, raw_body) = parse_rating_commentary(raw);
        let (headline, body_only) = split_rating_headline(&raw_body);
        let body = clean_commentary(&body_only);
        if let Some(p) = crate::guards::first_banned_phrase(&body, crate::guards::RATING_BODY_BANS) {
            tracing::warn!(guard = "rating_body_ban", phrase = p, "rating body rejected");
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
        // The title shares the HOOK contract (THE TWITTER RULE — 140 characters), and it
        // FAILS OPEN — salvage, then degrade to no title, never throw the report away.
        //
        // The comment here used to say "fail-closed like every title guard", and that was never
        // true of any other seat: the Analyst salvages then drops to NULL (s18, "a junk TITLE
        // never kills it") and the Influencer salvages (v21, guards::salvage_hook). The Scout
        // was the lone hold-out, and it cost whole reports — measured 2026-08-22, a live failure
        // reading `rating: headline violates hook_colon (headline="Hornets: Elite shooter...")`.
        // An expensive, correct, fully-graded profile was discarded over a punctuation mark in
        // its title, then re-rolled at temp=0 to produce the same title again.
        //
        // A two-beat title salvages to its first beat; anything else ships with no title at all,
        // which is the same outcome an absent HEADLINE line already has, and the next generation
        // gets another go at it.
        let headline = crate::guards::settle_title("scout", headline.as_deref());
        Ok(Some(RatingReply { body, headline }))
    }
}

/// parse_rating_commentary strips a legacy marker first line ("PEAK: <label>" / "SIGIL: <label>")
/// if a model still emits one (transition tolerance — what it strips is discarded since s18/s19)
/// and returns (stripped_label, body) — the body is everything after. No marker ⇒ the whole
/// response is the body.
fn parse_rating_commentary(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if let Some(idx) = trimmed.find('\n') {
        let first_line = trimmed[..idx].trim();
        let rest = trimmed[idx + 1..].trim();
        if let Some(label) = trim_marker(first_line) {
            return (label.to_string(), rest.to_string());
        }
        return (String::new(), trimmed.to_string());
    }
    // Single-line response. A bare `PEAK: Rim protection` still has no body and remains invalid to
    // the caller, but some local models put the entire scouting report on the marker line despite
    // the old two-line instruction. Salvage that prose as the body (the stripped label is
    // discarded either way since s19): the product gets usable commentary.
    if let Some(label) = trim_marker(trimmed) {
        if looks_like_inline_commentary(label) {
            return (String::new(), label.to_string());
        }
        return (label.to_string(), String::new());
    }
    (String::new(), trimmed.to_string())
}

fn looks_like_inline_commentary(s: &str) -> bool {
    s.split_whitespace().count() > 8 || s.contains('.') || s.contains(';')
}

/// trim_marker strips the divined-label marker ("PEAK: " or the legacy "SIGIL: ") from a line.
fn trim_marker(line: &str) -> Option<&str> {
    for prefix in ["PEAK: ", "SIGIL: "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(rest.trim());
        }
    }
    None
}

/// clean_commentary trims the prose + strips a leading "Analysis:"-style label or wrapping
/// quotes/fences if one slips through. Mirrors `cleanCommentary` (each prefix applied once, in order).
fn clean_commentary(raw: &str) -> String {
    let mut s = raw.trim();
    s = s.trim_matches('`');
    s = s.trim();
    for p in ["Analysis:", "Identity:", "On-field identity:"] {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest.trim();
        }
    }
    // s21: emphasis is STRIPPED, not rejected — the Insider's is4 treatment, for the same
    // reason and with the same precedent (`guards::salvage_hook`, `util::strip_markdown_emphasis`).
    //
    // Measured on the s21 probe: the front-office report bolded every skill name it cited, 144
    // asterisks in one body, against zero for the same input under s20. `RATING_BODY_BANS` still
    // carried "**" as a hard fail, so every one of those would have bailed as `rating_body_ban` —
    // a fail-rate explosion in a seat that had none, on the same day two others were being undone.
    // Asking a model not to emit Markdown is a request; stripping it is a guarantee, and the
    // stripped body is exactly the body the report intended.
    //
    // Line by line, because the helper is written for ONE line of a labeled reply and this body
    // is three labeled sections.
    crate::guards::clean_served_prose(s)
}

// ---------------------------------------------------------------------------
// The composition: build (deterministic) → generate (model) → persist.
// ---------------------------------------------------------------------------

/// RatingBuild is the deterministic prefix of a generation. `NoStats` ⇒ no usable rating row (no
/// composite + empty breakdown) → a NULL-body marker (no model call), mirroring Go's early return.
pub enum RatingBuild {
    NoStats { season: i32 },
    Ready(Box<RatingReady>),
}

/// RatingReady carries the assembled model inputs (the parity axes) + the deterministic context the
/// persist needs. `request_body` is computed from the SAME backend + opts the call will use, so it
/// can never drift from what is POSTed.
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

/// build_rating_request runs the deterministic prefix: load the profile, then (if usable) the
/// canonical input-components + hash, the notability, `build_stat_prompt`, the s9 options, and the
/// exact wire body. NO model call — these are the parity axes (the L2 finding). The role is
/// [`Role::StatsLogic`] (rating is its first consumer). `with_enrichment` loads BOTH model-facing
/// side blocks into the prompt (production): the s12 cross-season memory card and 7.7's
/// personnel-change block. Parity/eval/input-version callers pass `false` to pin the bare byte
/// shape — neither block is in `input_components`, so the hash those callers mint is identical
/// either way. The card needs `profile.season`, which is only known here — hence a flag rather
/// than the other junctions' pre-loaded `Option<&str>`.
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
    // The off-facet + degenerate-zero + display-tier filters run before EVERYTHING derived
    // from the breakdown — the scouting decision, the prompt's datapoint list, notability,
    // rate standouts, and the input_components hash — so every consumer sees one consistent,
    // signal-only view. (The hash change regenerates affected entities once; that regen is
    // the fix shipping — the F1-era precedent.)
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
    // 7.7's personnel block travels with the memory card on the same flag and the same
    // discipline: enrichment, best-effort, never a reason to fail the item. It is deliberately
    // NOT in `input_components`/`input_hash` — the stats rail's trigger stays the rating
    // snapshot (the Analyst's storyline render, 7.8, is out of its hash for exactly this
    // reason). A transfer alone does not re-run the Scout; the next stats-driven regen carries
    // the news.
    //
    // The availability half (mig 229) rides the SAME flag and the same discipline, and is loaded
    // independently so one failing read cannot cost the other its block: a transfer record that
    // loads fine still reaches the Scout when the availability query errors, and vice versa.
    // Both stay outside `input_components`/`input_hash` — putting availability in the pre-image
    // is the obvious fix and the wrong one, because it re-mints every entity's hash fleet-wide.
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
        prompt::render_personnel_block(
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
            Ok(claims) => prompt::render_availability_reports(&claims),
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
    // s19: season-over-season movement lines — the Scout's trajectory material (labeled
    // deltas against last season's percentiles, decided in code). Same enrichment discipline
    // as the memory card: best-effort, prompt-only, outside `input_components`/`input_hash`.
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
    let built_prompt = build_stat_prompt(
        req,
        &profile,
        notability,
        memory.as_deref(),
        personnel.as_deref(),
        z_memory.as_deref(),
        form_trend.as_deref(),
        availability_reports.as_deref(),
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
pub struct RatingOutput {
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
    pub input_hash: Option<String>,
    pub exclusions: RatingExclusions,
    pub model: Option<String>, // the configured model (set even for the no-stats marker — Go parity)
    pub built_prompt: Option<String>,
    pub request_body: Option<serde_json::Value>,
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
    pub prompt_version: &'static str,
}

impl RatingOutput {
    /// provenance lifts the moat fields into the shared `Provenance` envelope. Rating debounces on
    /// `input_hash`, and markers carry the configured model instead of NULL.
    fn provenance(&self) -> Provenance {
        Provenance {
            model_version: self
                .model
                .clone()
                .expect("rating output model_version is set for persisted rows"),
            prompt_version: self.prompt_version,
            input_ids: Vec::new(),
            input_hash: self.input_hash.clone(),
            trigger_payload: None,
        }
    }
}

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
            // The NULL-body marker. Go sets Model = ollama.Model() even here (so the read path sees
            // "no profile" with provenance), unlike vibe/transfer markers.
            let model = hx.router.for_role(Role::StatsLogic).model().to_string();
            return Ok(RatingOutput {
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
                input_hash: None,
                exclusions: RatingExclusions::default(),
                model: Some(model),
                built_prompt: None,
                request_body: None,
                eval_count: None,
                wall_ms: None,
                prompt_version: RATING_PROMPT_VERSION,
            });
        }
        RatingBuild::Ready(r) => *r,
    };

    if skip_unchanged {
        if let Some((last_hash, last_prompt_version)) = last_commentary_provenance(
            hx,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            ready.season,
        )
        .await?
        {
            // Skip only when BOTH the rating snapshot (input_hash) and the contract
            // (prompt_version) are unchanged. This is what ships a persona change
            // fleet-wide: without it, entities whose stats never move (NBA/NFL in
            // July) would speak the old voice until preseason. One regeneration per entity,
            // then the row stamps the new version and the gate closes again. (Since s14 the
            // version is ALSO folded into the hash pre-image, so the hash leg alone would
            // regen a bump; the explicit version leg stays as the belt-and-braces guard.)
            if last_hash == ready.input_hash && last_prompt_version == RATING_PROMPT_VERSION {
                return Ok(RatingOutput {
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
                    input_hash: Some(ready.input_hash),
                    exclusions: ready.exclusions,
                    model: Some(ready.model_configured),
                    built_prompt: None,
                    request_body: None,
                    eval_count: None,
                    wall_ms: None,
                    prompt_version: RATING_PROMPT_VERSION,
                });
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

    Ok(RatingOutput {
        season: ready.season,
        skipped_no_stats: false,
        skipped_unchanged: false,
        body: Some(reply.body),
        headline: reply.headline,
        notability: Some(ready.notability),
        notability_components: ready.notability_components,
        rating_trajectory: Some(ready.rating_trajectory.key),
        rating_trajectory_label: ready.rating_trajectory.label,
        rating_trajectory_components: ready.rating_trajectory.components,
        input_components: ready.input_components,
        input_hash: Some(ready.input_hash),
        exclusions: ready.exclusions,
        model: Some(extracted.model),
        built_prompt: Some(extracted.built_prompt),
        request_body: Some(extracted.request_body),
        eval_count: Some(extracted.eval_count),
        wall_ms: Some(extracted.wall_ms),
        prompt_version: RATING_PROMPT_VERSION,
    })
}

/// rating_work_input_version is the durable queue fingerprint for a rating-card demand.
/// It includes the season, the PROMPT CONTRACT, and the rating input hash (or an explicit
/// marker token), so repeated enqueue attempts collapse while changed scouting input — or a
/// changed contract — reopens the outstanding row. The prompt-version leg (s11) is what lets
/// a persona change reopen an already-done queue row: `work::enqueue`'s ON CONFLICT update
/// only fires when input_version moved, so without it a quiet entity's done row would absorb
/// the enqueue and the new voice would never ship there.
pub fn rating_work_input_version(season: i32, input_hash: Option<&str>) -> String {
    format!(
        "{RATING_WORK_PREFIX}{season}:{RATING_PROMPT_VERSION}:{}",
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

/// Work-row `input_version` for a rating opened by an ADJUDICATED transfer (Scott's brief,
/// 2026-08-15: "We need the Scout to be aware of when a transfer crossed the threshold and is
/// considered concrete").
///
/// Two problems have to be solved together, and the application id solves both:
///
/// 1. **Reopening.** `work::enqueue`'s conflict policy only reopens a `done` row when the
///    `input_version` CHANGED. A transfer does not move the rating snapshot, so re-enqueuing
///    with the ordinary stats-derived version collapses into the existing row and nothing runs.
///    Keying on `application_id` makes each newly-applied move its own version — the same trick
///    `enqueue_sigil_for_transfer` plays with the persisted rumor id.
/// 2. **The debounce.** `generate_rating`'s `skip_unchanged` compares the last row's
///    `input_hash`, and personnel is deliberately NOT in that pre-image. So even a reopened row
///    would short-circuit before the model call. `RatingHandler` reads this marker and turns the
///    debounce off for exactly these items.
///
/// **Why not simply put personnel in `input_components`.** That is the obvious fix and it is the
/// wrong one: changing the hash pre-image re-mints the `input_hash` of every entity in the fleet
/// and triggers a full regeneration — the s19 tail we are still draining. This keeps the pre-image
/// byte-identical, so nobody who did not sign anybody regenerates.
pub fn rating_work_input_version_for_transfer(season: i32, application_id: i64) -> String {
    format!(
        "{RATING_WORK_PREFIX}{season}:{RATING_PROMPT_VERSION}:{RATING_WORK_TRANSFER_MARK}{application_id}"
    )
}

/// Work-row `input_version` for a rating opened by an APPLIED injury or suspension (Scott's
/// brief, 2026-08-23: "we need to make sure on an event day, the Scout is enqueued one time
/// instead of multiple").
///
/// Same two problems as the transfer helper above, plus a third that the transfer path does not
/// have — and **keying on the DAY rather than the event solves all three at once**:
///
/// 1. **Reopening.** An injury does not move the rating snapshot, so an ordinary stats-derived
///    version collapses into the existing row and nothing runs. A marker in the hash slot makes
///    this its own version.
/// 2. **The debounce.** `generate_rating`'s `skip_unchanged` would short-circuit the reopened
///    row before the model call; [`rating_work_bypasses_debounce`] turns it off for these items.
/// 3. **Once per event day.** The transfer marker keys on `application_id`, so each move is its
///    own run — right for transfers, wrong here, because a club can lose three players to knocks
///    in one afternoon and that is ONE new fact about the squad. Keying on the day means every
///    event for an entity on that date renders the SAME `input_version`, so `work::enqueue`'s
///    `WHERE input_version IS DISTINCT FROM EXCLUDED.input_version` collapses them into one row.
///    The requirement costs nothing: no debounce table, no dedup pass, no new state.
///
/// `day` must be the event's `player_availability.event_date` rendered `YYYY-MM-DD`. It is a
/// DATE in the schema on purpose — a timestamp, or a date taken from a local zone rather than a
/// fixed one, splits one event day across two versions and the collapse silently stops
/// collapsing.
pub fn rating_work_input_version_for_availability(season: i32, day: &str) -> String {
    format!(
        "{RATING_WORK_PREFIX}{season}:{RATING_PROMPT_VERSION}:{RATING_WORK_AVAIL_MARK}{day}"
    )
}

/// The marker token sitting in the `input_version`'s `input_hash` slot, if the version parses.
/// Returns the raw slot contents — a mark, a hex digest, or `no-stats`.
fn rating_work_mark(input_version: Option<&str>) -> Option<&str> {
    input_version
        .and_then(|raw| raw.strip_prefix(RATING_WORK_PREFIX))
        .and_then(|rest| rest.split_once(':'))
        .and_then(|(_, tail)| tail.split_once(':'))
        .map(|(_, hash)| hash)
}

/// True when this work row was opened by a concrete transfer rather than by moved stats.
fn rating_work_is_transfer_triggered(input_version: Option<&str>) -> bool {
    rating_work_mark(input_version).is_some_and(|h| h.starts_with(RATING_WORK_TRANSFER_MARK))
}

/// True when this work row was opened by an applied injury or suspension.
fn rating_work_is_availability_triggered(input_version: Option<&str>) -> bool {
    rating_work_mark(input_version).is_some_and(|h| h.starts_with(RATING_WORK_AVAIL_MARK))
}

/// What woke this seat, as the value `stat_summaries.trigger_type` records (mig 228 widened the
/// CHECK to admit the last two).
///
/// This is the ONLY record of which trigger produced a card — the `input_hash` deliberately does
/// not move for the non-statistical ones — so it is what makes an eval or an incident split
/// "the nightly batch wrote this" from "an injury did".
fn rating_trigger_type(input_version: Option<&str>) -> &'static str {
    // A packet-triggered row carries no mark slot to read — its whole version is the `pk:`
    // fingerprint — but it is availability by construction: the `rating` slice hashes the
    // injury/suspension claims and nothing else, so the row exists because that news moved.
    // Recording it as 'periodic' would file the Editor's tag under the nightly batch and lose
    // the one provenance signal that separates them (mig 228 widened the CHECK for exactly this).
    if rating_work_is_packet_triggered(input_version) {
        return "availability";
    }
    match rating_work_mark(input_version) {
        Some(h) if h.starts_with(RATING_WORK_TRANSFER_MARK) => "transfer",
        Some(h) if h.starts_with(RATING_WORK_AVAIL_MARK) => "availability",
        _ => "periodic",
    }
}

/// True when this row was opened by the EDITOR's packet — the `pk:` version minted by mig 225's
/// `enqueue_voices_on_packet` from `slice_fingerprints->>'rating'`.
///
/// That slice hashes the injury/suspension claims, so the row exists precisely because the
/// availability news for this entity MOVED. Nothing else can mint a `pk:` rating row.
fn rating_work_is_packet_triggered(input_version: Option<&str>) -> bool {
    input_version.is_some_and(|raw| raw.starts_with(PACKET_WORK_PREFIX))
}

/// True when the `skip_unchanged` debounce must be turned OFF for this item.
///
/// All three non-statistical triggers need it for the same reason: the fact that changed —
/// personnel, availability, the news itself — is deliberately absent from the `input_hash`
/// pre-image, so the debounce compares equal and short-circuits before the model call. Putting
/// any of them INTO `input_components` is the obvious fix and the wrong one; see the transfer
/// helper.
///
/// The packet arm is what makes the Editor's TAG work end to end: he notices, the slice moves,
/// the row reopens, and the debounce steps aside so the Scout actually gets to read the claims
/// and judge them. Without this the enqueue lands and the seat skips it — which is exactly how
/// the routing-subscription route was measured to fail before the `rating` slice existed.
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

/// last_commentary_provenance returns `(input_hash, prompt_version)` of the entity-season's
/// LATEST commentary — the nightly skip signal. Canonical latest-generation rule (F-023): take
/// the latest row regardless of nullability; a no-stats marker has a NULL input_hash → None →
/// the next run never wrongly skips against an older real commentary the marker superseded.
/// s11 widened the read from `last_commentary_hash` (hash only) to also carry prompt_version,
/// so a contract/persona change reopens the gate exactly once per entity.
async fn last_commentary_provenance(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<(String, String)>> {
    let row: Option<(Option<String>, String)> = sqlx::query_as(
        r#"
        SELECT input_hash, prompt_version FROM stat_summaries
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
    Ok(row.and_then(|(hash, pv)| hash.filter(|h| !h.is_empty()).map(|h| (h, pv))))
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
    let prov = out.provenance().with_trigger_payload(trigger_payload);
    let trigger_json = prov.trigger_payload_json("{}");
    let ncomp_json = out.notability_components.to_string();
    let trajectory_components_json = out.rating_trajectory_components.to_string();

    // s19 retired divined_peak (never written); mig 221 dropped the column and renamed the
    // trajectory trio out of the PEAK vocabulary.
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
    .bind(out.headline.as_deref()) // the card title (s20); NULL for markers/pre-bump rows
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
    insert_cognition_ledger_best_effort(
        pool,
        CognitionLedgerEntry {
            stage: "rating".to_string(),
            lens: "rating".to_string(),
            role: Role::StatsLogic.as_str().to_string(),
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.to_string(),
            pair_entity_type: None,
            pair_entity_id: None,
            trigger_type: trigger_type.to_string(),
            trigger_payload: trigger_payload.clone(),
            product_table: "stat_summaries".to_string(),
            product_row_ids: vec![product_row_id],
            model_version: prov.model_version,
            prompt_version: prov.prompt_version.to_string(),
            output_contract_version: RATING_OUTPUT_CONTRACT_VERSION.to_string(),
            input_ids: Vec::new(),
            input_hash: prov.input_hash,
            request_body: out.request_body.clone(),
            built_prompt: out.built_prompt.clone(),
            included_evidence: rating_included_evidence(out),
            excluded_evidence: rating_excluded_evidence(out),
            context_budget: serde_json::json!({
                // Read off the EXACT wire body, not restated from the constant: the reservation
                // is window-derived now (7.12), so a ledger quoting 2,000 on a 4096 host would
                // misreport the budget the call actually ran under.
                "num_predict": out.request_body.as_ref()
                    .and_then(|b| b.pointer("/options/num_predict"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(RATING_NUM_PREDICT as i64),
                "eval_count": out.eval_count,
                "wall_ms": out.wall_ms,
            }),
            parser_outcome: rating_parser_outcome(out).to_string(),
        },
    )
    .await;
    Ok(())
}

/// enqueue_rating_for_applied_transfer — the Scout's transfer trigger (Scott's brief,
/// 2026-08-15). Called by the Insider the moment an identity application reaches `applied`,
/// which is the threshold where a move stops being a rumor and becomes a roster fact.
///
/// Until now the stats rail had exactly one trigger — the rating snapshot — which made it the
/// only seat that could not react to anything else. The consequence was not a thinner brief but
/// NO brief: an entity whose stats never move (a promoted side with no scored events, an
/// offseason club) could sign three players and its scouting brief would still describe a squad
/// that no longer exists, because nothing could ask the Scout to look again.
///
/// All three affected entities are offered: the player, the club they left, and the club they
/// joined. A departure changes who a staff will face just as much as an arrival does, and the
/// personnel block already renders both sides ("signed X from Y" / "lost X to Y").
///
/// Best-effort by design — a failure to enqueue must never fail the adjudication that earned it.
/// The nightly batch remains the backstop.
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

/// enqueue_rating_for_applied_availability — the Scout's availability trigger (Scott's brief,
/// 2026-08-23: "Injuries, suspensions, transfers add to the richness, and is exactly what a real
/// Scout does"). Called when a `player_availability` row (mig 229) reaches `applied`, which is
/// the threshold where a reported knock becomes a roster fact.
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

/// RatingHandler drains the durable `rating` stage (named `peak` until mig 221).
/// It is the queue-owned form of the stats rail: generate/persist the scouting card only when
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

    // Consolidation (2026-08-20): every drain stage shares the archbox card — see
    // `stage::ARCHBOX_SLOTS` for the ceiling and the three-knob rule. Capped at 2 so one
    // long voice decode cannot take the whole card from The Editor.
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

fn rating_input_components_value(out: &RatingOutput) -> serde_json::Value {
    serde_json::from_str(&out.input_components).unwrap_or_else(|_| {
        serde_json::json!({
            "raw_input_components": &out.input_components,
        })
    })
}

fn rating_included_evidence(out: &RatingOutput) -> serde_json::Value {
    serde_json::json!({
        "input_components": rating_input_components_value(out),
        "notability": out.notability,
        "notability_components": &out.notability_components,
        "rating_trajectory": &out.rating_trajectory,
        "rating_trajectory_label": &out.rating_trajectory_label,
        "rating_trajectory_components": &out.rating_trajectory_components,
    })
}

fn rating_excluded_evidence(out: &RatingOutput) -> serde_json::Value {
    let mut excluded = Vec::new();
    if out.skipped_no_stats {
        excluded.push(serde_json::json!({
            "reason": "no_usable_rating_profile",
        }));
    }
    if out.skipped_unchanged {
        excluded.push(serde_json::json!({
            "reason": "input_hash_unchanged",
        }));
    }
    if !out.exclusions.budget_truncated_stat_labels.is_empty() {
        excluded.push(serde_json::json!({
            "reason": "budget_truncated_stat_facts",
            "dropped_count": out.exclusions.budget_truncated_stat_labels.len(),
            "dropped_stat_labels": &out.exclusions.budget_truncated_stat_labels,
            "limit": MAX_STAT_FACTS,
        }));
    }
    if !out.exclusions.off_facet_stat_labels.is_empty() {
        excluded.push(serde_json::json!({
            "reason": "off_facet_position_mismatch",
            "dropped_count": out.exclusions.off_facet_stat_labels.len(),
            "dropped_stat_labels": &out.exclusions.off_facet_stat_labels,
        }));
    }
    if !out.exclusions.degenerate_zero_stat_labels.is_empty() {
        excluded.push(serde_json::json!({
            "reason": "degenerate_zero_usage_artifact",
            "dropped_count": out.exclusions.degenerate_zero_stat_labels.len(),
            "dropped_stat_labels": &out.exclusions.degenerate_zero_stat_labels,
        }));
    }
    if !out.exclusions.display_tier_stat_labels.is_empty() {
        excluded.push(serde_json::json!({
            "reason": "display_tier_retired_from_equation",
            "dropped_count": out.exclusions.display_tier_stat_labels.len(),
            "dropped_stat_labels": &out.exclusions.display_tier_stat_labels,
        }));
    }
    serde_json::json!(excluded)
}

fn rating_parser_outcome(out: &RatingOutput) -> &'static str {
    if out.skipped_no_stats {
        "no_call"
    } else {
        "parsed"
    }
}

#[cfg(test)]
mod tests;

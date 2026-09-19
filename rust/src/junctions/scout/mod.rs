//! Rating stage — The Scout's statistical report.
//!
//! Rust owns both rating shapes: the per-entity core here, and a `RatingHandler` queue stage for
//! current-season need-based rating work. `cmd/statcommentary` remains the operator/batch entry
//! point: nightly mode enqueues durable work, while explicit backfill can run the core inline.
//!
//! Postgres owns composite and percentile calculations. Rust selects and labels the evidence,
//! computes routing notability and trajectory, and supplies evidence for interpretation.
//!
//! FAIL CLOSED: rating's ONLY marker is the PRE-model no-stats path (no usable rating row → a
//! NULL-body marker, like vibe's no-corpus marker). There is no post-model fail-closed marker — an
//! empty model body is a hard error (the work fails + retries), never a served row.
//!
//! Missing measurements and ranks remain unknown; character and canvas own the writing.

use crate::composition::memories::{self, MemoryRequest, Mission};

use crate::runtime::harness::{Generation, GenerationCall, Harness, Parser};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::providers::ollama::GenerateOptions;
use crate::runtime::route::Role;
use crate::runtime::stage::StageHandler;
use crate::runtime::util::{hash_components, round1};
use crate::runtime::work::{Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap, HashSet};
use tracing::{debug, warn};

mod inputs;
pub use crate::composition::characters::scout::{RATING_PROMPT_VERSION, RATING_SYSTEM_PROMPT};
pub use crate::evidence::personnel::{
    load_availability_changes, load_personnel_changes, load_scout_reports, AvailabilityChange,
    PersonnelChange,
};
pub use inputs::{build_stat_prompt, render_personnel_block, render_scout_reports};

/// Output contract captured separately in the diagnostic ledger.
pub const RATING_OUTPUT_CONTRACT_VERSION: &str = "rating-commentary-v5";

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
pub const RATING_NUM_PREDICT: i32 = 700;

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

/// One measured skill. `pct` is an actual eligible-cohort percentile (higher is better).
/// Missing ranks, measurements and standardized distances remain missing.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RatingDatapoint {
    /// Identity of the underlying measurement, not merely its display skill label.
    #[serde(default)]
    pub measure: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub label: String,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub z: Option<f64>,
    #[serde(default)]
    pub pct: Option<f64>,
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

/// Missing scoped ranks stay absent, never bottom-ranked.
fn null_tolerant_map<'de, D>(d: D) -> Result<HashMap<String, f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<HashMap<String, Option<f64>>> = Option::deserialize(d)?;
    Ok(opt
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect())
}

/// Scrubbed rating profile. `composite_score` comes from a numeric column cast to float8; the
/// breakdown/scoped/modes are JSONB. The breakdown's ARRAY ORDER is preserved (jsonb keeps array
/// order), which `input_components` relies on while the prompt sorts by percentile.
#[derive(Clone, Debug)]
pub struct RatingProfile {
    pub observed_at: Option<String>,
    /// Labels come from stat definitions, including units such as Minutes Per Game.
    pub sample: BTreeMap<String, f64>,
    pub league_id: Option<i32>,
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
    pub measure: String,
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
    // Recompute the player eligibility gate from source stats as well as trusting the stored
    // bundle. This keeps pre-253 rows from exposing percentiles for a one-appearance sample while
    // the additive rating-evidence migration is waiting to deploy. It is the same self-scaling
    // threshold used by migration 253. Team ratings do not use the player participation gate.
    let eligibility_select = if entity_type == "player" {
        r#"
        COALESCE((
            SELECT bool_and(
                COALESCE(NULLIF(player_stats.stats->>rt.stat_key, '')::numeric, 0)
                >= LEAST(
                    rt.min_value,
                    GREATEST(1, ceil(0.5 * COALESCE((
                        SELECT MAX(NULLIF(ps2.stats->>rt.stat_key, '')::numeric)
                        FROM public.player_stats ps2
                        WHERE ps2.sport = $1 AND ps2.season = player_stats.season
                    ), 0)))
                )
            )
            FROM public.rating_thresholds rt
            WHERE rt.sport = $1
        ), FALSE)
        "#
    } else {
        "TRUE"
    };
    // The unscoped row first (NBA/NFL carry league_id 0/NULL), else the richest league row (the
    // most-datapoints row is the main competition — domestic league over a cup).
    let q = format!(
        r#"
        SELECT season, {pos_select},
               rating_score::float8,
               COALESCE(rating_breakdown, '[]'::jsonb)::text,
               COALESCE(rating_scoped_ranks, '{{}}'::jsonb)::text,
               {modes_select}, NULLIF(league_id,0), updated_at::date::text,
               COALESCE((
                   SELECT jsonb_object_agg(sd.display_name, NULLIF(stats->>sd.key_name,'')::numeric)
                   FROM public.stat_definitions sd
                   WHERE sd.sport = $1 AND sd.entity_type = $4
                     AND (sd.key_name IN ('appearances','games_played','matches_played','minutes_played')
                          OR sd.key_name IN (SELECT stat_key FROM public.rating_thresholds WHERE sport=$1))
                     AND NULLIF(stats->>sd.key_name,'') IS NOT NULL
               ), '{{}}'::jsonb),
               {eligibility_select} AS rank_eligible
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
        .bind(entity_type)
        .fetch_optional(pool)
        .await
        .context("load rating profile")?
    else {
        return Ok(None);
    };

    let season: i32 = row.get(0);
    let position: String = row.get(1);
    let mut composite_score: Option<f64> = row.get(2);
    let breakdown_raw: String = row.get(3);
    let scoped_raw: String = row.get(4);
    let modes_raw: String = row.get(5);

    let mut breakdown: Vec<RatingDatapoint> =
        serde_json::from_str(&breakdown_raw).context("unmarshal rating_breakdown")?;
    // Cohort framing and per-x modes are optional enrichment.
    let mut scoped_ranks: HashMap<String, f64> =
        serde_json::from_str(&scoped_raw).unwrap_or_default();
    let mut rate_modes = parse_rate_modes(&modes_raw);
    let rank_eligible: bool = row.get(9);
    if !rank_eligible {
        composite_score = None;
        scoped_ranks.clear();
        clear_datapoint_ranks(&mut breakdown);
        for datapoints in rate_modes.values_mut() {
            clear_datapoint_ranks(datapoints);
        }
    }

    Ok(Some(RatingProfile {
        league_id: row.get(6),
        observed_at: row.get(7),
        sample: serde_json::from_value(row.get(8)).context("decode rating sample")?,
        entity_type: entity_type.to_string(),
        season,
        position,
        composite_score,
        breakdown,
        scoped_ranks,
        rate_modes,
    }))
}

fn clear_datapoint_ranks(datapoints: &mut [RatingDatapoint]) {
    for datapoint in datapoints {
        datapoint.z = None;
        datapoint.pct = None;
        datapoint.scoped_pct.clear();
    }
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

/// Routing distinctiveness (0-100) and its components. Neither this score nor its formula
/// is writing context. Rate modes contribute only their maximum, independent of ordering.
pub fn compute_notability(p: &RatingProfile) -> (i32, serde_json::Value) {
    let mut top_pct = 0.0_f64;
    let mut elite_count = 0_i64;
    for d in &p.breakdown {
        if let Some(pct) = d.pct {
            top_pct = top_pct.max(pct);
        }
        if d.pct.is_some_and(|pct| pct >= 85.0) {
            elite_count += 1;
        }
    }
    // The per-x lens counts toward the top percentile (an elite-per-36 limited-minutes player
    // earns a fuller read) but NOT toward elite_count (avoid double-counting one skill across modes).
    for dps in p.rate_modes.values() {
        for d in dps {
            if let Some(pct) = d.pct {
                top_pct = top_pct.max(pct);
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

/// Compact numeric evidence, retaining hundredths for expected-value measurements.
fn trim_float(f: f64) -> String {
    if f == f.trunc() {
        format!("{f:.0}")
    } else {
        format!("{f:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
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

fn budget_truncated_stat_labels(breakdown: &[RatingDatapoint]) -> Vec<String> {
    let shown: HashSet<String> = ordered_facts(breakdown)
        .into_iter()
        .map(|d| d.label)
        .collect();
    breakdown
        .iter()
        .filter(|d| !shown.contains(&d.label))
        .map(|d| d.label.clone())
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
            let Some(pct) = d.pct.filter(|pct| *pct >= 80.0) else {
                continue;
            };
            out.push(RateStandout {
                mode: m.clone(),
                label: d.label.clone(),
                measure: d.measure.clone(),
                pct,
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
    d.pct.is_some_and(|pct| pct >= 75.0)
}

/// signed_z is the sign-adjusted z — the one number where "+" is always the good direction
/// (`format_datapoint_evidence` renders the same value).
fn signed_z(d: &RatingDatapoint) -> Option<f64> {
    d.z.map(|z| d.sign as f64 * z)
}

/// A named weakness must be MATERIALLY bad, not merely low-percentile. Distributions that clump
/// at zero (giveaways, ground yards for a WR) map tiny raw differences onto extreme percentiles:
/// Drake London's 1 giveaway sat at the 5th percentile with a sign-adjusted z of just -0.2 —
/// statistically "poor", practically average — while Stafford's genuine giveaway problem carried
/// z -4.9. Percentile finds the candidate; z-magnitude confirms it is real.
fn is_weakness(d: &RatingDatapoint) -> bool {
    d.pct.is_some_and(|pct| pct < 50.0) && signed_z(d).is_some_and(|z| z <= -0.5)
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
    let degenerate = |d: &RatingDatapoint| {
        d.pct.is_some() && d.value == Some(0.0) && signed_z(d).is_some_and(|z| z.abs() < 0.5)
    };
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
    let mut s = format!(
        "{}: {}",
        d.label,
        d.value
            .map(trim_float)
            .unwrap_or_else(|| "unmeasured".into())
    );
    if !d.measure.is_empty() && !d.measure.eq_ignore_ascii_case(&d.label) {
        s.push_str(&format!(" ({})", d.measure));
    }
    if let Some(pct) = d.pct {
        s.push_str(&format!(", percentile {pct:.1} ({})", pct_band(pct)));
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
            .find(|r| r.label == f.label && primary.is_some_and(|d| d.measure == r.measure))
        {
            f.evidence.push_str(&format!(
                " ({} percentile {:.1})",
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
                d.pct.map(pct_band).unwrap_or("unranked")
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

/// Prior rank of the same measurement. Arithmetic direction is rendered deterministically;
/// sporting significance belongs to the writer.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SkillChange {
    pub prior_pct: f64,
    pub prior_season: i32,
    pub prior_observed_at: Option<String>,
    pub prior_sample: BTreeMap<String, f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeDirection {
    Rose,
    Fell,
    Held,
}

pub fn relative_direction(delta: f64) -> RelativeDirection {
    if delta > 1.0 {
        RelativeDirection::Rose
    } else if delta < -1.0 {
        RelativeDirection::Fell
    } else {
        RelativeDirection::Held
    }
}

fn comparison_directions(
    current: &RatingProfile,
    comparisons: Option<&BTreeMap<String, SkillChange>>,
) -> BTreeMap<String, RelativeDirection> {
    current
        .breakdown
        .iter()
        .filter_map(|datapoint| {
            let current_pct = datapoint.pct?;
            let prior_pct = comparisons?.get(&datapoint.label)?.prior_pct;
            Some((
                datapoint.label.clone(),
                relative_direction(current_pct - prior_pct),
            ))
        })
        .collect()
}

fn measurement_bands(current: &RatingProfile) -> BTreeMap<String, String> {
    current
        .breakdown
        .iter()
        .filter_map(|datapoint| {
            Some((
                datapoint.label.clone(),
                pct_band(datapoint.pct?).to_string(),
            ))
        })
        .collect()
}

fn model_prompt_profile(
    profile: &RatingProfile,
    supports_cross_season: bool,
    comparisons: Option<&BTreeMap<String, SkillChange>>,
) -> RatingProfile {
    let mut prompt_profile = profile.clone();
    if !supports_cross_season {
        prompt_profile.composite_score = None;
        prompt_profile.breakdown = ordered_facts_unbounded(&profile.breakdown)
            .into_iter()
            .take(2)
            .collect();
    } else if let Some(comparisons) = comparisons {
        let mut ranked = profile
            .breakdown
            .iter()
            .filter_map(|datapoint| {
                let current_pct = datapoint.pct?;
                let change = comparisons.get(&datapoint.label)?;
                Some((
                    datapoint.label.as_str(),
                    current_pct,
                    current_pct - change.prior_pct,
                ))
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|a, b| {
            b.2.abs()
                .partial_cmp(&a.2.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(b.0))
        });
        let mut selected = ranked
            .iter()
            .take(inputs::MAX_COMPARISON_FACTS)
            .map(|fact| fact.0)
            .collect::<HashSet<_>>();
        let mut held = ranked
            .iter()
            .filter(|fact| fact.2.abs() <= 1.0 && !selected.contains(fact.0))
            .collect::<Vec<_>>();
        held.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(b.0))
        });
        selected.extend(
            held.into_iter()
                .take(inputs::MAX_HELD_COMPARISON_FACTS)
                .map(|fact| fact.0),
        );
        prompt_profile
            .breakdown
            .retain(|datapoint| selected.contains(datapoint.label.as_str()));
    }
    prompt_profile
}

/// Join measurements by skill before rendering. A missing comparison stays unknown;
/// another skill's direction must not become this one's trajectory.
pub fn build_skill_changes(
    current: &RatingProfile,
    prior: &RatingProfile,
) -> BTreeMap<String, SkillChange> {
    if current.league_id != prior.league_id {
        return BTreeMap::new();
    }
    let prior_by_label: HashMap<&str, &RatingDatapoint> = prior
        .breakdown
        .iter()
        .map(|d| (d.label.as_str(), d))
        .collect();
    current
        .breakdown
        .iter()
        .filter_map(|d| {
            let previous = prior_by_label.get(d.label.as_str())?;
            if d.measure.is_empty() || d.measure != previous.measure {
                return None;
            }
            let prior_pct = previous.pct?;
            d.pct?;
            Some((
                d.label.clone(),
                SkillChange {
                    prior_pct,
                    prior_season: prior.season,
                    prior_observed_at: prior.observed_at.clone(),
                    prior_sample: prior.sample.clone(),
                },
            ))
        })
        .collect()
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
        .map(|d| serde_json::json!({"label": d.label, "measure":d.measure, "value":d.value, "pct": d.pct.map(round1)}))
        .collect();

    let mut components = serde_json::Map::new();
    components.insert("datapoints".into(), serde_json::json!(datapoints));
    components.insert(
        "prompt_version".into(),
        serde_json::json!(RATING_PROMPT_VERSION),
    );
    components.insert("season".into(), serde_json::json!(p.season));
    components.insert("sample".into(), serde_json::json!(p.sample));
    if let Some(date) = &p.observed_at {
        components.insert("observed_at".into(), serde_json::json!(date));
    }
    if let Some(league) = p.league_id {
        components.insert("league_id".into(), serde_json::json!(league));
    }

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        let rates: Vec<serde_json::Value> = rs
            .iter()
            .map(|r| serde_json::json!({"label": r.label, "measure": r.measure, "mode": r.mode, "pct": round1(r.pct)}))
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

/// Production parser with facts from the exact built request. Shape and global
/// prose guards remain in [`RatingParser`]; this layer catches contradictions
/// that can only be judged against this entity's selected comparison evidence.
pub struct RatingRequestParser<'a> {
    prompt: &'a str,
    directions: &'a BTreeMap<String, RelativeDirection>,
    bands: &'a BTreeMap<String, String>,
}

impl<'a> RatingRequestParser<'a> {
    pub fn new(
        prompt: &'a str,
        directions: &'a BTreeMap<String, RelativeDirection>,
        bands: &'a BTreeMap<String, String>,
    ) -> Self {
        Self {
            prompt,
            directions,
            bands,
        }
    }
}

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
    if let Ok(card) = serde_json::from_str::<crate::composition::form::CardReply>(raw.trim()) {
        return (Some(card.headline), card.body);
    }
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
        if raw.trim_start().starts_with('{') {
            serde_json::from_str::<crate::composition::form::CardReply>(raw)?;
        }
        // Split the card title off FIRST so the body checks never grade it as prose.
        let (headline, body_only) = split_rating_headline(raw);
        let body = clean_commentary(&body_only);
        crate::composition::form::validate_body(&body)?;
        if let Some(p) = crate::composition::guards::first_banned_phrase(
            &body,
            crate::composition::guards::RATING_BODY_BANS,
        ) {
            tracing::warn!(
                guard = "rating_body_ban",
                phrase = p,
                "rating body rejected"
            );
            return Err(crate::composition::form::SurfaceError(format!(
                "Body makes the unsupported inference {p:?}; remove that claim and use only retained evidence."
            ))
            .into());
        }
        if let Some(p) = crate::composition::guards::first_product_name(&body) {
            tracing::warn!(guard = "product_name", name = p, "rating body rejected");
            anyhow::bail!("rating: body names product {p:?}");
        }
        if crate::composition::guards::has_foreign_script(&body) {
            tracing::warn!(guard = "foreign_script", "rating body rejected");
            anyhow::bail!("rating: body carries a foreign-script run");
        }
        // Optional titles fail open: salvage or drop without throwing away the report.
        let headline = crate::composition::guards::settle_title("scout", headline.as_deref());
        Ok(Some(RatingReply { body, headline }))
    }
}

impl Parser<RatingReply> for RatingRequestParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<RatingReply>> {
        let Some(reply) = RatingParser.parse(raw)? else {
            return Ok(None);
        };
        if let Some((label, stated, expected)) =
            first_direction_contradiction(&reply.body, self.directions)
        {
            return Err(crate::composition::form::SurfaceError(format!(
                "Body says {label} {stated}, but the compatible percentile evidence says it {expected}. Keep the supplied arithmetic direction."
            ))
            .into());
        }
        if has_internal_form_contradiction(&reply.body) {
            return Err(crate::composition::form::SurfaceError(
                "Body describes recent form as both strong/rising and declining/falling. Keep one interpretation supported by the supplied recent-form evidence.".into(),
            )
            .into());
        }
        if let Some(error) = first_measure_association_error(&reply.body) {
            return Err(crate::composition::form::SurfaceError(error.into()).into());
        }
        if let Some(error) = first_source_shape_error(&reply.body, self.prompt) {
            return Err(crate::composition::form::SurfaceError(error.into()).into());
        }
        if let Some((label, stated, expected)) = first_band_contradiction(&reply.body, self.bands) {
            return Err(crate::composition::form::SurfaceError(format!(
                "Body calls {label} {stated}, but its supplied percentile band is {expected}. Use the supplied band."
            ))
            .into());
        }
        if let Some(height) = first_unsupported_height(&reply.body, self.prompt) {
            return Err(crate::composition::form::SurfaceError(format!(
                "Body invents height {height:?}, which is absent from the retained evidence. Remove it."
            ))
            .into());
        }
        Ok(Some(reply))
    }
}

fn first_direction_contradiction(
    body: &str,
    directions: &BTreeMap<String, RelativeDirection>,
) -> Option<(String, &'static str, &'static str)> {
    const POSITIVE: &[&str] = &[
        "improv", "rose", "risen", "rising", "increas", "gained", "stronger", "higher",
    ];
    const NEGATIVE: &[&str] = &[
        "declin", "fell", "fall", "decreas", "dropped", "dropping", "slipped", "regress",
        "worsened",
    ];
    let folded = body.to_lowercase();
    let has_rise = directions
        .values()
        .any(|direction| *direction == RelativeDirection::Rose);
    let has_fall = directions
        .values()
        .any(|direction| *direction == RelativeDirection::Fell);
    if has_rise
        && has_fall
        && [
            "consistent improvement across",
            "improvement across all",
            "improved across all",
            "all metrics improved",
        ]
        .iter()
        .any(|phrase| folded.contains(phrase))
    {
        return Some((
            "the comparison".into(),
            "only rose",
            "contains mixed directions",
        ));
    }

    for sentence in folded.split(['.', '!', '?', '\n']) {
        let has_positive = POSITIVE.iter().any(|stem| sentence.contains(stem));
        let has_negative = NEGATIVE.iter().any(|stem| sentence.contains(stem));
        if has_positive == has_negative {
            continue;
        }
        for (label, expected) in directions {
            if !sentence.contains(&label.to_lowercase()) {
                continue;
            }
            match expected {
                RelativeDirection::Rose if has_negative => {
                    return Some((label.clone(), "fell", "rose"));
                }
                RelativeDirection::Fell if has_positive => {
                    return Some((label.clone(), "rose", "fell"));
                }
                RelativeDirection::Held => {
                    return Some((label.clone(), "changed", "held"));
                }
                _ => {}
            }
        }
    }

    let clauses = folded
        .split(['.', '!', '?', ';', ',', '\n'])
        .flat_map(|sentence| sentence.split(" while "))
        .flat_map(|clause| clause.split(" whereas "))
        .flat_map(|clause| clause.split(" but "))
        .flat_map(|clause| clause.split(" and "))
        .collect::<Vec<_>>();
    for (label, expected) in directions {
        for clause in clauses
            .iter()
            .filter(|clause| mentions_direction_label(clause, label))
        {
            let has_positive = POSITIVE.iter().any(|stem| clause.contains(stem));
            let has_negative = NEGATIVE.iter().any(|stem| clause.contains(stem));
            let has_stable = ["consistent", "held", "stable", "stayed", "unchanged"]
                .iter()
                .any(|stem| clause.contains(stem));
            match expected {
                RelativeDirection::Rose if has_negative => {
                    return Some((label.clone(), "fell", "rose"));
                }
                RelativeDirection::Fell if has_positive => {
                    return Some((label.clone(), "rose", "fell"));
                }
                RelativeDirection::Rose if has_stable => {
                    return Some((label.clone(), "held", "rose"));
                }
                RelativeDirection::Fell if has_stable => {
                    return Some((label.clone(), "held", "fell"));
                }
                RelativeDirection::Held if has_positive || has_negative => {
                    return Some((label.clone(), "changed", "held"));
                }
                _ => {}
            }
        }
    }
    None
}

fn mentions_direction_label(clause: &str, label: &str) -> bool {
    let folded = label.to_lowercase();
    if clause.contains(&folded) {
        return true;
    }
    folded.split_whitespace().any(|word| {
        let stem = word.strip_suffix("ing").unwrap_or(word);
        stem.len() >= 5 && clause.contains(stem)
    })
}

fn has_internal_form_contradiction(body: &str) -> bool {
    let folded = body.to_lowercase();
    let positive = ["strong form", "good form", "upward trend", "rising form"]
        .iter()
        .any(|phrase| folded.contains(phrase));
    let negative = [
        "downward trend",
        "declining form",
        "falling form",
        "recent decline",
    ]
    .iter()
    .any(|phrase| folded.contains(phrase));
    positive && negative
}

fn first_measure_association_error(body: &str) -> Option<&'static str> {
    for claim in body.to_lowercase().split(['.', '!', '?', ';', '\n']) {
        let has_xg = claim.contains("expected goals") || claim.contains("xg");
        let has_xa = claim.contains("expected assists") || claim.contains("xa");
        let creation = ["creation", "creative", "playmaking", "assist"]
            .iter()
            .any(|term| claim.contains(term));
        let scoring = ["scoring", "goalscoring", "finishing"]
            .iter()
            .any(|term| claim.contains(term));
        if has_xg && has_xa && claim.contains("percentile") {
            return Some(
                "State xG and xA percentile evidence in separate claims so one measure cannot inherit the other's ranks.",
            );
        }
        if has_xg && creation && !has_xa {
            return Some(
                "Expected goals (xG) is shooting/scoring evidence, not creation or playmaking evidence. Remove that association or use supplied xA evidence.",
            );
        }
        if has_xa && scoring && !has_xg {
            return Some(
                "Expected assists (xA) is creation evidence, not scoring/finishing evidence. Remove that association or use supplied xG evidence.",
            );
        }
    }
    None
}

fn first_source_shape_error(body: &str, prompt: &str) -> Option<&'static str> {
    let body_folded = body.to_lowercase();
    let prompt_folded = prompt.to_lowercase();
    if prompt_folded.contains("cross-season boundary:")
        && [
            "development",
            "developed",
            "growth",
            "became better",
            "became worse",
        ]
        .iter()
        .any(|phrase| body_folded.contains(phrase))
    {
        return Some(
            "Relative percentile movement does not establish player development, growth or changed ability. Describe only the supplied movement in contribution or standing.",
        );
    }
    if prompt_folded.contains("yellow cards + 3 x red cards") && body_folded.contains("red card") {
        return Some(
            "The supplied discipline value is a weighted formula, not separate yellow/red-card counts. Do not invent its components; describe only the supplied discipline value or percentile.",
        );
    }
    if prompt_folded.contains("fewer than 10 appearances") {
        if body.chars().count() > 800 {
            return Some(
                "The thin-sample card exceeds 800 characters. Keep only current identity, the two supplied leading measurements, one attributed report detail and the no-comparison boundary.",
            );
        }
        if ["frustrat", "confidence", "morale", "motivation"]
            .iter()
            .any(|stem| body_folded.contains(stem) && !prompt_folded.contains(stem))
            || ["may influence", "could influence", "might influence"]
                .iter()
                .any(|phrase| body_folded.contains(phrase))
        {
            return Some(
                "The attributed report does not support an emotional, motivational or psychological inference. Keep the reported action or quote without inventing its effect.",
            );
        }
        let stability_word = body_folded
            .split(|c: char| !c.is_ascii_alphabetic())
            .any(|word| {
                matches!(
                    word,
                    "unchanged"
                        | "stable"
                        | "stability"
                        | "consistently"
                        | "reliable"
                        | "reliability"
                )
            });
        if stability_word
            || ["relative standing", "no change"]
                .iter()
                .any(|phrase| body_folded.contains(phrase))
        {
            return Some(
                "The thin sample has no computed cross-season direction or stability evidence. Remove claims of no change, stable standing, consistency or reliability.",
            );
        }
        for claim in body_folded.split(['.', '!', '?', ';', '\n']) {
            let actualized = claim
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|words| {
                    words[0]
                        .trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
                        .parse::<f64>()
                        .is_ok()
                        && (words[1].starts_with("game")
                            || words[1].starts_with("appearance")
                            || words[1].starts_with("minute"))
                });
            let qualified = ["recorded", "stored", "snapshot", "source sample"]
                .iter()
                .any(|term| claim.contains(term));
            if actualized && !qualified {
                return Some(
                    "The thin stored sample is source coverage, not proof of complete participation. Say recorded/stored/snapshot appearances rather than claiming the player played that many games.",
                );
            }
        }
    }
    None
}

fn first_band_contradiction(
    body: &str,
    bands: &BTreeMap<String, String>,
) -> Option<(String, String, String)> {
    const BAND_TERMS: &[&str] = &[
        "above average",
        "below average",
        "elite",
        "strong",
        "average",
        "poor",
    ];
    let folded = body.to_lowercase();
    let clauses = folded
        .split(['.', '!', '?', ';', ',', '\n'])
        .flat_map(|sentence| sentence.split(" while "))
        .flat_map(|clause| clause.split(" whereas "))
        .flat_map(|clause| clause.split(" but "))
        .flat_map(|clause| clause.split(" and "));
    for clause in clauses {
        let matched = bands
            .iter()
            .filter(|(label, _)| clause.contains(&label.to_lowercase()))
            .max_by_key(|(label, _)| label.len());
        if let Some((label, expected)) = matched {
            if let Some(stated) = BAND_TERMS.iter().find(|term| clause.contains(**term)) {
                if *stated != expected {
                    return Some((label.clone(), (*stated).into(), expected.clone()));
                }
            }
        }
    }
    None
}

fn first_unsupported_height<'a>(body: &'a str, prompt: &str) -> Option<&'a str> {
    body.split_whitespace()
        .map(|token| token.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | '(' | ')')))
        .find(|token| {
            let Some((feet, mark)) = token
                .char_indices()
                .find(|(_, character)| matches!(character, '\'' | '’'))
            else {
                return false;
            };
            let before = &token[..feet];
            let after = &token[feet + mark.len_utf8()..];
            !before.is_empty()
                && before.chars().all(|c| c.is_ascii_digit())
                && after.chars().next().is_some_and(|c| c.is_ascii_digit())
                && (after.contains('"') || after.contains('”'))
                && !prompt.contains(token)
        })
}

/// Normalize the served prose and remove an accidental wrapping code fence.
fn clean_commentary(raw: &str) -> String {
    let mut s = raw.trim();
    s = s.trim_matches('`');
    s = s.trim();
    crate::composition::guards::clean_served_prose(s)
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
    /// Complete selected package retained for audit and inspection.
    pub memories: memories::Package,
    /// Exact narrower package used to render and fingerprint this request.
    pub model_memories: memories::Package,
    pub comparison_directions: BTreeMap<String, RelativeDirection>,
    pub measurement_bands: BTreeMap<String, String>,
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

/// Build the rating request without a model call. Live evaluation and production both use
/// enrichment; the bare switch is retained for explicit diagnostic probes only.
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
    let mut memory_request =
        MemoryRequest::new(Mission::Scout, &req.entity_type, req.entity_id, &req.sport);
    memory_request.season = Some(profile.season);
    let memories = memories::load(&hx.pool, memory_request).await?;
    let supports_cross_season = inputs::supports_cross_season_comparison(&profile);
    let model_memories = if supports_cross_season {
        memories.clone()
    } else {
        memories.current_snapshot_view()?
    };
    let input_components = model_memories.with_input_components(&input_components)?;
    let (notability, notability_components) = compute_notability(&profile);
    let exclusions = RatingExclusions {
        budget_truncated_stat_labels: budget_truncated_stat_labels(&profile.breakdown),
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
    // Personnel and availability are sourced enrichment, included in the material hash.
    // Load them independently so one failure cannot erase the other block.
    let personnel = if with_enrichment && !memories.historical {
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
    // else here: sourced enrichment, included in the material hash.
    let current_reports = if with_enrichment && !memories.historical {
        match load_scout_reports(&hx.pool, &req.entity_type, req.entity_id, &req.sport).await {
            Ok(claims) => inputs::render_scout_reports(&claims),
            Err(e) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    error = %e,
                    "rating: current-report load failed (continuing without the block)"
                );
                None
            }
        }
    } else {
        None
    };
    // Season-over-season movement is decided in code and added as prompt-only enrichment.
    let comparisons = if with_enrichment && supports_cross_season {
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
                Some(build_skill_changes(&profile, &prior))
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
    let comparison_directions = comparison_directions(&profile, comparisons.as_ref());
    let prompt_profile =
        model_prompt_profile(&profile, supports_cross_season, comparisons.as_ref());
    let measurement_bands = measurement_bands(&prompt_profile);
    // The recent-form marker rides the same enrichment flag: shading context in production,
    // absent only on explicit bare diagnostic probes.
    let form_trend = if with_enrichment {
        rating_trajectory.label.as_ref().map(|label| {
            format!(
                "{label}; {} scored events",
                rating_trajectory.components["sample_size"]
            )
        })
    } else {
        None
    };
    let identity = Some(model_memories.render_for_model()?);
    let mut components: serde_json::Value = serde_json::from_str(&input_components)?;
    components["skill_changes"] = serde_json::json!(comparisons);
    components["personnel"] = serde_json::json!(personnel);
    components["current_reports"] = serde_json::json!(current_reports);
    components["recent_form"] = serde_json::json!(form_trend);
    let input_components = components.to_string();
    let input_hash = hash_components(&input_components);
    let built_prompt = build_stat_prompt(
        req,
        &prompt_profile,
        personnel.as_deref(),
        comparisons.as_ref(),
        form_trend.as_deref(),
        current_reports.as_deref(),
        identity.as_deref(),
    );
    let opts = GenerateOptions {
        system: Some(RATING_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: RATING_NUM_PREDICT,
        num_ctx: hx.voice_num_ctx,
        json_mode: false,
        format_schema: Some(crate::composition::form::card_schema(false)),
        format_schema_raw: None,
    };
    let backend = hx.router.for_role(Role::StatsLogic);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(RatingBuild::Ready(Box::new(RatingReady {
        season: profile.season,
        memories,
        model_memories,
        comparison_directions,
        measurement_bands,
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

    let grounded_parser = RatingRequestParser::new(
        &ready.built_prompt,
        &ready.comparison_directions,
        &ready.measurement_bands,
    );
    let extracted = hx
        .extract(
            Role::StatsLogic,
            &ready.built_prompt,
            &ready.opts,
            &grounded_parser,
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
        let named = crate::composition::guards::title_names_entity(t, &req.entity_name);
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
            claim_token: None,
        };
        if let Err(e) = crate::runtime::work::enqueue(pool, &item).await {
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
            claim_token: None,
        };
        if let Err(e) = crate::runtime::work::enqueue(pool, &item).await {
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
        Some(crate::runtime::stage::ARCHBOX_SLOTS)
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let season = match rating_work_season(item.input_version.as_deref()) {
            Some(season) => season,
            None => current_season(&hx.pool, &sport).await?,
        };
        let name = crate::evidence::corpus::lookup_entity_name(
            &hx.pool,
            &item.entity_type,
            entity_id,
            &sport,
        )
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
        crate::application::analyst::enqueue_momentum_if_needed(
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

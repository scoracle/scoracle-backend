//! Rating stage — the stats-rail PEAK scouting report, ported from Go (Cutover Step 2, L12).
//!
//! The Go source was the machinery spec. `rating.go` supplied the loader + deterministic
//! notability/pctBand/trimFloat/ordered-facts assembly + parse + persist. The Rust port now owns both
//! shapes: the per-entity core here, and a `PeakHandler` queue stage for current-season need-based
//! PEAK work. `cmd/statcommentary` remains the operator/batch entry point: nightly mode enqueues
//! durable PEAK work, while explicit backfill can still run the core inline for historical seasons.
//!
//! Composition (Plan §1.2 + §4): `route(StatsLogic) + extract + persist`. Rating is the FIRST
//! `Role::StatsLogic` consumer (vibe/transfers are `EmotionalNews`). The deterministic parts stay
//! where they belong — composite / T-score / the `rating_breakdown` percentiles (`pct`/`z`) are
//! Postgres-computed stored derived stats, READ here, never recomputed. The transient prompt-shaping
//! (notability, `pctBand`, `trimFloat`, ordered facts) is mirrored in Rust byte-for-byte: it is NOT a
//! stored derived stat (the rating.go comment: "like sigil's trendDir"), so by the transfers
//! precedent it lives in the stage, exactly as Go does it. The L8 BREAKTHROUGH is preserved: the
//! percentile→tier mapping (`pctBand`) is done DETERMINISTICALLY in code and fed to the model as a
//! labeled FACT, and the model only VERBALIZES the labeled tier — it never maps percentile→quality
//! itself (some local models invert this, e.g. calling a 37th-pct skill "above average").
//!
//! FAIL CLOSED: rating's ONLY marker is the PRE-model no-stats path (no usable rating row → a
//! NULL-body marker, like vibe's no-corpus marker). There is no post-model fail-closed marker — an
//! empty model body is a hard error (the work fails + retries), never a served row (Go returns an
//! error too). So `RatingParser` never returns `Ok(None)` (like `VibeParser`).
//!
//! The deterministic profile assembly, input hash, and parser stay byte-stable. The s9 prompt is
//! Rust-owned and model-neutral, with the same core invariant: the labeled tier is the truth.

use crate::harness::{EntityKey, Harness, Parser, Provenance};
use crate::ledger::{insert_cognition_ledger_best_effort, CognitionLedgerEntry};
use crate::ollama::GenerateOptions;
use crate::route::Role;
use crate::stage::StageHandler;
use crate::util::{go_json_float, go_json_string, hash_components, round1};
use crate::work::{self, Item, Stage};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Deserializer};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use tracing::debug;

/// Prompt version for the PEAK scouting-report contract.
pub const RATING_PROMPT_VERSION: &str = "s10"; // s10: z-magnitude weakness gate + degenerate-zero filter + usage-artifact rule

/// Output contract captured separately in the Phase 2 diagnostic ledger.
pub const RATING_OUTPUT_CONTRACT_VERSION: &str = "peak-commentary-v1";

/// Production rating temperature (rating.go uses 0.6 — a touch of voice on the analyst prose). The
/// parity harness overrides to 0 (the deterministic axes need no model call anyway).
pub const RATING_TEMPERATURE: f64 = 0.6;

/// Token cap for the PEAK line plus one identity paragraph.
pub const RATING_NUM_PREDICT: i32 = 1200;

/// Durable PEAK queue input_version prefix. The queue key is entity/sport-scoped for historical
/// compatibility; the season is carried in the version so the handler can drain explicit
/// current-season demands without re-resolving the wrong season.
const PEAK_WORK_PREFIX: &str = "peak:s";

/// maxStatFacts bounds the breakdown datapoints fed to the prompt. Mirrors rating.go.
const MAX_STAT_FACTS: usize = 14;

/// System prompt for the PEAK scouting-report contract.
pub const RATING_SYSTEM_PROMPT: &str = r#"Task: write the entity's PEAK scouting report from the supplied Rating Engine profile.

Voice: direct, analytical, sports-literate. No hype. Ground every claim in the supplied datapoints.

Definitions:
- COMPOSITE = how well the entity performs overall.
- Each skill gives value, percentile, tier, and z-score.
- TIER IS THE TRUTH. Do not reinterpret percentile quality yourself.
- Per-x marks can support an efficient lower-minutes or per-90 edge.
- SCOUTING DECISION is deterministic. Use it as the decision card, not a suggestion.

Output:
1. First line exactly the Required PEAK line from SCOUTING DECISION, with no extra words.
2. Then one flowing scouting-report paragraph on the next line, no headers or bullets.

PEAK rules:
- Copy the Required PEAK line exactly.
- Put nothing before the PEAK marker and no explanatory text on the PEAK line.
- If the first output line does not begin with PEAK:, the answer is invalid.
- Do not start with the skill label by itself; the PEAK: marker must be present.
- Never choose the entity name, role, team name, or a different skill as the peak.
- Never choose an average, below average, poor, or merely above-average skill as the peak.

Scouting-report rules:
- Lead with real strengths: strong and elite skills.
- Use Primary strength to stop and Primary weakness to exploit as the main scouting frame.
- Cite values and percentiles as given.
- Mention per-x support when it confirms the edge.
- Do not praise marks below the 50th percentile.
- Mention weaknesses only when a skill is below average or poor AND its z is meaningfully negative. A poor percentile with a near-zero z is a usage artifact, not a liability — do not present it as one.
- If Primary weakness to exploit says None, name no weakness at all.
- If nothing is strong or elite, say that plainly and name the best available impact with its percentile.
- End with a clear scouting verdict on what kind of player/team this is.
- Length: 2-3 sentences for modest profiles, up to 5 only for truly rich profiles.
- Never invent a number, rate, role, or skill not in the data."#;

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
    #[serde(default, deserialize_with = "null_to_default")]
    /// Legacy DB key from the old specialist-credit framing. Still tolerated on read for old rows,
    /// but Wave 5 stops emitting it into model-facing input components.
    pub is_specialty: bool,
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

/// The entity's scrubbed rating profile — mirrors `ratingProfile`. `composite_score`/`peak_score`
/// come from the numeric/float8 COLUMN (cast `::float8` on read — the sqlx numeric landmine); the
/// breakdown/scoped/modes are JSONB. The breakdown's ARRAY ORDER is preserved (jsonb keeps array
/// order), which `input_components` relies on (it walks the breakdown in stored order, unlike the
/// prompt which sorts by pct).
#[derive(Clone, Debug)]
pub struct RatingProfile {
    pub entity_type: String,
    pub season: i32,
    pub position: String, // players only ("" for teams)
    pub composite_score: Option<f64>,
    pub peak_score: Option<f64>,
    pub peak_label: String,
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
pub struct PeakTrajectory {
    pub key: String,
    pub label: Option<String>,
    pub components: serde_json::Value,
}

impl PeakTrajectory {
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutingDecisionFact {
    pub label: String,
    pub evidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutingDecision {
    pub required_peak_line: String,
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
               rating_composite_score::float8, rating_specialist_score::float8, COALESCE(rating_specialty, ''),
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
    let peak_score: Option<f64> = row.get(3);
    let peak_label: String = row.get(4);
    let breakdown_raw: String = row.get(5);
    let scoped_raw: String = row.get(6);
    let modes_raw: String = row.get(7);

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
        peak_score,
        peak_label,
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
    let mut peak = 0.0_f64;
    let mut elite_count = 0_i64;
    for d in &p.breakdown {
        if d.pct > peak {
            peak = d.pct;
        }
        if d.pct >= 85.0 {
            elite_count += 1;
        }
    }
    // The per-x lens counts toward PEAK (an elite-per-36 limited-minutes player earns a fuller read)
    // but NOT toward elite_count (avoid double-counting one skill across modes).
    for dps in p.rate_modes.values() {
        for d in dps {
            if d.pct > peak {
                peak = d.pct;
            }
        }
    }
    let comp = p.composite_score.unwrap_or(50.0); // average T-score anchor when no composite
    let score = 0.6 * peak
        + (elite_count as f64 * 10.0).min(30.0)
        + clamp_f(-10.0, 10.0, (comp - 50.0) * 0.4);
    let n = clamp_f(0.0, 100.0, score).round() as i32;
    let comps = serde_json::json!({
        "peak_pct": round1(peak),
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

/// ordered_facts returns the highest-percentile datapoints, bounded to MAX_STAT_FACTS — the prompt's
/// datapoint order. Stable sort by pct DESC (Rust `slice::sort_by` is stable, like Go `SliceStable`),
/// so equal-pct ties keep their stored order, matching Go given the same breakdown input order.
fn ordered_facts(breakdown: &[RatingDatapoint]) -> Vec<RatingDatapoint> {
    let mut facts = ordered_facts_unbounded(breakdown);
    facts.truncate(MAX_STAT_FACTS);
    facts
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

fn format_datapoint_evidence(d: &RatingDatapoint) -> String {
    let dz = d.sign as f64 * d.z; // sign-adjusted so + is always the good direction
    let mut s = format!(
        "{}: {} · {:.0}th pct ({}) · z {:+.1}",
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

    let required_peak_line = match primary {
        Some(d) => format!("PEAK: {}", d.label),
        None => "PEAK: No standout skill".to_string(),
    };

    let primary_strength_to_stop = primary.map(|d| decision_fact(d));
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
        required_peak_line,
        primary_strength_to_stop,
        secondary_strengths,
        primary_weakness_to_exploit,
        no_standout_reason,
    }
}

fn render_scouting_decision(d: &ScoutingDecision) -> String {
    let mut b = String::new();
    b.push_str("\nSCOUTING DECISION\n");
    b.push_str(&format!("Required PEAK line: {}\n", d.required_peak_line));
    match &d.primary_strength_to_stop {
        Some(f) => b.push_str(&format!("Primary strength to stop: {}\n", f.evidence)),
        None => {
            b.push_str("Primary strength to stop: None; no strong/elite skill exists.\n");
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
        Some(f) => b.push_str(&format!("Primary weakness to exploit: {}\n", f.evidence)),
        None => b.push_str("Primary weakness to exploit: None supplied.\n"),
    }
    if let Some(reason) = &d.no_standout_reason {
        b.push_str(&format!("Why no standout: {reason}\n"));
    }
    b
}

/// build_stat_prompt assembles the user prompt. s9 reframes this as a deterministic opposing-scout
/// decision card plus supporting datapoints: the model explains the prepared PEAK choice instead of
/// inferring the structured label from the list. The `·` (U+00B7) and `—` (U+2014) are significant
/// bytes; the tier labels are pctBand's deterministic output.
pub fn build_stat_prompt(req: &RatingReq, p: &RatingProfile, notability: i32) -> String {
    let mut b = String::new();

    let mut header = format!("{} {}", req.sport, req.entity_type);
    if !p.position.is_empty() {
        header.push_str(", ");
        header.push_str(&p.position);
    }
    b.push_str(&format!("Entity: {} ({header})\n", req.entity_name));

    b.push_str(&format!(
        "\nProfile distinctiveness: {notability}/100 (higher = more standout skills — let a richer profile earn a fuller read).\n"
    ));

    if let Some(comp) = p.composite_score {
        b.push_str(&format!(
            "\nComposite (how WELL overall — T-score, 50 = average): {comp:.0}\n"
        ));
    }

    let decision = build_scouting_decision(p);
    b.push_str(&render_scouting_decision(&decision));

    b.push_str("\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n");
    for d in ordered_facts(&p.breakdown) {
        b.push_str("- ");
        b.push_str(&format_datapoint_evidence(&d));
        b.push('\n');
    }

    let rs = collect_rate_standouts(p);
    if !rs.is_empty() {
        b.push_str("\nRate-adjusted (per-x) corroboration — these also rate elite on a per-minute / per-90 basis (so the edge is not just a counting-stat artifact of heavy minutes):\n");
        for r in &rs {
            b.push_str(&format!(
                "- [{}] {}: {:.0}th pct\n",
                r.mode.replace('_', "-"),
                r.label,
                r.pct
            ));
        }
    }

    b.push_str(&format!(
        "\nWrite the identity analysis now. Start with this exact first line and no text before it: {}\nThen write the scouting paragraph on the next line. The first output characters must be PEAK:.",
        decision.required_peak_line
    ));
    b
}

// ---------------------------------------------------------------------------
// Input components + hash — the debounce key (Provenance.input_hash), the 5th parity axis.
//
// Reproduces Go's `(*ratingProfile).inputComponents` + `hashComponents`: the canonical JSON is
// BYTE-IDENTICAL to `json.Marshal(map[string]any{...})` (sorted keys, HTML-escaped strings, Go's
// shortest float form), so its SHA-256 128-bit hex prefix equals Go's `input_hash` — keeping the
// cutover clean (no spurious nightly regens vs the Go-written rows). The datapoints walk the breakdown
// in STORED order (NOT pct-sorted — unlike the prompt). Built with a tiny Go-JSON value emitter over
// the shared `util::go_json_*` leaf encoders (the structure is nested: arrays of objects).
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

/// input_components returns the canonical input-components JSON — the exact bytes Go's
/// `json.Marshal(orEmptyMap(ic))` produces (its SHA-256 is `input_hash`). Mirrors
/// `(*ratingProfile).inputComponents`: `season`/`peak_label`/`datapoints` are ALWAYS present; the rest
/// (rate_standouts, composite_score, peak_score, position) are conditional. `pct` values are `round1`'d.
/// Wave 5 deliberately removes the old `is_specialty` flag from each datapoint so the scouting
/// report surfaces specialist traits from the full metric spread rather than a pre-labeled axis.
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
        ("season".to_string(), GoJson::Int(p.season as i64)),
        ("peak_label".to_string(), GoJson::Str(p.peak_label.clone())),
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
    if let Some(ps) = p.peak_score {
        top.push(("peak_score".to_string(), GoJson::Float(round1(ps))));
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

fn z_trend_phrase(key: &str) -> &'static str {
    match key {
        "rising" => "rising",
        "falling" => "falling",
        _ => "steady",
    }
}

fn z_trajectory_label(key: &str, composite_key: &str, peak_key: &str) -> String {
    if composite_key == peak_key {
        return match key {
            "rising" => "Composite and PEAK z-scores trending up over recent games".to_string(),
            "falling" => "Composite and PEAK z-scores trending down over recent games".to_string(),
            _ => "Composite and PEAK z-scores holding steady over recent games".to_string(),
        };
    }

    format!(
        "Composite z-score {}; PEAK z-score {} over recent games",
        z_trend_phrase(composite_key),
        z_trend_phrase(peak_key)
    )
}

fn rounded_series(vals: &[f64]) -> Vec<f64> {
    vals.iter().copied().map(round1).collect()
}

async fn load_peak_trajectory(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    profile: &RatingProfile,
) -> Result<PeakTrajectory> {
    let (table, id_col) = match entity_type {
        "player" => ("event_box_scores", "player_id"),
        "team" => ("event_team_stats", "team_id"),
        _ => return Ok(PeakTrajectory::steady("unknown_entity_type")),
    };

    let q = format!(
        r#"
        SELECT e.rating_composite::float8, e.rating_specialist::float8
        FROM public.{table} e
        JOIN public.fixtures f ON f.id = e.fixture_id
        WHERE e.{id_col} = $1
          AND e.sport = $2
          AND e.season = $3
          AND (e.rating_composite IS NOT NULL OR e.rating_specialist IS NOT NULL)
        ORDER BY f.start_time DESC
        LIMIT 8
        "#
    );
    let events_desc: Vec<(Option<f64>, Option<f64>)> = sqlx::query_as(&q)
        .bind(entity_id)
        .bind(sport)
        .bind(profile.season)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load peak trajectory {entity_type}/{entity_id}"))?;

    if events_desc.len() < 3 {
        let mut out = PeakTrajectory::steady("sparse_recent_events");
        out.components = serde_json::json!({
            "reason": "sparse_recent_events",
            "recent_event_count": events_desc.len(),
            "source": "event_rating_z_scores",
            "metrics": ["rating_composite", "rating_peak"],
        });
        return Ok(out);
    }

    let composite_desc: Vec<f64> = events_desc.iter().filter_map(|(v, _)| *v).collect();
    let peak_desc: Vec<f64> = events_desc.iter().filter_map(|(_, v)| *v).collect();
    if composite_desc.len() < 3 && peak_desc.len() < 3 {
        let mut out = PeakTrajectory::steady("sparse_z_score_events");
        out.components = serde_json::json!({
            "reason": "sparse_z_score_events",
            "recent_event_count": events_desc.len(),
            "composite_sample_size": composite_desc.len(),
            "peak_sample_size": peak_desc.len(),
            "source": "event_rating_z_scores",
            "metrics": ["rating_composite", "rating_peak"],
        });
        return Ok(out);
    }

    let mut composite_chrono = composite_desc.clone();
    composite_chrono.reverse();
    let mut peak_chrono = peak_desc.clone();
    peak_chrono.reverse();
    let composite_slope = linear_slope(&composite_chrono);
    let peak_slope = linear_slope(&peak_chrono);
    let combined_slope = match (!composite_desc.is_empty(), !peak_desc.is_empty()) {
        (true, true) => (composite_slope + peak_slope) / 2.0,
        (true, false) => composite_slope,
        (false, true) => peak_slope,
        (false, false) => 0.0,
    };
    let key = trajectory_key(combined_slope).to_string();
    let composite_key = trajectory_key(composite_slope);
    let peak_key = trajectory_key(peak_slope);
    let label = Some(z_trajectory_label(&key, composite_key, peak_key));

    Ok(PeakTrajectory {
        key,
        label,
        components: serde_json::json!({
            "source": "event_rating_z_scores",
            "metrics": ["rating_composite", "rating_peak"],
            "recent_event_count": events_desc.len(),
            "composite_sample_size": composite_desc.len(),
            "peak_sample_size": peak_desc.len(),
            "combined_z_slope": round1(combined_slope),
            "composite_z_slope": round1(composite_slope),
            "peak_z_slope": round1(peak_slope),
            "latest_composite_z": composite_desc.first().copied().map(round1),
            "latest_peak_z": peak_desc.first().copied().map(round1),
            "recent_composite_z": rounded_series(&composite_desc),
            "recent_peak_z": rounded_series(&peak_desc),
        }),
    })
}

// ---------------------------------------------------------------------------
// Output parsing — mirrors parsePeakCommentary / trimMarker / cleanCommentary.
// ---------------------------------------------------------------------------

/// RatingReply is the parsed model output: the divined PEAK label (from the "PEAK: <label>" first
/// line) + the cleaned identity-analysis body. The `T` in `Parser<T>`.
#[derive(Clone, Debug)]
pub struct RatingReply {
    pub divined_peak: String, // "" if the model omitted the marker line
    pub body: String,
}

/// RatingParser splits the model reply into (divined_peak, body) and cleans the body. It NEVER returns
/// `Ok(None)` (like `VibeParser`): rating has no post-model fail-closed marker — an empty body is a
/// hard error the caller raises (Go returns an error too), and the only marker is the PRE-model
/// no-stats path. Mirrors `parsePeakCommentary` + `cleanCommentary`.
pub struct RatingParser;

impl Parser<RatingReply> for RatingParser {
    fn parse(&self, raw: &str) -> Result<Option<RatingReply>> {
        let (divined_peak, raw_body) = parse_peak_commentary(raw);
        let body = clean_commentary(&raw_body);
        Ok(Some(RatingReply { divined_peak, body }))
    }
}

/// parse_peak_commentary extracts the divined peak label from the first line ("PEAK: <label>") and
/// returns (divined_peak, body) — the body is everything after. Omitted marker ⇒ the whole response is
/// the body, divined_peak "". The legacy "SIGIL: " prefix is still accepted (forward-only s5 rename).
fn parse_peak_commentary(raw: &str) -> (String, String) {
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
    // the two-line instruction. Salvage that prose as the body while leaving `divined_peak` empty:
    // the product gets usable commentary, and the eval can still mark PEAK-label specificity red.
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
    s.trim().to_string()
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
    pub peak_trajectory: PeakTrajectory,
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
/// [`Role::StatsLogic`] (rating is its first consumer).
pub async fn build_rating_request(
    hx: &Harness,
    req: &RatingReq,
    temperature: f64,
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
    // The off-facet + degenerate-zero filters run before EVERYTHING derived from the breakdown —
    // the scouting decision, the prompt's datapoint list, notability, rate standouts, and the
    // input_components hash — so every consumer sees one consistent, position-true view.
    // (The hash change regenerates affected players once; that regen is the fix shipping.)
    let off_facet_stat_labels = drop_off_facet_datapoints(&mut profile);
    let degenerate_zero_stat_labels = drop_degenerate_zero_datapoints(&mut profile);
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
    };
    let peak_trajectory = load_peak_trajectory(
        &hx.pool,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        &profile,
    )
    .await?;
    let built_prompt = build_stat_prompt(req, &profile, notability);
    let opts = GenerateOptions {
        system: Some(RATING_SYSTEM_PROMPT.to_string()),
        temperature: Some(temperature),
        num_predict: RATING_NUM_PREDICT,
        num_ctx: 0,
        json_mode: false,
    format_schema: None,
    };
    let backend = hx.router.for_role(Role::StatsLogic);
    let request_body = backend.request_body(&built_prompt, &opts);
    let model_configured = backend.model().to_string();

    Ok(RatingBuild::Ready(Box::new(RatingReady {
        season: profile.season,
        notability,
        notability_components,
        peak_trajectory,
        input_components,
        input_hash,
        exclusions,
        opts,
        built_prompt,
        request_body,
        model_configured,
    })))
}

/// The un-persisted result of one generation — everything the production persist (→ stat_summaries)
/// and the parity harness (→ shadow) need. The twin of `transfer::TransferPairOutput`.
#[derive(Clone, Debug)]
pub struct RatingOutput {
    pub season: i32,
    pub skipped_no_stats: bool,
    pub skipped_unchanged: bool,
    pub body: Option<String>,         // None for a marker
    pub divined_peak: Option<String>, // None when absent/empty (Go: empty ⇒ NULL)
    pub notability: Option<i32>,
    pub notability_components: serde_json::Value,
    pub peak_trajectory: Option<String>,
    pub peak_trajectory_label: Option<String>,
    pub peak_trajectory_components: serde_json::Value,
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
) -> Result<RatingOutput> {
    let ready = match build_rating_request(hx, req, temperature).await? {
        RatingBuild::NoStats { season } => {
            // The NULL-body marker. Go sets Model = ollama.Model() even here (so the read path sees
            // "no profile" with provenance), unlike vibe/transfer markers.
            let model = hx.router.for_role(Role::StatsLogic).model().to_string();
            return Ok(RatingOutput {
                season,
                skipped_no_stats: true,
                skipped_unchanged: false,
                body: None,
                divined_peak: None,
                notability: None,
                notability_components: serde_json::json!({}),
                peak_trajectory: None,
                peak_trajectory_label: None,
                peak_trajectory_components: serde_json::json!({}),
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
        if let Some(last) = last_commentary_hash(
            hx,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            ready.season,
        )
        .await?
        {
            if last == ready.input_hash {
                return Ok(RatingOutput {
                    season: ready.season,
                    skipped_no_stats: false,
                    skipped_unchanged: true,
                    body: None,
                    divined_peak: None,
                    notability: None,
                    notability_components: serde_json::json!({}),
                    peak_trajectory: Some(ready.peak_trajectory.key.clone()),
                    peak_trajectory_label: ready.peak_trajectory.label.clone(),
                    peak_trajectory_components: ready.peak_trajectory.components.clone(),
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
        divined_peak: (!reply.divined_peak.is_empty()).then_some(reply.divined_peak),
        notability: Some(ready.notability),
        notability_components: ready.notability_components,
        peak_trajectory: Some(ready.peak_trajectory.key),
        peak_trajectory_label: ready.peak_trajectory.label,
        peak_trajectory_components: ready.peak_trajectory.components,
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

/// peak_work_input_version is the durable queue fingerprint for a PEAK card demand.
/// It includes the season and the rating input hash (or an explicit marker token), so repeated
/// enqueue attempts collapse while changed scouting input reopens the outstanding row.
pub fn peak_work_input_version(season: i32, input_hash: Option<&str>) -> String {
    format!(
        "{PEAK_WORK_PREFIX}{season}:{}",
        input_hash.filter(|s| !s.is_empty()).unwrap_or("no-stats")
    )
}

fn peak_work_season(input_version: Option<&str>) -> Option<i32> {
    let raw = input_version?;
    let rest = raw.strip_prefix(PEAK_WORK_PREFIX)?;
    let (season, _) = rest.split_once(':')?;
    season.parse::<i32>().ok().filter(|s| *s > 0)
}

/// last_commentary_hash returns the input_hash of the entity-season's LATEST commentary — the nightly
/// skip signal. Canonical latest-generation rule (F-023): take the latest row regardless of
/// nullability; a no-stats marker has a NULL input_hash → None → the next run never wrongly skips
/// against an older real commentary the marker superseded. Mirrors `lastCommentaryHash`.
async fn last_commentary_hash(
    hx: &Harness,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<String>> {
    let key = EntityKey {
        entity_type: entity_type.to_string(),
        entity_id,
        sport: sport.to_string(),
        season: Some(season),
    };
    Ok(hx
        .latest_row("stat_summaries", &key, "input_hash")
        .await?
        .filter(|s| !s.is_empty()))
}

/// persist_stat_summary writes ONE row to the LIVE stat_summaries table — the scored commentary and
/// the no-stats marker, which differ only in the bound values (body/notability/divined_peak NULL for
/// the marker; model_version is set for both). Mirrors `rating.go::persist`. Written + compiles;
/// NOT run this session (offline) — its first live run is the Step-3 cutover batch bin.
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
    let peak_components_json = out.peak_trajectory_components.to_string();

    let row = sqlx::query(
        r#"
        INSERT INTO stat_summaries (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            body, notability, notability_components, input_components, input_hash,
            model_version, prompt_version, generated_at, divined_peak,
            peak_trajectory, peak_trajectory_label, peak_trajectory_components
        ) VALUES ($1,$2,$3,$4,$5,$6::jsonb, $7,$8,$9::jsonb,$10::jsonb,$11, $12,$13,NOW(),$14,
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
    .bind(notability)
    .bind(&ncomp_json)
    .bind(&out.input_components)
    .bind(prov.input_hash.as_deref())
    .bind(prov.model_version.as_str())
    .bind(prov.prompt_version)
    .bind(out.divined_peak.as_deref())
    .bind(out.peak_trajectory.as_deref())
    .bind(out.peak_trajectory_label.as_deref())
    .bind(&peak_components_json)
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
                "num_predict": RATING_NUM_PREDICT,
                "eval_count": out.eval_count,
                "wall_ms": out.wall_ms,
            }),
            parser_outcome: rating_parser_outcome(out).to_string(),
        },
    )
    .await;
    Ok(())
}

/// enqueue_sigil_for_peak re-triggers panel synthesis after a newly persisted PEAK card or marker.
/// The Sigil stage still performs its own input-hash debounce; this queue handoff is only the cheap,
/// durable demand that the PEAK pillar changed.
pub async fn enqueue_sigil_for_peak(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    out: &RatingOutput,
) -> Result<()> {
    let sig = Item {
        stage: Stage::Sigil,
        entity_type: entity_type.to_string(),
        entity_id: i64::from(entity_id),
        sport: sport.to_string(),
        input_version: Some(peak_work_input_version(
            out.season,
            out.input_hash.as_deref(),
        )),
        attempts: 0,
    };
    work::enqueue(pool, &sig).await
}

async fn current_season(pool: &PgPool, sport: &str) -> Result<i32> {
    sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("current season {sport}"))
}

/// PeakHandler drains the durable `peak` stage. It is the queue-owned form of the stats rail:
/// generate/persist the PEAK scouting card only when the rating input hash moved, then enqueue
/// Momentum as the downstream consumer of the fresh PEAK pillar.
pub struct PeakHandler;

impl PeakHandler {
    pub fn new() -> Self {
        PeakHandler
    }
}

impl Default for PeakHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StageHandler for PeakHandler {
    fn stage(&self) -> Stage {
        Stage::Peak
    }

    async fn handle(&self, hx: &Harness, item: &Item) -> Result<()> {
        let entity_id = item.entity_id_i32()?;
        let sport = item.sport.to_uppercase();
        let season = match peak_work_season(item.input_version.as_deref()) {
            Some(season) => season,
            None => current_season(&hx.pool, &sport).await?,
        };
        let name =
            crate::corpus::lookup_entity_name(&hx.pool, &item.entity_type, entity_id, &sport)
                .await?;
        let req = RatingReq {
            entity_type: item.entity_type.clone(),
            entity_id,
            entity_name: name,
            sport: sport.clone(),
            season: Some(season),
            trigger_type: "periodic".to_string(),
        };

        let out = generate_rating(hx, &req, RATING_TEMPERATURE, true).await?;
        if out.skipped_unchanged {
            debug!(
                entity_type = %item.entity_type,
                entity_id = item.entity_id,
                sport = %sport,
                season = out.season,
                "peak: skipped unchanged rating input"
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
        crate::momentum::enqueue_momentum_if_needed(hx, &item.entity_type, entity_id, &sport)
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
        "peak_trajectory": &out.peak_trajectory,
        "peak_trajectory_label": &out.peak_trajectory_label,
        "peak_trajectory_components": &out.peak_trajectory_components,
        "divined_peak": &out.divined_peak,
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
mod tests {
    use super::*;

    fn dp(
        label: &str,
        value: f64,
        z: f64,
        pct: f64,
        sign: i32,
        specialty: bool,
    ) -> RatingDatapoint {
        RatingDatapoint {
            label: label.to_string(),
            value,
            z,
            pct,
            sign,
            is_specialty: specialty,
            ..Default::default()
        }
    }

    fn req(sport: &str, entity_type: &str, name: &str) -> RatingReq {
        RatingReq {
            entity_type: entity_type.to_string(),
            entity_id: 1,
            entity_name: name.to_string(),
            sport: sport.to_string(),
            season: None,
            trigger_type: "manual".to_string(),
        }
    }

    fn profile_player() -> RatingProfile {
        let mut scoring = dp("Scoring", 24.0, 3.1, 95.0, 1, true);
        scoring.scoped_pct.insert("position".to_string(), 88.0);
        RatingProfile {
            entity_type: "player".to_string(),
            season: 2025,
            position: "Guard".to_string(),
            composite_score: Some(67.0),
            peak_score: None,
            peak_label: "Scoring".to_string(),
            breakdown: vec![scoring, dp("Defense", 2.5, -0.5, 40.0, 1, false)],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        }
    }

    fn faceted(label: &str, pct: f64, facet: &str) -> RatingDatapoint {
        let mut d = dp(label, 1.0, 0.0, pct, 1, false);
        d.facet = facet.to_string();
        d
    }

    fn nfl_profile(position: &str, breakdown: Vec<RatingDatapoint>) -> RatingProfile {
        RatingProfile {
            entity_type: "player".to_string(),
            season: 2025,
            position: position.to_string(),
            composite_score: Some(60.0),
            peak_score: None,
            peak_label: String::new(),
            breakdown,
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        }
    }

    // --- off-facet filter: an offensive player's defensive stat sheet (and vice versa) must
    // never reach the scouting decision, the prompt, or the input_hash. The Stafford/London
    // bug: 0th-pct Tackling surfacing as a QB's "primary weakness to exploit".
    // -----------------------------------------------------------------------------------------------

    #[test]
    fn off_facet_filter_drops_other_side_for_qb() {
        let mut p = nfl_profile(
            "QB",
            vec![
                faceted("Passing Yards", 90.0, "offense"),
                faceted("Tackling", 0.0, "defense"),
                faceted("Tackles For Loss", 0.0, "defense"),
                faceted("Giveaways", 20.0, "offense"),
            ],
        );
        p.rate_modes.insert(
            "per_game".to_string(),
            vec![
                faceted("Passing Yards", 91.0, "offense"),
                faceted("Tackling", 0.0, "defense"),
            ],
        );
        let dropped = drop_off_facet_datapoints(&mut p);
        assert_eq!(dropped, vec!["Tackling", "Tackles For Loss"]);
        let labels: Vec<&str> = p.breakdown.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, vec!["Passing Yards", "Giveaways"]);
        assert_eq!(p.rate_modes["per_game"].len(), 1);
    }

    #[test]
    fn off_facet_filter_drops_offense_for_cornerback_spelled_out() {
        let mut p = nfl_profile(
            "Cornerback",
            vec![
                faceted("Interceptions", 85.0, "defense"),
                faceted("Receiving Yards", 2.0, "offense"),
            ],
        );
        let dropped = drop_off_facet_datapoints(&mut p);
        assert_eq!(dropped, vec!["Receiving Yards"]);
    }

    #[test]
    fn off_facet_filter_fails_open_for_special_teams_and_unknown() {
        for pos in ["Punter", "KR", "Unknown", ""] {
            let mut p = nfl_profile(
                pos,
                vec![
                    faceted("Field Goals", 70.0, "offense"),
                    faceted("Tackling", 10.0, "defense"),
                ],
            );
            assert!(drop_off_facet_datapoints(&mut p).is_empty(), "pos={pos:?}");
            assert_eq!(p.breakdown.len(), 2, "pos={pos:?}");
        }
    }

    #[test]
    fn off_facet_filter_noops_without_nfl_facets() {
        // NBA "Guard" collides with the NFL position name, but NBA breakdowns carry
        // facet="all" (or ""), which the filter never touches — the facet VALUE gates,
        // not the position alone.
        let mut p = nfl_profile(
            "Guard",
            vec![faceted("Scoring", 95.0, "all"), faceted("Steals", 20.0, "")],
        );
        assert!(drop_off_facet_datapoints(&mut p).is_empty());
        assert_eq!(p.breakdown.len(), 2);
    }

    #[test]
    fn degenerate_zero_datapoints_drop_but_real_absences_stay() {
        // London-shaped: 0 ground yards at z -0.28 is a usage artifact -> dropped. A zero with
        // a strongly negative z (a starter with no touchdowns) is a real finding -> kept.
        let mut artifact = faceted("Ground Yards Responsible", 1.0, "offense");
        artifact.value = 0.0;
        artifact.z = -0.28;
        let mut real_absence = faceted("Touchdowns", 2.0, "offense");
        real_absence.value = 0.0;
        real_absence.z = -2.1;
        let mut p = nfl_profile("WR", vec![artifact, real_absence]);
        let dropped = drop_degenerate_zero_datapoints(&mut p);
        assert_eq!(dropped, vec!["Ground Yards Responsible"]);
        assert_eq!(p.breakdown.len(), 1);
        assert_eq!(p.breakdown[0].label, "Touchdowns");
    }

    #[test]
    fn weakness_requires_material_z_not_just_percentile() {
        // London's giveaways: 5th pct but sign-adjusted z only -0.2 -> NOT a weakness.
        let mut clumped = faceted("Giveaways", 5.3, "offense");
        clumped.value = 1.0;
        clumped.z = 0.199;
        clumped.sign = -1;
        assert!(!is_weakness(&clumped));
        // Stafford's giveaways: 0th pct at sign-adjusted z -4.9 -> emphatically a weakness.
        let mut real = faceted("Giveaways", 0.0, "offense");
        real.value = 12.0;
        real.z = 4.9023;
        real.sign = -1;
        assert!(is_weakness(&real));
        // With only artifact-grade negatives, the decision names NO weakness.
        let mut strong = faceted("Air Yards Responsible", 92.9, "offense");
        strong.value = 919.0;
        strong.z = 1.0;
        let p = nfl_profile("WR", vec![strong, clumped]);
        let d = build_scouting_decision(&p);
        assert_eq!(d.primary_weakness_to_exploit, None);
    }

    #[test]
    fn scouting_decision_weakness_is_positional_after_filter() {
        let mut giveaways = faceted("Giveaways", 20.0, "offense");
        giveaways.value = 12.0;
        giveaways.z = 2.0; // sign-adjusted -2.0: materially bad, a real weakness
        giveaways.sign = -1;
        let mut p = nfl_profile(
            "QB",
            vec![
                faceted("Passing Yards", 90.0, "offense"),
                faceted("Tackling", 0.0, "defense"),
                giveaways,
            ],
        );
        drop_off_facet_datapoints(&mut p);
        let d = build_scouting_decision(&p);
        // Without the filter the 0th-pct Tackling wins min-by-pct; with it, the weakness is
        // the worst stat the player actually plays.
        assert_eq!(
            d.primary_weakness_to_exploit.as_ref().map(|f| f.label.as_str()),
            Some("Giveaways")
        );
    }

    // --- build_stat_prompt byte-fixtures: the deterministic parity axis. The expected strings are
    // computed by hand from the Rust assembly, so prompt drift fails here (offline, no model).
    // -----------------------------------------------------------------------------------------------

    #[test]
    fn prompt_player_composite_datapoints_and_scoped_position() {
        let p = profile_player();
        let prompt = build_stat_prompt(&req("NBA", "player", "Test Player"), &p, 70);
        assert_eq!(
            prompt,
            "Entity: Test Player (NBA player, Guard)\n\
\nProfile distinctiveness: 70/100 (higher = more standout skills — let a richer profile earn a fuller read).\n\
\nComposite (how WELL overall — T-score, 50 = average): 67\n\
\nSCOUTING DECISION\n\
Required PEAK line: PEAK: Scoring\n\
Primary strength to stop: Scoring: 24 · 95th pct (elite) · z +3.1 [position: 88th, strong]\n\
Secondary strengths: None supplied.\n\
Primary weakness to exploit: Defense: 2.5 · 40th pct (below average) · z -0.5\n\
\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n\
- Scoring: 24 · 95th pct (elite) · z +3.1 [position: 88th, strong]\n\
- Defense: 2.5 · 40th pct (below average) · z -0.5\n\
\nWrite the identity analysis now. Start with this exact first line and no text before it: PEAK: Scoring\n\
Then write the scouting paragraph on the next line. The first output characters must be PEAK:."
        );
    }

    #[test]
    fn prompt_team_no_composite_no_position() {
        // Team: position "" (no ", Guard" in the header), no composite line, one datapoint.
        let p = RatingProfile {
            entity_type: "team".to_string(),
            season: 2025,
            position: String::new(),
            composite_score: None,
            peak_score: None,
            peak_label: "Defense".to_string(),
            breakdown: vec![dp("Defense", 0.38, 1.2, 78.0, 1, false)],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        };
        let prompt = build_stat_prompt(&req("FOOTBALL", "team", "Test FC"), &p, 55);
        assert_eq!(
            prompt,
            "Entity: Test FC (FOOTBALL team)\n\
\nProfile distinctiveness: 55/100 (higher = more standout skills — let a richer profile earn a fuller read).\n\
\nSCOUTING DECISION\n\
Required PEAK line: PEAK: Defense\n\
Primary strength to stop: Defense: 0.38 · 78th pct (strong) · z +1.2\n\
Secondary strengths: None supplied.\n\
Primary weakness to exploit: None supplied.\n\
\nDatapoints — value · percentile + TIER (the percentile mapped to elite/strong/above average/average/below average/poor; THIS TIER IS THE TRUTH) · z (standard deviations above the mean: the scarcity/scale of the edge; a high z is a rarer, more premium skill); [position] percentile shown when present:\n\
- Defense: 0.38 · 78th pct (strong) · z +1.2\n\
\nWrite the identity analysis now. Start with this exact first line and no text before it: PEAK: Defense\n\
Then write the scouting paragraph on the next line. The first output characters must be PEAK:."
        );
    }

    #[test]
    fn scouting_decision_requires_no_standout_when_top_is_only_above_average() {
        let p = RatingProfile {
            entity_type: "player".to_string(),
            season: 2025,
            position: "SG".to_string(),
            composite_score: Some(49.0),
            peak_score: None,
            peak_label: String::new(),
            breakdown: vec![
                dp("Spot-up shooting", 38.1, 0.5, 64.0, 1, false),
                dp("Turnovers", 3.7, -1.4, 23.0, 1, false),
            ],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        };
        let d = build_scouting_decision(&p);
        assert_eq!(d.required_peak_line, "PEAK: No standout skill");
        assert!(d.primary_strength_to_stop.is_none());
        assert_eq!(
            d.primary_weakness_to_exploit
                .as_ref()
                .map(|f| f.label.as_str()),
            Some("Turnovers")
        );
        assert!(d
            .no_standout_reason
            .as_deref()
            .is_some_and(|r| r.contains("Spot-up shooting") && r.contains("above average")));
    }

    // --- input_components canonical JSON: the input_hash pre-image (must match Go json.Marshal). -----

    #[test]
    fn input_components_matches_go_marshal_bytes() {
        // Datapoints walk the breakdown in STORED order (NOT pct-sorted): Scoring then Defense.
        // Top keys sorted: composite_score, datapoints, peak_label, position, season. Datapoint keys
        // Datapoint keys sorted: label, pct. composite round1(67.0)=67 → "67"; pct round1 → "95"/"40".
        let ic = input_components(&profile_player());
        assert_eq!(
            ic,
            r#"{"composite_score":67,"datapoints":[{"label":"Scoring","pct":95},{"label":"Defense","pct":40}],"peak_label":"Scoring","position":"Guard","season":2025}"#
        );
        // The hash is a deterministic function of those exact bytes.
        assert_eq!(hash_components(&ic), hash_components(&ic));
    }

    #[test]
    fn input_components_omits_absent_optional_keys() {
        // No composite, no position, no peak_score, no rate modes → only season/peak_label/datapoints.
        let p = RatingProfile {
            entity_type: "team".to_string(),
            season: 2024,
            position: String::new(),
            composite_score: None,
            peak_score: None,
            peak_label: String::new(),
            breakdown: vec![],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        };
        // Empty breakdown → "datapoints":[] (Go marshals a non-nil empty slice as []); peak_label "".
        assert_eq!(
            input_components(&p),
            r#"{"datapoints":[],"peak_label":"","season":2024}"#
        );
    }

    #[test]
    fn rating_datapoint_tolerates_null_like_go() {
        // A sparse datapoint with an explicit null value + a null scoped entry — Go's json.Unmarshal
        // keeps zero values; serde must too (the L12 gate caught this: player:268's "Penalties Won"
        // carries "value": null, which plain #[serde(default)] would reject).
        let d: RatingDatapoint = serde_json::from_str(
            r#"{"label":"Penalties Won","value":null,"z":0.0,"pct":12.4,"scoped_pct":{"position":11.6,"x":null}}"#,
        )
        .expect("null tolerated like Go");
        assert_eq!(d.value, 0.0);
        assert_eq!(d.pct, 12.4);
        assert_eq!(d.scoped_pct.get("position"), Some(&11.6));
        assert_eq!(d.scoped_pct.get("x"), Some(&0.0)); // null map value → 0.0 (Go parity)
    }

    // --- deterministic helpers ----------------------------------------------------------------------

    #[test]
    fn pct_band_boundaries() {
        assert_eq!(pct_band(90.0), "elite");
        assert_eq!(pct_band(89.9), "strong");
        assert_eq!(pct_band(75.0), "strong");
        assert_eq!(pct_band(60.0), "above average");
        assert_eq!(pct_band(50.0), "average");
        assert_eq!(pct_band(49.9), "below average");
        assert_eq!(pct_band(35.0), "below average");
        assert_eq!(pct_band(34.9), "poor");
        assert_eq!(pct_band(0.0), "poor");
    }

    #[test]
    fn trim_float_formats_like_go() {
        assert_eq!(trim_float(3.0), "3"); // integral → %.0f
        assert_eq!(trim_float(24.0), "24");
        assert_eq!(trim_float(0.38), "0.38"); // abs < 1 → %.2f
        assert_eq!(trim_float(0.4), "0.40");
        assert_eq!(trim_float(10.7), "10.7"); // else → %.1f
        assert_eq!(trim_float(2.5), "2.5");
        assert_eq!(trim_float(-3.0), "-3");
    }

    #[test]
    fn peak_trajectory_buckets_and_labels_recent_form() {
        assert_eq!(trajectory_key(linear_slope(&[0.1, 0.5, 1.0])), "rising");
        assert_eq!(trajectory_key(linear_slope(&[2.2, 1.7, 1.1])), "falling");
        assert_eq!(trajectory_key(linear_slope(&[0.7, 0.8, 0.75])), "steady");
        assert_eq!(
            z_trajectory_label("falling", "falling", "falling"),
            "Composite and PEAK z-scores trending down over recent games"
        );
    }

    #[test]
    fn peak_trajectory_labels_divergent_z_score_tracks() {
        assert_eq!(
            z_trajectory_label("steady", "rising", "falling"),
            "Composite z-score rising; PEAK z-score falling over recent games"
        );
    }

    #[test]
    fn compute_notability_known_case() {
        // peak 95, elite_count 1 (only Scoring ≥ 85), comp 67.
        // score = 0.6*95 + min(30, 10) + clamp(-10,10,(67-50)*0.4=6.8) = 57 + 10 + 6.8 = 73.8 → 74.
        let (n, comps) = compute_notability(&profile_player());
        assert_eq!(n, 74);
        assert_eq!(comps["peak_pct"], 95.0);
        assert_eq!(comps["elite_count"], 1);
        assert_eq!(comps["composite"], 67.0);
    }

    #[test]
    fn ordered_facts_sorts_desc_and_truncates() {
        let p = RatingProfile {
            entity_type: "player".to_string(),
            season: 2025,
            position: String::new(),
            composite_score: None,
            peak_score: None,
            peak_label: String::new(),
            breakdown: vec![
                dp("low", 1.0, 0.0, 20.0, 1, false),
                dp("high", 1.0, 0.0, 90.0, 1, false),
                dp("mid", 1.0, 0.0, 55.0, 1, false),
            ],
            scoped_ranks: HashMap::new(),
            rate_modes: HashMap::new(),
        };
        let ordered = ordered_facts(&p.breakdown);
        assert_eq!(
            ordered.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(),
            vec!["high", "mid", "low"]
        );
    }

    // --- output parsing (mirror Go parsePeakCommentary / cleanCommentary) ---------------------------

    #[test]
    fn parse_peak_commentary_splits_marker_and_body() {
        let (peak, body) = parse_peak_commentary("PEAK: Elite scoring\nA lethal scorer who...");
        assert_eq!(peak, "Elite scoring");
        assert_eq!(body, "A lethal scorer who...");
    }

    #[test]
    fn parse_peak_commentary_accepts_legacy_sigil_prefix() {
        let (peak, body) = parse_peak_commentary("SIGIL: Rim protection\nDominant inside.");
        assert_eq!(peak, "Rim protection");
        assert_eq!(body, "Dominant inside.");
    }

    #[test]
    fn parse_peak_commentary_no_marker_is_all_body() {
        let (peak, body) = parse_peak_commentary("Just prose, no marker line\nsecond line");
        assert_eq!(peak, "");
        assert_eq!(body, "Just prose, no marker line\nsecond line");
    }

    #[test]
    fn parse_peak_commentary_salvages_inline_peak_paragraph_as_body() {
        let raw = "PEAK: Nia Torres showcases elite rim protection at the 96th percentile and anchors the paint.";
        let (peak, body) = parse_peak_commentary(raw);
        assert_eq!(peak, "");
        assert_eq!(
            body,
            "Nia Torres showcases elite rim protection at the 96th percentile and anchors the paint."
        );
    }

    #[test]
    fn clean_commentary_strips_fences_and_labels() {
        assert_eq!(
            clean_commentary("`Analysis: A solid two-way wing.`"),
            "A solid two-way wing."
        );
        assert_eq!(clean_commentary("  Identity: A poacher.  "), "A poacher.");
        assert_eq!(clean_commentary("Plain prose."), "Plain prose.");
    }

    #[test]
    fn rating_parser_never_fails_closed() {
        // Even garbage parses to Some (rating's only marker is pre-model); an empty body is the
        // caller's hard error, not a served UNKNOWN.
        let reply = RatingParser
            .parse("PEAK: X\nbody")
            .unwrap()
            .expect("always Some");
        assert_eq!(reply.divined_peak, "X");
        assert_eq!(reply.body, "body");
        assert!(RatingParser.parse("").unwrap().is_some());
    }
}

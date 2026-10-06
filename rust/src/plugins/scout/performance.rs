//! Scout performance reads, selected measurements, sample limits, and standing.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap, HashSet};

use super::prompt::RATING_PROMPT_VERSION;
use crate::util::round1;

/// maxStatFacts bounds the breakdown datapoints fed to the prompt.
pub(crate) const MAX_STAT_FACTS: usize = 14;

/// The appearance count below which a cross-season comparison is not computed.
///
/// A sample thinner than this cannot support a direction claim, so the world
/// states that rather than letting the model infer one from two snapshots.
pub const MIN_CROSS_SEASON_APPEARANCES: f64 = 10.0;

/// Selection limits for cross-season measurements and held standings.
pub const MAX_COMPARISON_FACTS: usize = 4;
pub const MAX_HELD_COMPARISON_FACTS: usize = 2;

/// Whether this sample can support a cross-season comparison at all.
pub fn supports_cross_season_comparison(p: &RatingProfile) -> bool {
    sample_appearances(p).is_some_and(|n| n >= MIN_CROSS_SEASON_APPEARANCES)
}

/// The participation count this profile was computed over.
///
/// The stat-definition label varies by source ("Appearances", "Games Played",
/// "Matches Played"), so every spelling is matched rather than assuming one. A
/// sample with no recognised participation label is thin by default: a profile
/// whose size we cannot see is not a profile we will compare across seasons.
pub fn sample_appearances(p: &RatingProfile) -> Option<f64> {
    p.sample.iter().find_map(|(label, value)| {
        matches!(
            label.trim().to_ascii_lowercase().as_str(),
            "appearances" | "games played" | "games_played" | "matches played" | "matches_played"
        )
        .then_some(*value)
    })
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
    /// Raw standardized distance from the peer mean; polarity is applied using `sign`.
    pub z: Option<f64>,
    #[serde(default)]
    pub pct: Option<f64>,
    #[serde(default, deserialize_with = "null_to_default")]
    pub in_comp: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    pub in_spec: bool,
    #[serde(default, deserialize_with = "null_to_default")]
    /// SQL measurement polarity: +1 favors higher raw values, -1 lower values.
    /// Missing/invalid values have unknown polarity, not neutral quality.
    pub sign: i32,
    #[serde(default, deserialize_with = "null_to_default")]
    pub facet: String,
    /// Size of the same-measure eligible population the percentile ranked. Missing when
    /// no comparison population exists.
    #[serde(default)]
    pub cohort: Option<f64>,
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
    /// Datapoints the thin-sample selection withholds in code instead of ordering the
    /// model to ignore them (see `THIN_SAMPLE_OMITTED_LABEL`).
    pub thin_sample_omitted_stat_labels: Vec<String>,
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
        + ((comp - 50.0) * 0.4).clamp(-10.0, 10.0);
    let n = score.clamp(0.0, 100.0).round() as i32;
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

/// Return at most `MAX_STAT_FACTS` spanning the full percentile range. Keep both ends and sample
/// the middle evenly so the prompt does not become a top-N highlight reel.
pub(super) fn ordered_facts(breakdown: &[RatingDatapoint]) -> Vec<RatingDatapoint> {
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

pub(crate) fn budget_truncated_stat_labels(breakdown: &[RatingDatapoint]) -> Vec<String> {
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

/// Select at most five elite (percentile ≥ 80) measurements per rate mode.
pub(crate) fn collect_rate_standouts(p: &RatingProfile) -> Vec<RateStandout> {
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

/// signed_z is the sign-adjusted z — the one number where "+" is always the good direction
/// (`format_datapoint_evidence` renders the same value). Unknown polarity must
/// not manufacture a neutral score or silently invert the measurement.
fn signed_z(d: &RatingDatapoint) -> Option<f64> {
    match d.sign {
        -1 | 1 => d.z.filter(|z| z.is_finite()).map(|z| d.sign as f64 * z),
        _ => None,
    }
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
pub(crate) fn drop_degenerate_zero_datapoints(p: &mut RatingProfile) -> Vec<String> {
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
/// a category he does not play. Only NFL breakdowns
/// carry offense/defense facets (NBA and FOOTBALL emit facet="all"), so this no-ops for every
/// other sport, for teams, and for facet-less rows by construction. Returns the dropped
/// breakdown labels for the exclusions ledger — the selection is provable, not silent.
pub(crate) fn drop_off_facet_datapoints(p: &mut RatingProfile) -> Vec<String> {
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
pub(crate) fn drop_display_tier_datapoints(p: &mut RatingProfile) -> Vec<String> {
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

/// Prior rank of the same measurement. Arithmetic direction is rendered deterministically;
/// sporting significance belongs to the writer.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SkillChange {
    pub prior_pct: f64,
    pub prior_season: i32,
    pub prior_observed_at: Option<String>,
    pub prior_sample: BTreeMap<String, f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
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

/// Thin samples drop the Discipline datapoint in code rather than handing it to the
/// model with an order to ignore it. Participation totals are already withheld from the
/// thin-sample sample line, so this is the last selection decision the old evidence
/// boundary used to delegate to prose. Returns the omitted labels for the ledger.
pub const THIN_SAMPLE_OMITTED_LABEL: &str = "Discipline";

pub(crate) fn thin_sample_omitted_stat_labels(p: &RatingProfile) -> Vec<String> {
    if supports_cross_season_comparison(p) {
        return Vec::new();
    }
    p.breakdown
        .iter()
        .filter(|datapoint| datapoint.label == THIN_SAMPLE_OMITTED_LABEL)
        .map(|datapoint| datapoint.label.clone())
        .collect()
}

pub(crate) fn model_prompt_profile(
    profile: &RatingProfile,
    supports_cross_season: bool,
    comparisons: Option<&BTreeMap<String, SkillChange>>,
) -> RatingProfile {
    let mut prompt_profile = profile.clone();
    if !supports_cross_season {
        prompt_profile.composite_score = None;
        // The thin-sample evidence boundary omits Discipline from the selection itself
        // (see THIN_SAMPLE_OMITTED_LABEL); participation totals never reach the prompt.
        prompt_profile.breakdown = ordered_facts_unbounded(&profile.breakdown)
            .into_iter()
            .filter(|datapoint| datapoint.label != THIN_SAMPLE_OMITTED_LABEL)
            .take(2)
            .collect();
    } else if let Some(comparisons) = comparisons.filter(|changes| !changes.is_empty()) {
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
            .take(MAX_COMPARISON_FACTS)
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
                .take(MAX_HELD_COMPARISON_FACTS)
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
        .map(|d| serde_json::json!({"label": d.label, "measure":d.measure, "value":d.value, "pct": d.pct.map(round1), "sign":d.sign, "z":d.z, "cohort":d.cohort}))
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

/// The measured profile, as presented. An absent prior percentile is no supported
/// direction, which is different from a direction of "no change".
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Profile {
    pub season: i32,
    /// The sample this profile is computed over, by stat-definition label.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sample: std::collections::BTreeMap<String, f64>,
    /// The selected measurements, keyed by measure label.
    #[serde(serialize_with = "serialize_measurements")]
    pub values: Vec<MeasuredValue>,
    /// Standardized overall score, where 50 is the peer mean. Absent rather
    /// than defaulted: a missing composite is not a composite of 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composite: Option<f64>,
    /// How this profile's values may be read. `cross_season` gates the whole
    /// comparison part; without it a prior-season percentile cannot be stated.
    pub supports_cross_season: bool,
    /// A thin or single-appearance sample is coverage, not proof of participation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
}

/// One measured quantity, with everything needed to read it honestly.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct MeasuredValue {
    pub label: String,
    pub measure: String,
    /// The raw value. `None` is unmeasured, which is not zero.
    #[serde(default)]
    pub value: Option<f64>,
    /// Eligible-cohort percentile; higher is better, already polarity-adjusted.
    #[serde(default)]
    pub percentile: Option<f64>,
    /// Size of the population that percentile ranked. Absent means no comparison
    /// population exists, not a population of one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort: Option<f64>,
    /// The plugin's band for this percentile. The model may use this wording;
    /// it may not compute a different one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
    /// Prior-season percentile when a compatible comparison exists. `Some` is a
    /// supported direction; `None` is no comparison, which is not "unchanged".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_percentile: Option<f64>,
}

impl MeasuredValue {
    pub(crate) fn standing_change(&self) -> Option<RelativeDirection> {
        Some(relative_direction(
            self.percentile? - self.prior_percentile?,
        ))
    }
}

/// Send the same computed direction acceptance uses, attached to its measurement.
/// Stored parts retain the underlying percentiles; rendering never trusts a stale direction.
fn serialize_measurements<S: serde::Serializer>(
    values: &[MeasuredValue],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Compared<'a> {
        #[serde(flatten)]
        measurement: &'a MeasuredValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        standing_change: Option<RelativeDirection>,
    }
    serializer.collect_seq(values.iter().map(|measurement| Compared {
        measurement,
        standing_change: measurement.standing_change(),
    }))
}

/// A boundary on what this profile supports, in the plugin's words.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Limit {
    /// Fewer appearances than a cross-season comparison needs.
    ThinSample { appearances: f64, minimum: f64 },
    /// Participation could not be read. Not zero, and not a thin sample.
    UnknownSample { minimum: f64 },
    /// At most one recorded appearance: source coverage, not playing time.
    OneAppearance { appearances: f64 },
    /// Measurements exist but their identity is unavailable.
    WithheldIdentity,
    /// No current measurements were supplied.
    NoMeasurements,
}

/// Move an already-filtered profile into the shape the model is shown.
/// Does not select further.
pub fn profile_parts(
    profile: &RatingProfile,
    supports_cross_season: bool,
    comparisons: Option<&std::collections::BTreeMap<String, SkillChange>>,
) -> Profile {
    let values = profile
        .breakdown
        .iter()
        .map(|datapoint| MeasuredValue {
            label: datapoint.label.clone(),
            measure: datapoint.measure.clone(),
            value: datapoint.value,
            percentile: datapoint.pct,
            cohort: datapoint.cohort,
            band: datapoint.pct.map(|pct| pct_band(pct).to_string()),
            prior_percentile: supports_cross_season
                .then(|| {
                    comparisons
                        .and_then(|changes| changes.get(&datapoint.label))
                        .map(|change| change.prior_pct)
                })
                .flatten(),
        })
        .collect();
    Profile {
        season: profile.season,
        // A stored count is source coverage. Beside a thin profile it reads as
        // playing time, which `limit` exists to prevent. The count stays in provenance.
        sample: if supports_cross_season {
            profile.sample.clone()
        } else {
            profile
                .sample
                .iter()
                .filter(|(label, _)| !is_participation(label))
                .map(|(label, value)| (label.clone(), *value))
                .collect()
        },
        values,
        composite: profile.composite_score,
        supports_cross_season,
        limit: limit_for(profile, supports_cross_season),
    }
}

fn is_participation(label: &str) -> bool {
    matches!(
        label.trim().to_ascii_lowercase().as_str(),
        "appearances" | "games played" | "games_played" | "matches played" | "matches_played"
    )
}

fn limit_for(profile: &RatingProfile, supports_cross_season: bool) -> Option<Limit> {
    let appearances = sample_appearances(profile);
    if profile.breakdown.is_empty() && profile.composite_score.is_none() {
        return Some(Limit::NoMeasurements);
    }
    if profile
        .breakdown
        .iter()
        .all(|datapoint| datapoint.measure.trim().is_empty())
        && !profile.breakdown.is_empty()
    {
        return Some(Limit::WithheldIdentity);
    }
    match appearances {
        Some(n) if n <= 1.0 => Some(Limit::OneAppearance { appearances: n }),
        Some(_) if !supports_cross_season => Some(Limit::ThinSample {
            appearances: appearances.unwrap_or_default(),
            minimum: MIN_CROSS_SEASON_APPEARANCES,
        }),
        None if !supports_cross_season => Some(Limit::UnknownSample {
            minimum: MIN_CROSS_SEASON_APPEARANCES,
        }),
        _ => None,
    }
}

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

pub async fn load_rating_trajectory(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    profile: &RatingProfile,
) -> Result<RatingTrajectory> {
    if !matches!(entity_type, "player" | "team") {
        bail!("unknown entity type {entity_type:?}");
    }
    // Window, slope and category are computed here so Rust does not keep a second producer.
    // Archbox, 14 days of unchanged rows: 200/200 matched stored slope, series, window and sample.
    let row = sqlx::query(
        r#"
        WITH counted AS (
            SELECT (
                SELECT count(*)::bigint FROM public.event_box_scores e
                WHERE $4 = 'player' AND e.player_id = $1 AND e.sport = $2 AND e.season = $3
                  AND e.rating IS NOT NULL
            ) + (
                SELECT count(*)::bigint FROM public.event_team_stats e
                WHERE $4 = 'team' AND e.team_id = $1 AND e.sport = $2 AND e.season = $3
                  AND e.rating IS NOT NULL
            ) AS events_played
        ),
        windowed AS (
            SELECT events_played,
                   CASE WHEN events_played < 3 THEN 0
                        ELSE least(16, greatest(3, round(events_played::numeric * 0.10)))::int
                   END AS window_size
            FROM counted
        ),
        newest AS (
            SELECT COALESCE(array_agg(rating ORDER BY start_time DESC), ARRAY[]::float8[]) AS ratings
            FROM (
                SELECT rating, start_time
                FROM (
                    SELECT e.rating::float8 AS rating, f.start_time
                    FROM public.event_box_scores e
                    JOIN public.fixtures f ON f.id = e.fixture_id
                    WHERE $4 = 'player' AND e.player_id = $1 AND e.sport = $2 AND e.season = $3
                      AND e.rating IS NOT NULL
                      AND (SELECT window_size FROM windowed) > 0
                    UNION ALL
                    SELECT e.rating::float8, f.start_time
                    FROM public.event_team_stats e
                    JOIN public.fixtures f ON f.id = e.fixture_id
                    WHERE $4 = 'team' AND e.team_id = $1 AND e.sport = $2 AND e.season = $3
                      AND e.rating IS NOT NULL
                      AND (SELECT window_size FROM windowed) > 0
                ) raw
                ORDER BY start_time DESC
                LIMIT (SELECT window_size FROM windowed)
            ) capped
        ),
        sloped AS (
            SELECT w.events_played,
                   w.window_size,
                   COALESCE(cardinality(n.ratings), 0) AS sample_size,
                   n.ratings,
                   (
                     SELECT num / NULLIF(den, 0)
                     FROM (
                       SELECT (cardinality(chrono) - 1)::float8 / 2 AS mean_x,
                              (SELECT avg(y) FROM unnest(chrono) y) AS mean_y,
                              chrono
                       FROM (
                         SELECT array_agg(y ORDER BY ord DESC) AS chrono
                         FROM unnest(n.ratings) WITH ORDINALITY t(y, ord)
                       ) c
                     ) m
                     CROSS JOIN LATERAL (
                       SELECT sum(((ord - 1)::float8 - mean_x) * (y - mean_y)) AS num,
                              sum(((ord - 1)::float8 - mean_x) * ((ord - 1)::float8 - mean_x)) AS den
                       FROM unnest(m.chrono) WITH ORDINALITY u(y, ord)
                     ) a
                   ) AS raw_slope
            FROM windowed w
            CROSS JOIN newest n
        )
        SELECT events_played,
               window_size,
               sample_size,
               CASE WHEN sample_size < 2 OR raw_slope IS NULL THEN NULL
                    ELSE (round((raw_slope * 10)::numeric) / 10)::float8
               END AS slope,
               COALESCE((
                 SELECT jsonb_agg(round((y * 10)::numeric) / 10 ORDER BY ord)
                 FROM unnest(ratings) WITH ORDINALITY t(y, ord)
               ), '[]'::jsonb)::text AS series,
               CASE
                 WHEN events_played < 3 THEN 'sparse_recent_events'
                 WHEN sample_size < 3 THEN 'sparse_z_score_events'
                 ELSE NULL
               END AS reason,
               CASE
                 WHEN events_played < 3 OR sample_size < 3 THEN 'steady'
                 WHEN raw_slope > 0.25 THEN 'rising'
                 WHEN raw_slope < -0.25 THEN 'falling'
                 ELSE 'steady'
               END AS key,
               CASE
                 WHEN events_played < 3 OR sample_size < 3 THEN NULL
                 WHEN raw_slope > 0.25 THEN 'overall scores trending up over recent games'
                 WHEN raw_slope < -0.25 THEN 'overall scores trending down over recent games'
                 ELSE 'overall scores holding steady over recent games'
               END AS label
        FROM sloped
        "#,
    )
    .bind(entity_id)
    .bind(sport)
    .bind(profile.season)
    .bind(entity_type)
    .fetch_one(pool)
    .await
    .with_context(|| format!("load rating trajectory {entity_type}/{entity_id}"))?;

    let events_played: i64 = row.get(0);
    let window_size: i32 = row.get(1);
    let sample_size: i32 = row.get(2);
    let slope: Option<f64> = row.get(3);
    let series: String = row.get(4);
    let reason: Option<String> = row.get(5);
    let key: String = row.get(6);
    let label: Option<String> = row.get(7);
    let recent: serde_json::Value =
        serde_json::from_str(&series).context("decode trajectory series")?;
    let components = if let Some(reason) = reason {
        serde_json::json!({
            "reason": reason,
            "events_played": events_played,
            "window_pct": 0.10,
            "window_size": window_size,
            "sample_size": sample_size,
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
        })
    } else {
        serde_json::json!({
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
            "events_played": events_played,
            "window_pct": 0.10,
            "window_size": window_size,
            "sample_size": sample_size,
            "rating_z_slope": slope,
            "latest_rating_z": recent.as_array().and_then(|rows| rows.first()).cloned(),
            "recent_rating_z": recent,
        })
    };
    // Sparse paths omit window_size and sample_size when no window was opened.
    let components = if events_played < 3 {
        serde_json::json!({
            "reason": "sparse_recent_events",
            "events_played": events_played,
            "window_pct": 0.10,
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
        })
    } else {
        components
    };
    Ok(RatingTrajectory {
        key,
        label,
        components,
    })
}

/// Return the input hash from the entity-season's latest commentary. Take
/// the latest row regardless of nullability; a no-stats marker has a NULL input_hash → None →
/// the next run never wrongly skips against an older real commentary the marker superseded.
pub async fn last_commentary_input_hash(
    pool: &PgPool,
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
    .fetch_optional(pool)
    .await
    .with_context(|| format!("last commentary provenance {entity_type}/{entity_id}"))?;
    Ok(row.and_then(|(hash,)| hash.filter(|h| !h.is_empty())))
}

pub(crate) async fn current_season(pool: &PgPool, sport: &str) -> Result<i32> {
    sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("current season {sport}"))
}

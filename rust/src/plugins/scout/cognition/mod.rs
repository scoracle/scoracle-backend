//! The Scout creates a statistical report from a prepared assignment.
//!
//! Rust owns the per-entity creation core here. The application layer owns queue coordination and
//! publication; `cmd/statcommentary` remains the operator/batch entry point.
//!
//! Postgres owns composite and percentile calculations. Rust selects and labels the evidence,
//! computes routing notability and trajectory, and supplies evidence for interpretation.
//!
//! No usable profile produces an uncalled marker; explicit model abstention produces a called
//! NULL-body marker. An empty or malformed card remains an error.
//!
//! Missing measurements and ranks remain unknown; character and canvas own the writing.

use crate::studio::model::GenerateOptions;
use crate::studio::palette::{Paint, Palette};
use crate::studio::{Generation, GenerationCall, Parser, Studio};
use crate::util::round1;
use anyhow::Result;
use serde::{Deserialize, Deserializer};
use std::collections::{BTreeMap, HashMap, HashSet};

mod brief;
pub use brief::{CHARACTER, RATING_PROMPT_VERSION};

pub mod parts;
pub mod prompt;

/// The Scout's declared prose surface: a hook and a body.
///
/// The keys are this plugin's; the shape and the validator are shared with every
/// other character. The paragraph ceiling is a per-plugin recorded decision, not
/// a default — and this plugin records a NON-PARTICIPATION, for the same reason
/// the Journalist does.
///
/// Influencer's 140 characters is a writing policy for a short observation.
/// The Scout writes a multi-paragraph analytical read of a measurement profile,
/// and a 140-character paragraph cap would apply a rule the plugin has never been
/// measured against. F4b settled the identical question for the Journalist:
/// shipping a number nobody replayed is worse than recording that the rule does
/// not apply. The shared 1,200-character body ceiling still applies, and this is
/// a declared constant rather than an omission, so a later window can revisit it
/// against evidence.
pub const SCOUT_PARAGRAPH_MAX_CHARS: Option<usize> = None;

pub fn prose() -> crate::plugins::cognition::prose::Prose {
    crate::plugins::cognition::prose::Prose::new(
        &["headline", "body"],
        crate::plugins::cognition::prose::Dimensions::new(
            crate::plugins::support::form::BODY_MAX_CHARS,
            SCOUT_PARAGRAPH_MAX_CHARS,
        ),
    )
}

/// Output contract captured separately in the diagnostic ledger.
pub const RATING_OUTPUT_CONTRACT_VERSION: &str = "rating-commentary-v7-palette";

/// Prose this plugin will not serve, and why each one is a claim rather than a
/// description.
///
/// These are Scout content policy and belong to the Scout. They were in
/// `support::guards` while the Scout was their only consumer, which made a
/// per-character writing decision look like a shared mechanical invariant — the
/// same mistake `MOMENTUM_BANNED_PHRASES` makes for the Analyst, and the same fix
/// applies in Window 6.
///
/// Two families live here, and the distinction is the point:
///
/// - **internal notation** (` · `, "exact labels and bands", "printed
///   measurements") — the model narrating the package instead of the sport;
/// - **inferences from a stored sample** ("reduced minutes", "constrained role",
///   "limited playing time") — a coverage gap read as a fact about playing
///   time. This is the one the thin-sample boundary exists to prevent, and a ban
///   is a backstop for it rather than the mechanism: the world now states the
///   limit, and `Limit` carries it.
pub const RATING_BODY_BANS: &[&str] = &[
    " · ",
    "exact labels and bands",
    "mid-season",
    "printed measurements",
    "limited playing time",
    "playing time is likely limited",
    "reduced playing time",
    "reduced minutes",
    "fewer minutes",
    "lower total minutes",
    "constrained role",
    "shift in role",
    "tactical adjustments",
    "positional or tactical changes",
    "substituted early",
    "typical team averages",
    "only verified fixture",
];

/// Production rating temperature.
pub const RATING_TEMPERATURE: f64 = 0.6;

/// Token cap for the compact scouting card.
pub const RATING_NUM_PREDICT: i32 = 700;

/// maxStatFacts bounds the breakdown datapoints fed to the prompt.
pub(crate) const MAX_STAT_FACTS: usize = 14;

/// The appearance count below which a cross-season comparison is not computed.
///
/// A sample thinner than this cannot support a direction claim, so the world
/// states that rather than letting the model infer one from two snapshots.
pub const MIN_CROSS_SEASON_APPEARANCES: f64 = 10.0;

/// How many cross-season measurements a read may carry, and how many of those
/// may be held-flat. These are SELECTION limits, not presentation limits: they
/// decide which comparisons are worth the space. They were constants in the
/// retired flat-prompt builder and are kept here because the parts still apply
/// the same selection.
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

/// Subject of a Scout assignment. Durable identifiers and trigger policy stay in the application.
/// `sport_name` is the curated sport display name (e.g. "Football (Soccer)") so the model is
/// never left to guess what a sport id means; empty falls back to the raw id.
#[derive(Clone, Debug)]
pub struct Subject {
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
    pub sport_name: String,
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

/// collect_rate_standouts surfaces, per rate mode, the elite (pct ≥ 80) per-x datapoints — the lens
/// that reveals a limited-minutes player producing at an elite rate. Modes sorted for stable output
/// with byte-wise string ordering; at most five per mode.
/// Used by BOTH the prompt's rate-adjusted section AND `input_components`' rate_standouts (same output).
fn collect_rate_standouts(p: &RatingProfile) -> Vec<RateStandout> {
    collect_rate_standouts_public(p)
}

/// The same selection, named for the parts module, which presents these as a
/// part rather than as a prompt section. One selection, two presentations: a
/// second implementation would let the two disagree about which rate standouts
/// exist.
pub(crate) fn collect_rate_standouts_public(p: &RatingProfile) -> Vec<RateStandout> {
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

pub(crate) fn comparison_directions(
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

pub(crate) fn measurement_bands(current: &RatingProfile) -> BTreeMap<String, String> {
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

pub(crate) fn trajectory_window_size(events_played: i64) -> i64 {
    if events_played < TRAJECTORY_WINDOW_MIN {
        0
    } else {
        ((events_played as f64 * TRAJECTORY_WINDOW_PCT).round() as i64)
            .clamp(TRAJECTORY_WINDOW_MIN, TRAJECTORY_WINDOW_MAX)
    }
}

/// Build the recent-form material from newest-first event ratings loaded by the application.
pub(crate) fn rating_trajectory_from_events(
    events_played: i64,
    composite_desc: Vec<f64>,
) -> RatingTrajectory {
    if events_played < TRAJECTORY_WINDOW_MIN {
        let mut out = RatingTrajectory::steady("sparse_recent_events");
        out.components = serde_json::json!({
            "reason": "sparse_recent_events",
            "events_played": events_played,
            "window_pct": TRAJECTORY_WINDOW_PCT,
            "source": "event_rating_z_scores",
            "metrics": ["rating"],
        });
        return out;
    }
    let window_size = trajectory_window_size(events_played);
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
        return out;
    }
    let mut composite_chrono = composite_desc.clone();
    composite_chrono.reverse();
    let composite_slope = linear_slope(&composite_chrono);
    let key = trajectory_key(composite_slope).to_string();
    let label = Some(z_trajectory_label(&key));
    RatingTrajectory {
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
    }
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

/// An explicit JSON null is a completed pass. Empty or invalid cards remain errors.
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
    if raw.trim() == "null" {
        return String::new();
    }
    let (_headline, body) = split_rating_headline(raw);
    clean_commentary(&body)
}

/// split_rating_headline lifts the s20 card-title line out of a raw brief: the FIRST line
/// beginning `HEADLINE:` is captured (whitespace-folded; empty ⇒ None) and removed, and the
/// remaining lines are returned in order. Position-tolerant — order drift is a shape quirk,
/// never a failed generation. Markdown decoration is deliberately NOT stripped before the
/// match: a decorated title fails the brief's own plain-text guard downstream.
fn split_rating_headline(raw: &str) -> (Option<String>, String) {
    if let Ok(card) = serde_json::from_str::<crate::plugins::support::form::CardReply>(raw.trim()) {
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
        // The declared surface is the shared keyed prose map, so decoding routes
        // through the shared validator over this plugin's own keys. That is what
        // makes the `form` part a contract rather than decoration: the grammar
        // offered to the model and the surface actually accepted come from one
        // declaration, and an undeclared field or a dropped slot is refused.
        let prose = crate::plugins::scout::cognition::prose();
        let map = crate::plugins::support::form::decode_prose_map(raw, &prose.keys)?;
        // A declined body is this plugin's abstention. It is distinct from a
        // dropped slot, which `decode_prose_map` has already refused.
        let Some(body) = map.get("body").map(clean_commentary) else {
            return Ok(None);
        };
        // The two slots are different kinds of text and carry different rules.
        // The body is prose, so the shared paragraph and body ceilings apply to
        // it. The headline is a TITLE, and its contract is the hook ceiling plus
        // `settle_title`, which salvages or drops an unusable one — a bad title
        // must never cost a finished report. Running the prose rule over both
        // would let a 200-character title throw away a valid read, which is
        // exactly the failure the 2026-08-24 hook change removed.
        let mut prose_only = crate::plugins::support::form::ProseMap::new();
        prose_only.push("body", Some(body.clone()));
        // A ceiling violation is a CORRECTABLE surface, not a broken call, so it
        // is raised as `SurfaceError` — that is what the bounded correction
        // recognises. Without this the shared validator's plain error would
        // skip the retry and one over-long body would cost the whole read.
        if let Err(error) = prose_only.validate(prose.dims) {
            return Err(crate::plugins::support::form::SurfaceError(format!("{error}")).into());
        }
        crate::plugins::support::form::validate_body(&body)?;
        let headline = map.get("headline").map(clean_commentary);
        // A FILLED slot with no content in it is a contract violation under the
        // shared rule, and that applies to the title too: `"headline": "   "` is
        // a model returning a field it declined to fill, which must be `null`.
        // That is distinct from an UNUSABLE title, which `settle_title` salvages
        // or drops so a bad title never costs a finished report.
        if let Some(title) = headline.as_deref() {
            anyhow::ensure!(
                !title.trim().is_empty(),
                "`headline` is blank; a slot carries prose or is declined"
            );
        }
        if let Some(p) =
            crate::plugins::support::guards::first_banned_phrase(&body, RATING_BODY_BANS)
        {
            tracing::warn!(
                guard = "rating_body_ban",
                phrase = p,
                "rating body rejected"
            );
            return Err(crate::plugins::support::form::SurfaceError(format!(
                "Body makes the unsupported inference {p:?}; remove that claim and use only retained evidence."
            ))
            .into());
        }
        if let Some(p) = crate::plugins::support::guards::first_product_name(&body) {
            tracing::warn!(guard = "product_name", name = p, "rating body rejected");
            anyhow::bail!("rating: body names product {p:?}");
        }
        if crate::plugins::support::guards::has_foreign_script(&body) {
            tracing::warn!(guard = "foreign_script", "rating body rejected");
            anyhow::bail!("rating: body carries a foreign-script run");
        }
        // A declined or unusable title fails open: salvage or drop without
        // throwing away the report.
        let headline = crate::plugins::support::guards::settle_title("scout", headline.as_deref());
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
            return Err(crate::plugins::support::form::SurfaceError(format!(
                "Body says {label} {stated}, but the compatible percentile evidence says it {expected}. Keep the supplied arithmetic direction."
            ))
            .into());
        }
        if has_internal_form_contradiction(&reply.body) {
            return Err(crate::plugins::support::form::SurfaceError(
                "Body describes recent form as both strong/rising and declining/falling. Keep one interpretation supported by the supplied recent-form evidence.".into(),
            )
            .into());
        }
        if let Some(error) = first_measure_association_error(&reply.body) {
            return Err(crate::plugins::support::form::SurfaceError(error.into()).into());
        }
        if let Some(error) = first_source_shape_error(&reply.body, self.prompt, self.bands) {
            return Err(crate::plugins::support::form::SurfaceError(error.into()).into());
        }
        if let Some((label, stated, expected)) = first_band_contradiction(&reply.body, self.bands) {
            return Err(crate::plugins::support::form::SurfaceError(format!(
                "Body calls {label} {stated}, but its supplied percentile band is {expected}. Use the supplied band."
            ))
            .into());
        }
        if let Some(height) = first_unsupported_height(&reply.body, self.prompt) {
            return Err(crate::plugins::support::form::SurfaceError(format!(
                "Body invents height {height:?}, which is absent from the retained evidence. Remove it."
            ))
            .into());
        }
        let visible = match reply.headline.as_deref() {
            Some(headline) => format!("{}\n{headline}", reply.body),
            None => reply.body.clone(),
        };
        if let Some(number) = first_unsupported_number(&visible, self.prompt) {
            return Err(crate::plugins::support::form::SurfaceError(format!(
                "Body or headline uses numeric value {number}, which is absent from the retained evidence. Remove it or use the exact supplied measurement."
            ))
            .into());
        }
        Ok(Some(reply))
    }
}

/// Numeric claims are evidence, not decoration. Every literal emitted on the card must occur in
/// the exact prepared assignment. Comparing parsed values admits harmless formatting differences
/// such as `95` versus `95.0`, while rejecting invented ratings, deltas and percentiles.
fn first_unsupported_number(body: &str, prompt: &str) -> Option<String> {
    let supplied = numeric_literals(prompt);
    numeric_literals(body)
        .into_iter()
        .find(|(_, value)| {
            !supplied
                .iter()
                .any(|(_, known)| (known - value).abs() < 0.000_001)
        })
        .map(|(literal, _)| literal)
}

fn numeric_literals(text: &str) -> Vec<(String, f64)> {
    let chars = text.char_indices().collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < chars.len() {
        let (start, ch) = chars[cursor];
        let unary_sign = matches!(ch, '+' | '-')
            && chars
                .get(cursor + 1)
                .is_some_and(|(_, next)| next.is_ascii_digit())
            && cursor
                .checked_sub(1)
                .and_then(|index| chars.get(index))
                .is_none_or(|(_, previous)| {
                    previous.is_whitespace() || matches!(previous, '(' | ':' | '=')
                });
        if !ch.is_ascii_digit() && !unary_sign {
            cursor += 1;
            continue;
        }

        let mut end_cursor = cursor + usize::from(unary_sign);
        let mut decimal_seen = false;
        while let Some((_, next)) = chars.get(end_cursor) {
            if next.is_ascii_digit() {
                end_cursor += 1;
            } else if *next == '.'
                && !decimal_seen
                && chars
                    .get(end_cursor + 1)
                    .is_some_and(|(_, after)| after.is_ascii_digit())
            {
                decimal_seen = true;
                end_cursor += 1;
            } else {
                break;
            }
        }
        let end = chars
            .get(end_cursor)
            .map(|(index, _)| *index)
            .unwrap_or(text.len());
        let literal = &text[start..end];
        if let Ok(value) = literal.parse::<f64>() {
            out.push((literal.to_string(), value));
        }
        cursor = end_cursor.max(cursor + 1);
    }
    out
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

fn first_source_shape_error(
    body: &str,
    prompt: &str,
    bands: &BTreeMap<String, String>,
) -> Option<&'static str> {
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
                "The thin-sample card exceeds 800 characters. Select the supported playing characteristics and relevant attributed evidence, preserving the no-comparison boundary. Use identity as context.",
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
    }
    // Stability and per-measure direction need cross-time or per-measure evidence that
    // neither thin-sample shape carries (the "fewer than 10" boundary and the one-appearance
    // boundary are the same no-comparison contract).
    if prompt_folded.contains("fewer than 10 appearances")
        || prompt_folded.contains("at most one appearance")
    {
        // Stability is a cross-time claim. On a thin sample the only supplied timeline is
        // the snapshot itself, so stability language is a violation when it reaches across
        // time (seasons, prior form, "no change") — a within-snapshot description of
        // standing stays free prose.
        let temporal_refers_back = ["season", "last year", "prior", "previous", "across"]
            .iter()
            .any(|word| body_folded.contains(word));
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
        if (stability_word && temporal_refers_back)
            || ["relative standing", "no change"]
                .iter()
                .any(|phrase| body_folded.contains(phrase))
        {
            return Some(
                "The thin sample has no computed cross-season direction or stability evidence. Remove claims of no change, stable standing, consistency or reliability.",
            );
        }
        // A per-measure direction claim needs per-measure trend evidence, which no thin
        // sample carries. The supplied recent-form line is about overall scores; it does
        // not license direction verbs on a named measurement.
        let direction_verbs = [
            "improve",
            "improved",
            "improving",
            "decline",
            "declined",
            "declining",
            "weaken",
            "weakened",
            "weakening",
            "strengthen",
            "strengthened",
            "strengthening",
            "regress",
            "regressed",
            "regressing",
            "deteriorate",
            "deteriorated",
            "deteriorating",
        ];
        for claim in body_folded.split(['.', '!', '?', ';', '\n']) {
            let names_measure = bands
                .keys()
                .any(|label| claim.contains(&label.to_lowercase()));
            if names_measure && direction_verbs.iter().any(|verb| claim.contains(verb)) {
                return Some(
                    "The thin sample carries no per-measure trend evidence. Describe the named measure's current standing; do not claim it improved, declined, weakened or strengthened.",
                );
            }
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
    crate::plugins::support::guards::clean_served_prose(s)
}

// ---------------------------------------------------------------------------
// Prepared assignment -> Studio creation.
// ---------------------------------------------------------------------------

/// Application-prepared Scout work. No storage row, queue lease, or concrete model host enters
/// this contract.
pub enum RatingBuild {
    NoStats { season: i32 },
    Ready(Box<Assignment>),
}

pub struct Assignment {
    pub subject: Subject,
    pub season: i32,
    pub comparison_directions: BTreeMap<String, RelativeDirection>,
    pub measurement_bands: BTreeMap<String, String>,
    pub notability: i32,
    pub notability_components: serde_json::Value,
    pub rating_trajectory: RatingTrajectory,
    pub input_components: String,
    pub input_hash: String,
    pub exclusions: RatingExclusions,
    pub opts: GenerateOptions,
    /// The prepared world, rendered once. This is what the model reads; it is
    /// rendered here rather than in `create` so the package measured, hashed and
    /// sent are the same bytes.
    pub built_prompt: String,
    /// The same world, unrendered, so the manual, the response schema and the
    /// package are derived from one source rather than three.
    pub parts: parts::Parts,
    /// Selected output vocabulary. Retained as the plugin's approved statements
    /// and as the provenance of what it is willing to say; see [`create`].
    pub palette: Palette,
}

/// Select factual statements in the plugin. The model may choose approved phrasing;
/// no string from its response can become served prose.
pub fn rating_palette(
    subject: &Subject,
    profile: &RatingProfile,
    comparisons: Option<&BTreeMap<String, SkillChange>>,
    form_trend: Option<&str>,
) -> Option<Palette> {
    let eligible = ordered_facts_unbounded(&profile.breakdown)
        .into_iter()
        .filter(|fact| {
            fact.pct
                .is_some_and(|pct| pct.is_finite() && (0.0..=100.0).contains(&pct))
                && !fact.measure.trim().is_empty()
        })
        .collect::<Vec<_>>();
    let mut facts = eligible.iter().take(2).cloned().collect::<Vec<_>>();
    if eligible.len() > 2 {
        facts.push(eligible.last().expect("nonempty eligible facts").clone());
    }
    if facts.is_empty() {
        let score = profile.composite_score.filter(|score| score.is_finite())?;
        let entity = subject.entity_name.trim();
        return Some(Palette {
            paints: vec![Paint {
                id: "composite".into(),
                phrasings: vec![
                    format!(
                        "{entity}'s overall standardized score is {score:.0}, where 50 is average."
                    ),
                    format!(
                        "The overall standardized score for {entity} is {score:.0}; 50 is average."
                    ),
                ],
            }],
        });
    }
    let mut paints: Vec<Paint> = facts.into_iter().enumerate().map(|(index, fact)| {
        let pct = fact.pct.expect("selected percentile");
        let band = pct_band(pct);
        let population = fact.cohort.filter(|n| n.is_finite() && *n >= 1.0)
            .map(|n| format!(" among {:.0} eligible profiles", n)).unwrap_or_default();
        let measure = fact.measure.trim();
        let entity = subject.entity_name.trim();
        let suffix = comparisons
            .and_then(|items| items.get(&fact.label))
            .filter(|prior| prior.prior_pct.is_finite() && (0.0..=100.0).contains(&prior.prior_pct))
            .map(|prior| {
                let direction = match relative_direction(pct - prior.prior_pct) {
                    RelativeDirection::Rose => "rose",
                    RelativeDirection::Fell => "fell",
                    RelativeDirection::Held => "held",
                };
                format!(" Relative percentile standing {direction} versus the prior season (percentile {:.1}).", prior.prior_pct)
            }).unwrap_or_default();
        Paint { id: format!("measure_{index}"), phrasings: vec![
            format!("{entity} ranks {band} in {measure}: percentile {pct:.1}{population}.{suffix}"),
            format!("In {measure}, {entity} sits at percentile {pct:.1}{population}, a {band} standing.{suffix}"),
        ] }
    }).collect();
    if let Some(trend) = form_trend.filter(|trend| !trend.trim().is_empty()) {
        paints.push(Paint {
            id: "recent_form".into(),
            phrasings: vec![
                format!(
                    "Recent form for {}: {}.",
                    subject.entity_name.trim(),
                    trend.trim_end_matches('.')
                ),
                format!(
                    "For {}, the recent-form record reads: {}.",
                    subject.entity_name.trim(),
                    trend.trim_end_matches('.')
                ),
            ],
        });
    }
    Some(Palette { paints })
}

/// The un-persisted result of one Scout creation.
#[derive(Clone, Debug)]
pub struct RatingProduct {
    pub season: i32,
    pub skipped_no_stats: bool,
    /// The model was called and explicitly chose not to publish prose.
    pub abstained: bool,
    pub skipped_unchanged: bool,
    pub body: Option<String>,
    pub headline: Option<String>,
    pub notability: Option<i32>,
    pub notability_components: serde_json::Value,
    pub rating_trajectory: Option<String>,
    pub rating_trajectory_label: Option<String>,
    pub rating_trajectory_components: serde_json::Value,
    pub input_components: String,
    pub exclusions: RatingExclusions,
}

pub type RatingOutput = Generation<RatingProduct>;

/// A missing usable profile is a real uncalled product marker, not model abstention.
pub fn no_stats(season: i32, configured_model: impl Into<String>) -> RatingOutput {
    Generation::uncalled(
        RatingProduct {
            season,
            skipped_no_stats: true,
            abstained: false,
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
        configured_model.into(),
        RATING_PROMPT_VERSION,
        Vec::new(),
        None,
    )
}

/// An unchanged prepared assignment completes without a call or new product row.
pub fn unchanged(assignment: Assignment, configured_model: impl Into<String>) -> RatingOutput {
    Generation::uncalled(
        RatingProduct {
            season: assignment.season,
            skipped_no_stats: false,
            abstained: false,
            skipped_unchanged: true,
            body: None,
            headline: None,
            notability: None,
            notability_components: serde_json::json!({}),
            rating_trajectory: Some(assignment.rating_trajectory.key),
            rating_trajectory_label: assignment.rating_trajectory.label,
            rating_trajectory_components: assignment.rating_trajectory.components,
            input_components: assignment.input_components,
            exclusions: assignment.exclusions,
        },
        configured_model.into(),
        RATING_PROMPT_VERSION,
        Vec::new(),
        Some(assignment.input_hash),
    )
}

/// Refuse a world that contradicts the plugin's own approved statements.
///
/// The palette used to be the thing sent, so `validate()` was enough. Now it is
/// this plugin's declaration of what the profile supports, and the check keeps
/// it load-bearing: a fact in the world that no approved statement rests on, or
/// an approved statement about a fact the world does not carry, is a
/// preparation defect. Neither reaches the model.
fn check_supported(world: &str, parts: &parts::Parts) -> Result<()> {
    let supplied = numeric_literals(world);
    let mut problems = Vec::new();
    for value in &parts.profile.values {
        // An approved statement may only assert figures this value carries.
        for figure in [
            value.value,
            value.percentile,
            value.prior_percentile,
            value.quality_z,
        ]
        .into_iter()
        .flatten()
        {
            if !supplied
                .iter()
                .any(|(_, known)| (known - figure).abs() < 0.000_001)
            {
                problems.push(format!(
                    "{} carries {figure} but the rendered world does not",
                    value.label
                ));
            }
        }
        if let Some(band) = value.band.as_deref() {
            if !world.contains(band) {
                problems.push(format!(
                    "{} is banded {band:?} but the rendered world does not state it",
                    value.label
                ));
            }
        }
    }
    if let Some(composite) = parts.profile.composite {
        if !supplied
            .iter()
            .any(|(_, known)| (known - composite).abs() < 0.000_001)
        {
            problems.push(format!(
                "the composite {composite} is not in the rendered world"
            ));
        }
    }
    // A measured window's numbers must also be present: they are the plugin's
    // arithmetic over stored fixtures and a reader must be able to check them.
    for window in &parts.memory.measured {
        for figure in [
            window.per_match,
            window.per_match_change,
            window.percent_change,
        ]
        .into_iter()
        .flatten()
        {
            if !supplied
                .iter()
                .any(|(_, known)| (known - figure).abs() < 0.000_001)
            {
                problems.push(format!(
                    "measured window {} carries {figure} but the rendered world does not",
                    window.measure_label
                ));
            }
        }
    }
    anyhow::ensure!(
        problems.is_empty(),
        "the prepared world contradicts the plugin's approved statements: {}",
        problems.join("; ")
    );
    Ok(())
}

/// Create the Scout card from material prepared by the application.
///
/// **The palette is no longer the call.** Until s61 this function overwrote the
/// system prompt, the temperature, the token allowance and the response schema,
/// then sent `palette.prompt()` and parsed the result back through
/// `PaletteParser` — so the model chose one of two or three approved phrasings
/// per fact, and the ~300-line prose prompt, the 700-token card contract and the
/// entire `RatingRequestParser` guard chain were reachable only from evaluation.
/// The world the adapter had already assembled was never sent.
///
/// Now the model reads the prepared world and articulates it, with the palette
/// retained as the plugin's statement of what it is willing to say: every
/// statement in it must be supported by the world, and `check_supported` refuses
/// a body that asserts a figure the world does not carry.
pub async fn create(studio: &Studio<'_>, assignment: Assignment) -> Result<RatingOutput> {
    let palette = &assignment.palette;
    // The approved statements are still validated: they are the plugin's own
    // claim about what the profile supports, and an unusable one is a preparation
    // defect even though it is no longer the thing sent.
    palette.validate()?;
    check_supported(&assignment.built_prompt, &assignment.parts)?;
    let mut opts = assignment.opts.clone();
    let extracted = studio
        .extract(
            &assignment.built_prompt,
            &opts,
            &RatingRequestParser::new(
                &assignment.built_prompt,
                &assignment.comparison_directions,
                &assignment.measurement_bands,
            ),
            crate::plugins::support::prompt::publishing_correction,
        )
        .await?;
    opts.system = assignment.opts.system.clone();
    let call = GenerationCall::from(&extracted);
    let Some(reply) = extracted.value else {
        // A called pass is a complete response: the model was asked, and
        // declined. That is distinct from a missing usable profile, which is an
        // uncalled marker, and distinct from a malformed card, which is an error.
        return Ok(Generation::called(
            RatingProduct {
                season: assignment.season,
                skipped_no_stats: false,
                abstained: true,
                skipped_unchanged: false,
                body: None,
                headline: None,
                notability: Some(assignment.notability),
                notability_components: assignment.notability_components,
                rating_trajectory: Some(assignment.rating_trajectory.key),
                rating_trajectory_label: assignment.rating_trajectory.label,
                rating_trajectory_components: assignment.rating_trajectory.components,
                input_components: assignment.input_components,
                exclusions: assignment.exclusions,
            },
            extracted.model,
            RATING_PROMPT_VERSION,
            Vec::new(),
            Some(assignment.input_hash),
            call,
        ));
    };
    crate::plugins::support::form::validate_body(&reply.body)?;
    // The title is the plugin's own, not the model's: it names the entity and the
    // kind of read, which is a fact rather than something to articulate.
    let headline = crate::plugins::support::guards::settle_title(
        "scout",
        Some(&format!(
            "{}: measured profile",
            assignment.subject.entity_name
        )),
    );

    Ok(Generation::called(
        RatingProduct {
            season: assignment.season,
            skipped_no_stats: false,
            abstained: false,
            skipped_unchanged: false,
            body: Some(reply.body),
            headline,
            notability: Some(assignment.notability),
            notability_components: assignment.notability_components,
            rating_trajectory: Some(assignment.rating_trajectory.key),
            rating_trajectory_label: assignment.rating_trajectory.label,
            rating_trajectory_components: assignment.rating_trajectory.components,
            input_components: assignment.input_components,
            exclusions: assignment.exclusions,
        },
        extracted.model,
        RATING_PROMPT_VERSION,
        Vec::new(),
        Some(assignment.input_hash),
        call,
    ))
}

#[cfg(test)]
mod tests;

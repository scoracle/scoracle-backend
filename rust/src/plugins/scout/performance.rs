//! Selected measurements, sample limits, and standing. Selection already happened.
use serde::Serialize;

use crate::plugins::scout::cognition::{
    pct_band, relative_direction, sample_appearances, RatingProfile, RelativeDirection,
    SkillChange, MIN_CROSS_SEASON_APPEARANCES,
};

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

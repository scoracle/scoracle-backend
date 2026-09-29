//! The Scout's six parts and one manual, assembled through the shared renderer.
//!
//! This module replaces the flat prose block `inputs.rs` used to build. The
//! difference is not formatting: it is that a part is now a named, typed thing
//! the model can be told about, and the plugin decides what goes in each one.
//!
//! # What each part is
//!
//! | Part | Module | Content |
//! | --- | --- | --- |
//! | identity | `plugins::meta` | the subject |
//! | fresh | this module | the current measured profile and its comparisons |
//! | memory | `scout::memories` | studied statistical trends, and the injury / suspension text |
//! | form | `support::form` | the keys and dimensions this plugin returns |
//! | voice | `scout.rs` | the character |
//! | manual | `prompt.rs` | how the parts fit together |
//!
//! # Why the memory part is two things
//!
//! The Scout is the one character whose memory is not reporting history. It has
//! measured numbers — a trend across recent fixtures, a prior-season standing —
//! and it has text: injury and suspension claims, personnel changes. Both belong
//! in the world, and they are not interchangeable. A percentile movement is not
//! a suspension, and neither is evidence of the other. `memories::Selected`
//! carries both under distinct keys so the manual can say which is which, and so
//! a reader can tell a measured trend from a reported claim without the model.
use serde::Serialize;

/// The Scout's measured profile, as presented. Selection has already happened:
/// these are the measures the plugin chose, with their provenance intact.
///
/// `values` is a map rather than an array so the model reads a measure by name
/// and cannot silently reorder the set. `comparisons` carries a prior-season
/// percentile only for the same measure; a measure absent from it has no
/// supported direction, which is different from a direction of "no change".
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Profile {
    pub season: i32,
    #[serde(default)]
    pub observed_at: Option<String>,
    /// The curated display name for the sport ("Football (Soccer)"), so the
    /// model is never left to infer what a stored sport id means. Omitted when
    /// there is no curated name, in which case the id in `identity` stands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sport_name: Option<String>,
    /// The sample this profile is computed over, by stat-definition label.
    pub sample: std::collections::BTreeMap<String, f64>,
    /// The selected measurements, keyed by measure label.
    pub values: Vec<MeasuredValue>,
    /// Standardized overall score, where 50 is the peer mean. Absent rather
    /// than defaulted: a missing composite is not a composite of 50.
    pub composite: Option<f64>,
    /// How this profile's values may be read. `cross_season` gates the whole
    /// comparison part; without it a prior-season percentile cannot be stated.
    pub supports_cross_season: bool,
    /// Measures that exist but were not selected, so an omitted measure is not
    /// read as unmeasured or as zero at the source.
    #[serde(default, skip_serializing_if = "<[String]>::is_empty")]
    pub not_selected: Vec<String>,
    /// The boundary the plugin has placed on what this profile may be used for.
    /// A thin or single-appearance sample is coverage, not proof of
    /// participation, and the model must not read it as such.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
}

/// One measured quantity, with everything needed to read it honestly.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct MeasuredValue {
    /// The display label, which is what a reader would recognize.
    pub label: String,
    /// Identity of the underlying measurement, distinct from its label.
    pub measure: String,
    /// The raw value. `None` is unmeasured, which is not zero.
    #[serde(default)]
    pub value: Option<f64>,
    /// The eligible-cohort percentile; higher is better, already polarity-adjusted.
    #[serde(default)]
    pub percentile: Option<f64>,
    /// Size of the population that percentile ranked. Absent means no
    /// comparison population exists, not a population of one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cohort: Option<f64>,
    /// The plugin's own band label for this percentile. The model may use this
    /// wording; it may not compute a different one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
    /// The sign-adjusted standardized distance from the peer mean, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_z: Option<f64>,
    /// The prior season's percentile for this same measure, when a compatible
    /// comparison exists. `Some` is a supported direction; `None` is no
    /// comparison, which is not "unchanged".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_percentile: Option<f64>,
}

/// A boundary on what this profile supports, in the plugin's words.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Limit {
    /// Fewer appearances than a cross-season comparison needs. The count is
    /// known and it is too small.
    ThinSample { appearances: f64, minimum: f64 },
    /// The participation count could not be read at all, so its size is
    /// unknown. This is a DIFFERENT boundary from a thin sample: a thin sample
    /// is a coverage we can see and measure, an unknown one is a coverage we
    /// cannot. Reporting "0 appearances" for a profile whose sample labels we
    /// could not read would state a measurement we never made.
    UnknownSample { minimum: f64 },
    /// At most one recorded appearance: source coverage, not playing time.
    OneAppearance { appearances: f64 },
    /// Measurements exist but their underlying identity is unavailable, so they
    /// cannot be interpreted. Distinct from `NoMeasurements`: there is something
    /// there and we cannot say what.
    WithheldIdentity,
    /// No current measurements were supplied at all.
    NoMeasurements,
}

/// The recent-form measurement the plugin computed, if it had one.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Trend {
    /// The plugin's own direction label — `rising`, `falling` or `steady` — over
    /// a named number of scored events.
    pub direction: String,
    pub sample_size: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A rate-adjusted standout, where a limited-minutes subject produces at an
/// elite rate. This is a different lens from the same raw value, and the
/// presentation says which lens it is rather than leaving two numbers to be
/// confused for one.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct RateStandout {
    /// The rate basis, e.g. `per_36`.
    pub mode: String,
    pub label: String,
    pub percentile: f64,
}

/// The Scout's parts, in a form a quality fixture can store.
///
/// A fixture that stores only a rendered prompt cannot detect a changed
/// assembler. Storing the parts and rebuilding through [`assemble`] makes that a
/// test failure. The type lives here because this plugin owns what its parts
/// are; the harness only chooses the JSON.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Parts {
    pub subject: crate::plugins::meta::EntityMeta,
    #[serde(default)]
    pub sport_name: String,
    pub season: i32,
    /// The selected measurements, already chosen and formatted.
    pub profile: Profile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rate_standouts: Vec<RateStandout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trend: Option<Trend>,
    /// Studied statistical trend context, and the injury / suspension claims.
    /// Both live here because both are memory, and they are keyed apart because
    /// a measured trend is not a reported claim.
    #[serde(default)]
    pub memory: crate::plugins::scout::memories::Selected,
}

/// Present the selected profile as this plugin's own part.
///
/// Everything here was already chosen: `profile` has passed the off-facet,
/// degenerate-zero, display-tier and thin-sample filters, and `comparisons`
/// already holds only same-measure prior percentiles within one league. This
/// function moves the selection's results into a shape a reader — and the model —
/// can be told about, and it does not select anything further.
pub fn profile_parts(
    profile: &crate::plugins::scout::cognition::RatingProfile,
    sport_name: &str,
    supports_cross_season: bool,
    comparisons: Option<
        &std::collections::BTreeMap<String, crate::plugins::scout::cognition::SkillChange>,
    >,
    exclusions: &crate::plugins::scout::cognition::RatingExclusions,
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
            // The plugin's own band, so the model verbalizes rather than maps a
            // percentile to a quality word itself.
            band: datapoint
                .pct
                .map(|pct| crate::plugins::scout::cognition::pct_band(pct).to_string()),
            quality_z: signed_z(datapoint),
            // A prior percentile only when a cross-season comparison is
            // supported. Without one, `None` means "no comparison", which the
            // manual states is not stability — and stating it is the point.
            prior_percentile: supports_cross_season
                .then(|| {
                    comparisons
                        .and_then(|changes| changes.get(&datapoint.label))
                        .map(|change| change.prior_pct)
                })
                .flatten(),
        })
        .collect();
    let mut not_selected = exclusions.budget_truncated_stat_labels.clone();
    not_selected.extend(exclusions.off_facet_stat_labels.iter().cloned());
    not_selected.extend(exclusions.degenerate_zero_stat_labels.iter().cloned());
    not_selected.extend(exclusions.display_tier_stat_labels.iter().cloned());
    not_selected.extend(exclusions.thin_sample_omitted_stat_labels.iter().cloned());
    not_selected.sort();
    not_selected.dedup();
    Profile {
        season: profile.season,
        observed_at: profile.observed_at.clone(),
        sport_name: (!sport_name.trim().is_empty()).then(|| sport_name.trim().to_string()),
        // Participation totals are withheld whenever the sample cannot support a
        // cross-season comparison. A stored count is SOURCE COVERAGE, and
        // presenting "3 appearances" beside a profile is an invitation to read it
        // as playing time — which the `limit` below exists to prevent. The count
        // is not lost: it is in the provenance, and the limit states it in words.
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
        not_selected,
        limit: limit_for(profile, supports_cross_season),
    }
}

/// Whether a stat-definition label counts participation, under any of the
/// spellings the sources use.
fn is_participation(label: &str) -> bool {
    matches!(
        label.trim().to_ascii_lowercase().as_str(),
        "appearances" | "games played" | "games_played" | "matches played" | "matches_played"
    )
}

/// The boundary this profile places on what may be said about it, or `None`
/// when the sample is thick enough that no boundary applies.
fn limit_for(
    profile: &crate::plugins::scout::cognition::RatingProfile,
    supports_cross_season: bool,
) -> Option<Limit> {
    let appearances = sample_appearances(profile);
    if profile.breakdown.is_empty() && profile.composite_score.is_none() {
        return Some(Limit::NoMeasurements);
    }
    // Measurements whose identity is unavailable cannot be interpreted at all,
    // which is a different problem from having too few of them.
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
            minimum: crate::plugins::scout::cognition::MIN_CROSS_SEASON_APPEARANCES,
        }),
        // No participation label we recognise. The count is unknown, which is
        // not zero and not thin — it is a coverage we cannot see.
        None if !supports_cross_season => Some(Limit::UnknownSample {
            minimum: crate::plugins::scout::cognition::MIN_CROSS_SEASON_APPEARANCES,
        }),
        _ => None,
    }
}

fn sample_appearances(profile: &crate::plugins::scout::cognition::RatingProfile) -> Option<f64> {
    profile.sample.iter().find_map(|(label, value)| {
        matches!(
            label.trim().to_ascii_lowercase().as_str(),
            "appearances" | "games played" | "games_played" | "matches played" | "matches_played"
        )
        .then_some(*value)
    })
}

/// The sign-adjusted standardized distance, where positive is always favorable.
/// Unknown polarity yields nothing rather than a zero that reads as average.
fn signed_z(datapoint: &crate::plugins::scout::cognition::RatingDatapoint) -> Option<f64> {
    match datapoint.sign {
        -1 | 1 => datapoint
            .z
            .filter(|z| z.is_finite())
            .map(|z| datapoint.sign as f64 * z),
        _ => None,
    }
}

/// Present the rate-adjusted standouts, which are the same measurements under a
/// different basis. Empty is omitted rather than rendered as an empty list, so
/// "no elite rate was found" stays distinguishable from "this was not looked at".
pub fn rate_standout_parts(
    profile: &crate::plugins::scout::cognition::RatingProfile,
) -> Vec<RateStandout> {
    crate::plugins::scout::cognition::collect_rate_standouts_public(profile)
        .into_iter()
        .map(|standout| RateStandout {
            mode: standout.mode,
            label: standout.label,
            percentile: standout.pct,
        })
        .collect()
}

impl Parts {
    /// Render the world. The parts and their order are this plugin's choice;
    /// `assembly::World` only renders them, deterministically and in the order
    /// given here.
    pub fn assemble(&self) -> crate::plugins::assembly::World {
        crate::plugins::assembly::World::new()
            .part("identity", self.subject.for_writing())
            .part("fresh", &self.profile)
            .part("memory", &self.memory)
            .part("rate_standouts", &self.rate_standouts)
            .part("trend", &self.trend)
            .part("voice", crate::plugins::scout::cognition::CHARACTER)
            .part("form", crate::plugins::scout::cognition::prose().form())
    }

    pub fn render(&self) -> String {
        self.assemble().render()
    }
}

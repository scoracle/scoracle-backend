//! Scout-owned data and relationships, rendered by the shared ordered renderer.
//! Measured history and dated reporting retain separate provenance.
//! `prompt.rs` owns the job; Scout's local `voice.rs` supplies tone.
use serde::Serialize;

/// The Scout's measured profile, as presented. Selection has already happened:
/// these are the measures the plugin chose, with their provenance intact.
///
/// Each value keeps its label, measurement identity and any compatible prior
/// percentile together. An absent prior percentile is no supported direction,
/// which is different from a direction of "no change".
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
    /// The prior season's percentile for this same measure, when a compatible
    /// comparison exists. `Some` is a supported direction; `None` is no
    /// comparison, which is not "unchanged".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_percentile: Option<f64>,
}

impl MeasuredValue {
    fn standing_change(&self) -> Option<super::RelativeDirection> {
        Some(super::relative_direction(
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
        standing_change: Option<super::RelativeDirection>,
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
    supports_cross_season: bool,
    comparisons: Option<
        &std::collections::BTreeMap<String, crate::plugins::scout::cognition::SkillChange>,
    >,
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
    Profile {
        season: profile.season,
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

impl Parts {
    /// Admission uses selected measurements; unknown ranks never become zero.
    pub fn has_measured_profile(&self) -> bool {
        self.profile.values.iter().any(|value| {
            !value.measure.trim().is_empty()
                && value
                    .percentile
                    .is_some_and(|p| p.is_finite() && (0.0..=100.0).contains(&p))
        }) || self.profile.composite.is_some_and(f64::is_finite)
    }

    pub fn generation_options(
        &self,
        num_ctx: i32,
        temperature: f64,
    ) -> crate::studio::model::GenerateOptions {
        crate::studio::model::GenerateOptions {
            system: Some(super::prompt::TASK.into()),
            temperature: Some(temperature),
            num_predict: super::RATING_NUM_PREDICT,
            num_ctx,
            json_mode: false,
            format_schema: Some(super::prose().schema()),
            format_schema_raw: None,
        }
    }

    pub fn comparison_directions(
        &self,
    ) -> std::collections::BTreeMap<String, super::RelativeDirection> {
        self.profile
            .values
            .iter()
            .filter_map(|value| Some((value.label.clone(), value.standing_change()?)))
            .collect()
    }

    pub fn measurement_bands(&self) -> std::collections::BTreeMap<String, String> {
        self.profile
            .values
            .iter()
            .filter_map(|value| Some((value.label.clone(), value.band.clone()?)))
            .collect()
    }

    pub fn parse(&self, raw: &str) -> anyhow::Result<Option<super::RatingReply>> {
        use crate::studio::Parser;
        super::RatingRequestParser::new(
            &self.render(),
            &self.comparison_directions(),
            &self.measurement_bands(),
        )
        .parse(raw)
    }

    /// Preserve the completed wire world and provenance in one debounce input.
    pub fn input_components(&self, mut provenance: serde_json::Value) -> String {
        provenance["measured_fixture_ids"] = serde_json::json!(self
            .memory
            .measured
            .iter()
            .map(|window| (&window.measure_label, &window.fixture_ids))
            .collect::<Vec<_>>());
        serde_json::json!({"world": self.render(), "prompt_version": super::RATING_PROMPT_VERSION,
            "output_contract": super::RATING_OUTPUT_CONTRACT_VERSION, "provenance": provenance})
        .to_string()
    }

    /// Render the world. The parts and their order are this plugin's choice;
    /// `assembly::World` only renders them, deterministically and in the order
    /// given here.
    pub fn assemble(&self) -> crate::plugins::assembly::World {
        #[derive(Serialize)]
        struct Meta<'a> {
            name: &'a str,
            entity_type: &'a str,
            sport: &'a str,
        }
        let mut world = crate::plugins::assembly::World::new()
            .part(
                "meta",
                Meta {
                    name: &self.subject.name,
                    entity_type: &self.subject.entity_type,
                    sport: if self.sport_name.trim().is_empty() {
                        &self.subject.sport
                    } else {
                        &self.sport_name
                    },
                },
            )
            .part("fresh", &self.profile);
        if !self.memory.measured.is_empty()
            || !self.memory.reported.is_empty()
            || !self.memory.coverage_limits.is_empty()
        {
            world = world.part("memories", &self.memory);
        }
        world
            .part("voice", crate::plugins::scout::voice::VOICE)
            .part("form", crate::plugins::scout::cognition::prose().form())
    }

    pub fn render(&self) -> String {
        self.assemble().render()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rendered_directions_match_acceptance_without_borrowing_missing_comparisons() {
        let capture: serde_json::Value = serde_json::from_str(
            include_str!("../../../../fixtures/scout/closure-s62-inputs.jsonl")
                .lines()
                .next()
                .unwrap(),
        )
        .unwrap();
        let mut parts: Parts =
            serde_json::from_value(capture["assignment"]["parts"].clone()).unwrap();
        let world: serde_json::Value = serde_json::from_str(&parts.render()).unwrap();
        assert!(world["fresh"].get("composite_peer_mean").is_none());
        parts.profile.composite = None;
        let world: serde_json::Value = serde_json::from_str(&parts.render()).unwrap();
        assert!(world["fresh"].get("composite_peer_mean").is_none());
        for (prior, expected) in [
            (Some(80.0), Some("rose")),
            (Some(95.0), Some("fell")),
            (Some(89.5), Some("held")),
            (None, None),
        ] {
            parts.profile.values[0].prior_percentile = prior;
            let world: serde_json::Value = serde_json::from_str(&parts.render()).unwrap();
            assert_eq!(
                world["fresh"]["values"][0]["standing_change"].as_str(),
                expected
            );
            assert_eq!(
                parts
                    .comparison_directions()
                    .get("Rebounds")
                    .map(|v| serde_json::to_value(v).unwrap()),
                expected.map(|s| json!(s))
            );
        }
    }

    #[test]
    fn completed_world_and_provenance_determine_the_fingerprint() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../fixtures/quality/rating/synthetic-strong.json"
        ))
        .unwrap();
        let parts: Parts = serde_json::from_value(fixture["parts"].clone()).unwrap();
        let hash = |p: &Parts, revision| {
            crate::util::hash_components(&p.input_components(json!({"source_revision":revision})))
        };
        let baseline = hash(&parts, 1);
        assert_eq!(baseline, hash(&parts.clone(), 1));
        assert_ne!(baseline, hash(&parts, 2));
        let mut changed = parts.clone();
        changed
            .memory
            .measured
            .push(crate::plugins::scout::memories::Measured {
                measure_label: "Blocks".into(),
                unit: "count".into(),
                previous: crate::plugins::scout::memories::Window {
                    from: "2026-08-12".into(),
                    before: "2026-09-01".into(),
                    fixtures: 3,
                    measured: 2,
                    per_match: Some(1.0),
                },
                current: crate::plugins::scout::memories::Window {
                    from: "2026-09-01".into(),
                    before: "2026-09-20".into(),
                    fixtures: 3,
                    measured: 2,
                    per_match: Some(2.0),
                },
                per_match_change: None,
                percent_change: None,
                fixture_ids: vec![1, 2, 3],
            });
        assert_ne!(baseline, hash(&changed, 1));
        let measured = hash(&changed, 1);
        changed.memory.measured[0].current.per_match = Some(3.0);
        assert_ne!(measured, hash(&changed, 1));
        let current_changed = hash(&changed, 1);
        changed.memory.measured[0].previous.before = "2026-08-31".into();
        assert_ne!(current_changed, hash(&changed, 1));
        let bounds_changed = hash(&changed, 1);
        let rendered = changed.render();
        changed.memory.measured[0].fixture_ids.push(4);
        assert_eq!(rendered, changed.render());
        assert_ne!(bounds_changed, hash(&changed, 1));
        let world: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert!(world["memories"]["measured"][0]
            .get("fixture_ids")
            .is_none());
        changed = parts.clone();
        changed
            .memory
            .coverage_limits
            .push("One fixture has no measurement".into());
        assert_ne!(baseline, hash(&changed, 1));
        changed = parts;
        changed.profile.limit = Some(Limit::ThinSample {
            appearances: 3.0,
            minimum: 10.0,
        });
        assert_ne!(baseline, hash(&changed, 1));
    }
}

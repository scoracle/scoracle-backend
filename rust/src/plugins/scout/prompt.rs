//! Scout instructions and the prepared world the model reads.
//! Tone lives in local `voice.rs`. Measurements come from `performance.rs`.
use serde::Serialize;

use crate::plugins::meta::EntityMeta;
use crate::plugins::scout::cognition::{self, RatingReply};
use crate::plugins::scout::memories::Selected;
use crate::plugins::scout::performance::Profile;

pub const RATING_PROMPT_VERSION: &str = "s66-measured-windows";

/// Instructions accompanying every prepared Scout world.
pub const TASK: &str = "\
meta identifies the subject. fresh contains selected season measurements and
their sample and limits. memories contains separate previous and current
measurement windows and dated,
attributed reports. voice sets tone; form sets output shape and limits.

Describe what the supplied measurements establish in compact prose. Use only
supplied numbers and claims. A percentile is relative standing, not ability or
the size of a difference. A prior percentile and standing_change apply only to
that measure; supports_cross_season alone does not establish a comparison.
Missing comparison or measured history is unknown, not stability. A composite
is a standardized overall score with peer mean 50, not a percentile or a
measure of any individual skill.

Each measured window owns its dates, fixture count, measured count and per-match
average. The supplied change compares current with previous, not seasons or
individual games. Missing fixture measurements are unknown, not zero.

Honor any limit. Sample counts describe source coverage, not playing time or
when a season began. Keep reports attributed and dated; a withdrawal retracts
its claim and does not establish recovery. Do not infer a cause, role, tactics,
fitness or future outcome from a measurement or report. A short or null body is
valid when the evidence does not support more.";

/// Retry the same bounded task without silently changing its content policy.
pub fn correction(error: &anyhow::Error) -> Option<String> {
    (error.is::<crate::plugins::support::form::SurfaceError>()
        || error.is::<crate::studio::model::IncompleteOutput>())
    .then(|| format!("{error:#} Return the complete requested JSON within the supplied form limits. Preserve the supplied qualifications and use only the prepared evidence."))
}

/// Fixture-replayable selected world. Wire order is the field order of [`Input`].
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    #[serde(default)]
    pub sport_name: String,
    pub season: i32,
    pub profile: Profile,
    #[serde(default)]
    pub memory: Selected,
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
            system: Some(TASK.into()),
            temperature: Some(temperature),
            num_predict: cognition::RATING_NUM_PREDICT,
            num_ctx,
            json_mode: false,
            format_schema: Some(cognition::prose().schema()),
            format_schema_raw: None,
        }
    }

    pub fn comparison_directions(
        &self,
    ) -> std::collections::BTreeMap<String, cognition::RelativeDirection> {
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

    pub fn parse(&self, raw: &str) -> anyhow::Result<Option<RatingReply>> {
        use crate::studio::Parser;
        cognition::RatingRequestParser::new(
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
        serde_json::json!({"world": self.render(), "prompt_version": RATING_PROMPT_VERSION,
            "output_contract": cognition::RATING_OUTPUT_CONTRACT_VERSION, "provenance": provenance})
        .to_string()
    }

    /// Field order is the wire order. Optional memories are omitted, not empty.
    pub fn render(&self) -> String {
        #[derive(Serialize)]
        struct Meta<'a> {
            name: &'a str,
            entity_type: &'a str,
            sport: &'a str,
        }
        #[derive(Serialize)]
        struct Form<'a> {
            keys: &'a [String],
            max_chars: usize,
            #[serde(skip_serializing_if = "Option::is_none")]
            paragraph_max_chars: Option<usize>,
        }
        #[derive(Serialize)]
        struct Input<'a> {
            meta: Meta<'a>,
            fresh: &'a Profile,
            #[serde(skip_serializing_if = "Option::is_none")]
            memories: Option<&'a Selected>,
            voice: &'static str,
            form: Form<'a>,
        }
        let prose = cognition::prose();
        let sport = if self.sport_name.trim().is_empty() {
            self.subject.sport.as_str()
        } else {
            self.sport_name.as_str()
        };
        let memories = (!self.memory.measured.is_empty()
            || !self.memory.reported.is_empty()
            || !self.memory.coverage_limits.is_empty())
        .then_some(&self.memory);
        serde_json::to_string(&Input {
            meta: Meta {
                name: &self.subject.name,
                entity_type: &self.subject.entity_type,
                sport,
            },
            fresh: &self.profile,
            memories,
            voice: crate::plugins::scout::voice::VOICE,
            form: Form {
                keys: &prose.keys,
                max_chars: prose.dims.total_max_chars,
                paragraph_max_chars: prose.dims.paragraph_max_chars,
            },
        })
        .expect("scout world serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::scout::performance::Limit;
    use serde_json::json;

    #[test]
    fn correction_preserves_the_provider_failure_under_context() {
        let error = anyhow::Error::new(crate::studio::model::IncompleteOutput(
            "incomplete model output (finish reason: length)".into(),
        ))
        .context("model generate");
        assert!(correction(&error)
            .unwrap()
            .contains("finish reason: length"));
    }

    #[test]
    fn rendered_directions_match_acceptance_without_borrowing_missing_comparisons() {
        let capture: serde_json::Value = serde_json::from_str(
            include_str!("../../../fixtures/scout/closure-s62-inputs.jsonl")
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
            "../../../fixtures/quality/rating/synthetic-strong.json"
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

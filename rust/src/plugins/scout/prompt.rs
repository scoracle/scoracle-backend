//! Scout preparation, instructions and the selected world the model reads.
//! Tone lives in local `voice.rs`. Measurements come from `performance.rs`.
use serde::Serialize;

use super::parser::{self, RatingReply};
use super::performance::{RatingExclusions, RatingTrajectory};
use crate::plugins::meta::EntityMeta;
use crate::plugins::scout::memories::Selected;
use crate::plugins::scout::performance::{self, Profile};
use crate::studio::model::GenerateOptions;
use crate::util::hash_components;
use anyhow::{Context, Result};

pub const RATING_PROMPT_VERSION: &str = "s66-measured-windows";

/// Production rating temperature.
pub const RATING_TEMPERATURE: f64 = 0.6;

/// Token cap for the compact scouting card.
pub const RATING_NUM_PREDICT: i32 = 700;

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

/// Application-prepared Scout work. No storage row, queue lease, or concrete model host enters
/// this contract.
pub enum RatingBuild {
    NoStats { season: i32 },
    Ready(Box<Assignment>),
}

pub struct Assignment {
    pub subject: Subject,
    pub season: i32,
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
    pub parts: crate::plugins::scout::prompt::Parts,
}

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
            num_predict: RATING_NUM_PREDICT,
            num_ctx,
            json_mode: false,
            format_schema: Some(parser::prose().schema()),
            format_schema_raw: None,
        }
    }

    pub fn comparison_directions(
        &self,
    ) -> std::collections::BTreeMap<String, performance::RelativeDirection> {
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
        parser::RatingRequestParser::new(
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
            "output_contract": parser::RATING_OUTPUT_CONTRACT_VERSION, "provenance": provenance})
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
        let prose = parser::prose();
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

/// Durable subject and invocation policy used to prepare a Studio Scout assignment.
#[derive(Clone, Debug)]
pub struct RatingReq {
    pub entity_type: String,
    pub entity_id: i32,
    pub entity_name: String,
    pub sport: String,
    pub season: Option<i32>,
    pub trigger_type: String,
}

/// Prepare the Scout's complete assignment without calling a model.
pub async fn build_rating_request(
    pool: &sqlx::PgPool,
    voice_num_ctx: i32,
    req: &RatingReq,
    temperature: f64,
    with_enrichment: bool,
) -> Result<RatingBuild> {
    let Some(mut profile) = performance::load_rating_profile(
        pool,
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

    let off_facet_stat_labels = performance::drop_off_facet_datapoints(&mut profile);
    let degenerate_zero_stat_labels = performance::drop_degenerate_zero_datapoints(&mut profile);
    let display_tier_stat_labels = performance::drop_display_tier_datapoints(&mut profile);
    if profile.composite_score.is_none() && profile.breakdown.is_empty() {
        return Ok(RatingBuild::NoStats {
            season: profile.season,
        });
    }

    let base_components = performance::input_components(&profile);
    // Old-season profiles must not inherit present employment or availability.
    let historical =
        profile.season != performance::current_season(pool, &req.sport.to_uppercase()).await?;
    let supports_cross_season = performance::supports_cross_season_comparison(&profile);
    let (notability, notability_components) = performance::compute_notability(&profile);
    let exclusions = RatingExclusions {
        budget_truncated_stat_labels: performance::budget_truncated_stat_labels(&profile.breakdown),
        off_facet_stat_labels,
        degenerate_zero_stat_labels,
        display_tier_stat_labels,
        thin_sample_omitted_stat_labels: performance::thin_sample_omitted_stat_labels(&profile),
    };
    let rating_trajectory = performance::load_rating_trajectory(
        pool,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        &profile,
    )
    .await?;

    // Keep the adjudicated records for provenance and select attributed memory from them.
    let (personnel, reported_memory) = if with_enrichment && !historical {
        let (changes, total) = match crate::evidence::personnel::load_personnel_changes(
            pool,
            &req.sport,
            &req.entity_type,
            req.entity_id,
        )
        .await
        {
            Ok(loaded) => loaded,
            Err(error) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    %error,
                    "rating: personnel-change load failed (continuing without the block)"
                );
                (Vec::new(), 0)
            }
        };
        let (availability, availability_total) =
            match crate::evidence::personnel::load_availability_changes(
                pool,
                &req.sport,
                &req.entity_type,
                req.entity_id,
            )
            .await
            {
                Ok(loaded) => loaded,
                Err(error) => {
                    tracing::warn!(
                        entity_type = %req.entity_type,
                        entity_id = req.entity_id,
                        sport = %req.sport,
                        %error,
                        "rating: availability load failed (continuing without those lines)"
                    );
                    (Vec::new(), 0)
                }
            };
        let reported = crate::plugins::scout::memories::Reported::from_records(
            &req.entity_type,
            req.entity_id,
            &changes,
            &availability,
            total + availability_total,
        );
        (
            serde_json::json!({"changes": changes, "availability": availability,
                "total_changes": total, "total_availability": availability_total}),
            reported,
        )
    } else {
        (serde_json::Value::Null, Vec::new())
    };

    let (current_reports, contested_claims) = if with_enrichment && !historical {
        match crate::evidence::personnel::load_scout_reports(
            pool,
            &req.entity_type,
            req.entity_id,
            &req.sport,
        )
        .await
        {
            Ok(claims) => {
                // A marked claim is one another source contradicts. The memory
                // carries the contradiction rather than dropping the claim,
                // because "the subject is disputed" is the reader-relevant fact.
                let contested: Vec<crate::plugins::scout::memories::Reported> = claims
                    .iter()
                    .filter(|c| c.marked)
                    .map(crate::plugins::scout::memories::Reported::from_claim)
                    .collect();
                (serde_json::to_value(&claims)?, contested)
            }
            Err(error) => {
                return Err(error).context("load verified Harvester Scout reports");
            }
        }
    } else {
        (serde_json::Value::Null, Vec::new())
    };
    let mut reported_memory = reported_memory;
    for mut claim in contested_claims {
        claim.disputed = Some(true);
        reported_memory.push(claim);
    }

    let comparisons = if with_enrichment && supports_cross_season {
        match performance::load_rating_profile(
            pool,
            &req.entity_type,
            req.entity_id,
            &req.sport,
            Some(profile.season - 1),
        )
        .await
        {
            Ok(Some(mut prior)) => {
                let _ = performance::drop_off_facet_datapoints(&mut prior);
                let _ = performance::drop_degenerate_zero_datapoints(&mut prior);
                let _ = performance::drop_display_tier_datapoints(&mut prior);
                Some(performance::build_skill_changes(&profile, &prior))
            }
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(
                    entity_type = %req.entity_type,
                    entity_id = req.entity_id,
                    sport = %req.sport,
                    %error,
                    "rating: prior-season profile load failed (continuing without movement lines)"
                );
                None
            }
        }
    } else {
        None
    };
    let prompt_profile =
        performance::model_prompt_profile(&profile, supports_cross_season, comparisons.as_ref());
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
    let mut components: serde_json::Value = serde_json::from_str(&base_components)?;
    components["skill_changes"] = serde_json::json!(comparisons);
    components["personnel"] = serde_json::json!(personnel);
    components["current_reports"] = serde_json::json!(current_reports);
    components["recent_form"] = serde_json::json!(form_trend);
    let sport_name: String =
        sqlx::query_scalar("SELECT display_name FROM public.sports WHERE id = $1")
            .bind(&req.sport)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| req.sport.clone());
    let subject = Subject {
        entity_type: req.entity_type.clone(),
        entity_name: req.entity_name.clone(),
        sport: req.sport.clone(),
        sport_name,
    };
    let (measured_memory, coverage_limits) = match super::memories::measured_memory(
        pool,
        &subject,
        req.entity_id,
        &profile,
        with_enrichment && !historical,
    )
    .await
    {
        Ok(parts) => parts,
        Err(error) => {
            // A failed study is not a study that found nothing. The difference
            // matters: the second is a fact about the fixtures, the first is a
            // fact about our ability to look. Recording it keeps the model from
            // reading absence-of-trend as stability.
            tracing::warn!(
                entity_type = %req.entity_type,
                entity_id = req.entity_id,
                sport = %req.sport,
                %error,
                "rating: match-statistic study failed (continuing without measured memory)"
            );
            (
                Vec::new(),
                vec!["Recent-fixture measurement window unavailable for this read.".into()],
            )
        }
    };
    let parts = crate::plugins::scout::prompt::Parts {
        subject: crate::plugins::meta::EntityMeta {
            name: subject.entity_name.clone(),
            entity_type: subject.entity_type.clone(),
            entity_id: req.entity_id,
            sport: subject.sport.clone(),
        },
        sport_name: subject.sport_name.clone(),
        season: profile.season,
        profile: crate::plugins::scout::performance::profile_parts(
            &prompt_profile,
            supports_cross_season,
            comparisons.as_ref(),
        ),
        memory: crate::plugins::scout::memories::select(
            &crate::plugins::scout::memories::Selection::rated(),
            measured_memory,
            reported_memory,
            coverage_limits,
        ),
    };
    let built_prompt = parts.render();
    let opts = parts.generation_options(voice_num_ctx, temperature);
    if !parts.has_measured_profile() {
        return Ok(RatingBuild::NoStats {
            season: profile.season,
        });
    }
    let input_components = parts.input_components(components);
    let input_hash = hash_components(&input_components);
    Ok(RatingBuild::Ready(Box::new(Assignment {
        subject,
        season: profile.season,
        notability,
        notability_components,
        rating_trajectory,
        input_components,
        input_hash,
        exclusions,
        opts,
        built_prompt,
        parts,
    })))
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

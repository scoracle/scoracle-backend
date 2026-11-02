//! Tone lives in local `voice.rs`. Measurements come from `performance.rs`.
//! Scout preparation, instructions and the selected world the model reads.
use super::character_instructions::INSTRUCTIONS;
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("stats-logic", "STATS_LOGIC");
use serde::Serialize;

use super::parser::{self, RatingReply};
use super::performance::{RatingExclusions, RatingTrajectory};
use crate::harness::model::GenerateOptions;
use crate::plugins::scout::performance::{self, Profile};
use crate::tools::meta::EntityMeta;
use crate::util::hash_components;
use anyhow::Result;

pub const RATING_PROMPT_VERSION: &str = "s89-clear-payload";

/// Production rating temperature.
pub const RATING_TEMPERATURE: f64 = 0.6;

/// Token cap for the compact scouting card.
pub const RATING_NUM_PREDICT: i32 = 700;

/// Subject of a Scout assignment. Durable identifiers and trigger policy stay in the application.
/// `sport_name` is the curated sport display name (e.g. "Football (Soccer)") so the model is
/// never left to guess what a sport id means; empty falls back to the raw id.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Subject {
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
    pub sport_name: String,
}

/// Application-prepared Scout work. No storage row, queue lease, or concrete model host enters
/// this contract.
#[derive(Serialize, serde::Deserialize)]
pub enum RatingBuild {
    NoStats {
        season: i32,
        #[serde(default)]
        availability: super::availability::Availability,
    },
    Ready(Box<Assignment>),
}

#[derive(Serialize, serde::Deserialize)]
pub struct Assignment {
    pub subject: Subject,
    pub season: i32,
    pub notability: i32,
    pub notability_components: serde_json::Value,
    pub rating_trajectory: RatingTrajectory,
    pub input_components: String,
    pub input_hash: String,
    pub exclusions: RatingExclusions,
    #[serde(skip)]
    pub opts: GenerateOptions,
    /// The prepared world, rendered once. This is what the model reads; it is
    /// rendered here rather than in `create` so the package measured, hashed and
    /// sent are the same bytes.
    pub built_prompt: String,
    /// The same world, unrendered, so the manual, the response schema and the
    /// package are derived from one source rather than three.
    pub parts: crate::plugins::scout::request::Parts,
}

/// Instructions accompanying every prepared Scout world.

/// Fixture-replayable selected world, rendered through the shared context envelope.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    #[serde(default)]
    pub database_record: Option<crate::tools::meta::DatabaseRecord>,
    #[serde(default)]
    pub sport_name: String,
    pub season: i32,
    pub profile: Profile,
    #[serde(default)]
    pub availability: super::availability::Availability,
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
    ) -> crate::harness::model::GenerateOptions {
        crate::harness::model::GenerateOptions {
            system: Some(INSTRUCTIONS.system().to_owned()),
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
        use crate::harness::Parser;
        parser::RatingRequestParser::new(
            &self.render(),
            &self.comparison_directions(),
            &self.measurement_bands(),
        )
        .parse(raw)
    }

    /// Preserve the completed wire world and provenance in one debounce input.
    pub fn input_components(&self, provenance: serde_json::Value) -> String {
        serde_json::json!({"world": self.render(), "prompt_version": RATING_PROMPT_VERSION,
            "instructions": INSTRUCTIONS, "output_contract": parser::RATING_OUTPUT_CONTRACT_VERSION, "provenance": provenance})
        .to_string()
    }

    /// Field order is the wire order; this package contains selected measurements.
    pub fn render(&self) -> String {
        let mut meta = self.subject.for_writing();
        if !self.sport_name.trim().is_empty() {
            meta = crate::tools::meta::WritingIdentity::new(
                &self.subject.name,
                &self.subject.entity_type,
                &self.sport_name,
            );
        }
        crate::tools::prompt::assemble(
            &INSTRUCTIONS,
            crate::tools::meta::context(meta, self.database_record.as_ref()),
            super::fresh::context(self),
        )
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
    let mut connection = pool.acquire().await?;
    build_rating_request_on(
        &mut connection,
        voice_num_ctx,
        req,
        temperature,
        with_enrichment,
    )
    .await
}

pub(crate) async fn build_rating_request_on(
    connection: &mut sqlx::PgConnection,
    voice_num_ctx: i32,
    req: &RatingReq,
    temperature: f64,
    with_enrichment: bool,
) -> Result<RatingBuild> {
    let availability = super::availability::Availability::load(
        connection,
        &EntityMeta {
            name: req.entity_name.clone(),
            entity_type: req.entity_type.clone(),
            entity_id: req.entity_id,
            sport: req.sport.clone(),
        },
    )
    .await?;
    let Some(mut profile) = performance::load_rating_profile(
        connection,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        req.season,
    )
    .await?
    else {
        return Ok(RatingBuild::NoStats {
            season: req.season.unwrap_or(0),
            availability,
        });
    };

    let off_facet_stat_labels = performance::drop_off_facet_datapoints(&mut profile);
    let degenerate_zero_stat_labels = performance::drop_degenerate_zero_datapoints(&mut profile);
    let display_tier_stat_labels = performance::drop_display_tier_datapoints(&mut profile);
    if profile.composite_score.is_none() && profile.breakdown.is_empty() {
        return Ok(RatingBuild::NoStats {
            season: profile.season,
            availability,
        });
    }

    let base_components = performance::input_components(&profile);
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
        connection,
        &req.entity_type,
        req.entity_id,
        &req.sport,
        &profile,
    )
    .await?;

    let comparisons = if with_enrichment && supports_cross_season {
        match performance::load_rating_profile(
            connection,
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
    let mut components: serde_json::Value = serde_json::from_str(&base_components)?;
    components["availability"] = serde_json::json!(availability);
    components["availability_contract"] =
        serde_json::json!(crate::plugins::classifier::AVAILABILITY_CONTRACT);
    components["skill_changes"] = serde_json::json!(comparisons);
    let sport_name: String =
        sqlx::query_scalar("SELECT display_name FROM public.sports WHERE id = $1")
            .bind(&req.sport)
            .fetch_optional(&mut *connection)
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
    let parts = crate::plugins::scout::request::Parts {
        subject: crate::tools::meta::EntityMeta {
            name: subject.entity_name.clone(),
            entity_type: subject.entity_type.clone(),
            entity_id: req.entity_id,
            sport: subject.sport.clone(),
        },
        database_record: crate::tools::meta::load_character_identity_on(
            connection,
            &req.entity_type,
            req.entity_id,
            &req.sport,
        )
        .await?,
        sport_name: subject.sport_name.clone(),
        season: profile.season,
        availability: availability.clone(),
        profile: crate::plugins::scout::performance::profile_parts(
            &prompt_profile,
            supports_cross_season,
            comparisons.as_ref(),
        ),
    };
    let built_prompt = parts.render();
    let opts = parts.generation_options(voice_num_ctx, temperature);
    if !parts.has_measured_profile() {
        return Ok(RatingBuild::NoStats {
            season: profile.season,
            availability,
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
        assert!(world["new_content"].get("composite_peer_mean").is_none());
        parts.profile.composite = None;
        let world: serde_json::Value = serde_json::from_str(&parts.render()).unwrap();
        assert!(world["new_content"].get("composite_peer_mean").is_none());
        for (prior, expected) in [
            (Some(80.0), Some("rose")),
            (Some(95.0), Some("fell")),
            (Some(89.5), Some("held")),
            (None, None),
        ] {
            parts.profile.values[0].prior_percentile = prior;
            let world: serde_json::Value = serde_json::from_str(&parts.render()).unwrap();
            assert_eq!(
                world["new_content"]["values"][0]["standing_change"].as_str(),
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
        let legacy = serde_json::to_value(&parts).unwrap();
        let mut injected = legacy.clone();
        injected["memory"] = json!({"reported":[{"reported_headline":"Old memory"}]});
        let fresh: Parts = serde_json::from_value(injected).unwrap();
        assert_eq!(fresh.render(), parts.render());
        assert!(!fresh.render().contains("memories"));
        let mut changed = parts;
        changed.profile.limit = Some(Limit::ThinSample {
            appearances: 3.0,
            minimum: 10.0,
        });
        assert_ne!(baseline, hash(&changed, 1));
    }
}

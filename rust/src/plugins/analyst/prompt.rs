//! Finished reading selection, dated study, request projection and provider options.
use crate::harness::route::RouteKey;
pub const MODEL: RouteKey = RouteKey::new("momentum-logic", "MOMENTUM_LOGIC");
use crate::harness::model::GenerateOptions;
use crate::plugins::oracle::prompt as oracle;
use crate::tools::meta::EntityMeta;
use crate::util::{hash_components, round1};
use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::PgPool;

pub const MOMENTUM_PROMPT_VERSION: &str = "momentum-s46-context-guide";

pub const MOMENTUM_SYSTEM_PROMPT: &str = include_str!("instructions.txt");

/// The Scout card supplied to the Analyst. The reading is already a finished interpretation;
/// the Analyst should synthesize it, not reconstruct it from the Scout's raw measurements.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Form {
    pub body: String,
    pub headline: Option<String>,
    pub season: Option<i32>,
    pub generated_at: Option<String>,
    pub input_hash: Option<String>,
}

/// The Influencer card supplied to the Analyst.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Mood {
    pub body: String,
    pub headline: Option<String>,
    pub sentiment: Option<i32>,
    pub generated_at: Option<String>,
    pub input_hash: Option<String>,
}

/// One explicitly dated trajectory study. It is supporting evidence for the two finished
/// readings, not a second set of overlapping labels or a pre-written Analyst verdict.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub vibe_slope: Option<f64>,
    pub vibe_samples: i32,
    pub vibe_window_start: Option<String>,
    pub vibe_window_end: Option<String>,
    pub rating_slope: Option<f64>,
    pub rating_samples: i32,
    pub rating_window_start: Option<String>,
    pub rating_window_end: Option<String>,
    pub momentum_score: Option<f64>,
    pub generated_at: Option<String>,
}

impl Snapshot {
    pub fn empty(&self) -> bool {
        self.vibe_slope.is_none() && self.rating_slope.is_none() && self.momentum_score.is_none()
    }
}

/// Complete materials for one creation. The application owns the durable subject key.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Assignment {
    pub entity_id: i32,
    pub entity_type: String,
    pub entity_name: String,
    pub sport: String,
    pub context: MomentumContext,
    pub voice_num_ctx: i32,
}

/// Keep Momentum on the incumbent stats route until a broader fixture set proves a split.
pub const MOMENTUM_TEMPERATURE: f64 = 0.3;

// A single direction read — two rails and what they are doing to each other — is a handful of
// sentences on a card. Nothing here scales with story count.
pub const MOMENTUM_NUM_PREDICT: i32 = 700;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MomentumContext {
    pub season: i32,
    pub database_record: Option<crate::tools::meta::DatabaseRecord>,
    pub rating: Option<Form>,
    pub vibe: Option<Mood>,
    pub snapshot: Snapshot,
    pub input_components_json: String,
    pub input_hash: String,
}

impl MomentumContext {
    pub fn empty(&self) -> bool {
        self.rating.is_none()
            && self.vibe.is_none()
            && self.snapshot.rating_slope.is_none()
            && self.snapshot.vibe_slope.is_none()
    }
}

pub fn build_momentum_input_components(
    rating: Option<&Form>,
    vibe: Option<&Mood>,
    mom: &Snapshot,
) -> String {
    let mut components = serde_json::Map::new();
    components.insert(
        "fresh".into(),
        serde_json::to_value(super::fresh::readings(rating, vibe, mom))
            .expect("Analyst evidence serializes"),
    );
    components.insert(
        "instructions".into(),
        serde_json::json!(MOMENTUM_SYSTEM_PROMPT),
    );
    components.insert("tone".into(), serde_json::json!(super::voice::VOICE));
    components.insert(
        "prompt_version".into(),
        serde_json::json!(MOMENTUM_PROMPT_VERSION),
    );
    if let Some(r) = rating {
        components.insert("scout_body".into(), serde_json::json!(r.body));
        components.insert("scout_headline".into(), serde_json::json!(r.headline));
        components.insert("scout_season".into(), serde_json::json!(r.season));
        components.insert(
            "scout_generated_at".into(),
            serde_json::json!(r.generated_at),
        );
        components.insert("scout_input_hash".into(), serde_json::json!(r.input_hash));
    }
    if let Some(v) = vibe {
        components.insert("influencer_body".into(), serde_json::json!(v.body));
        components.insert("influencer_headline".into(), serde_json::json!(v.headline));
        components.insert(
            "influencer_sentiment".into(),
            serde_json::json!(v.sentiment),
        );
        components.insert(
            "influencer_generated_at".into(),
            serde_json::json!(v.generated_at),
        );
        components.insert(
            "influencer_input_hash".into(),
            serde_json::json!(v.input_hash),
        );
    }
    if let Some(s) = mom.rating_slope {
        components.insert("momentum_rating_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_rating_samples".into(),
            serde_json::json!(mom.rating_samples),
        );
    }
    if mom.rating_window_start.is_some() {
        components.insert(
            "momentum_rating_window_start".into(),
            serde_json::json!(mom.rating_window_start),
        );
    }
    if mom.rating_window_end.is_some() {
        components.insert(
            "momentum_rating_window_end".into(),
            serde_json::json!(mom.rating_window_end),
        );
    }
    if let Some(s) = mom.vibe_slope {
        components.insert("momentum_vibe_slope".into(), serde_json::json!(round1(s)));
        components.insert(
            "momentum_vibe_samples".into(),
            serde_json::json!(mom.vibe_samples),
        );
    }
    if mom.vibe_window_start.is_some() {
        components.insert(
            "momentum_vibe_window_start".into(),
            serde_json::json!(mom.vibe_window_start),
        );
    }
    if mom.vibe_window_end.is_some() {
        components.insert(
            "momentum_vibe_window_end".into(),
            serde_json::json!(mom.vibe_window_end),
        );
    }
    if let Some(score) = mom.momentum_score {
        components.insert("momentum_score".into(), serde_json::json!(round1(score)));
    }
    if mom.generated_at.is_some() {
        components.insert(
            "momentum_generated_at".into(),
            serde_json::json!(mom.generated_at),
        );
    }
    serde_json::Value::Object(components).to_string()
}

impl MomentumContext {
    pub fn new(season: i32, rating: Option<Form>, vibe: Option<Mood>, snapshot: Snapshot) -> Self {
        let input_components_json =
            build_momentum_input_components(rating.as_ref(), vibe.as_ref(), &snapshot);
        let input_hash = hash_components(&input_components_json);
        Self {
            season,
            database_record: None,
            rating,
            vibe,
            snapshot,
            input_components_json,
            input_hash,
        }
    }
}

type ScoutReadingRow = (String, Option<String>, Option<i32>, String, Option<String>);
type InfluencerReadingRow = (String, Option<String>, Option<i16>, String, Option<String>);
async fn load_scout_reading(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Option<Form>> {
    let row: Option<ScoutReadingRow> = sqlx::query_as(
        r#"
        SELECT body, headline, season, generated_at::date::text, input_hash
          FROM (
            SELECT body, headline, season, generated_at, input_hash
              FROM public.stat_summaries
             WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND reporting_date=public.character_reporting_date() AND (NOT (SELECT enabled FROM coverage_schedule) OR candidate_edition=(SELECT edition_date FROM coverage_schedule)) AND season = $4
             ORDER BY generated_at DESC, id DESC
             LIMIT 1
          ) latest
         WHERE body IS NOT NULL AND btrim(body) <> ''
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load Analyst Scout reading {entity_type}/{entity_id}"))?;
    Ok(
        row.map(|(body, headline, season, generated_at, input_hash)| Form {
            body,
            headline,
            season,
            generated_at: Some(generated_at),
            input_hash,
        }),
    )
}

async fn load_influencer_reading(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<Mood>> {
    let row: Option<InfluencerReadingRow> = sqlx::query_as(
        r#"
        SELECT prompt, hook, sentiment, generated_at::date::text, input_hash
          FROM public.vibe_scores
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND reporting_date=public.character_reporting_date() AND (NOT (SELECT enabled FROM coverage_schedule) OR candidate_edition=(SELECT edition_date FROM coverage_schedule))
           AND prompt IS NOT NULL AND btrim(prompt) <> ''
         ORDER BY generated_at DESC, id DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load Analyst Influencer reading {entity_type}/{entity_id}"))?;
    Ok(row.map(
        |(body, headline, sentiment, generated_at, input_hash)| Mood {
            body,
            headline,
            sentiment: sentiment.map(i32::from),
            generated_at: Some(generated_at),
            input_hash,
        },
    ))
}

pub async fn load_momentum_context(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<MomentumContext> {
    let season = oracle::resolve_season(pool, sport, None).await?;
    let (rating, vibe, snapshot, database_record) = tokio::try_join!(
        load_scout_reading(pool, entity_type, entity_id, sport, season),
        load_influencer_reading(pool, entity_type, entity_id, sport),
        load_snapshot(pool, entity_type, entity_id, sport, season),
        crate::tools::meta::load_character_identity(pool, entity_type, entity_id, sport),
    )?;
    let mut context = MomentumContext::new(season, rating, vibe, snapshot);
    context.database_record = database_record;
    context.input_hash = hash_components(&serde_json::to_string(&(
        &context.input_components_json,
        &context.database_record,
        MOMENTUM_SYSTEM_PROMPT,
    ))?);
    Ok(context)
}

/// Preserve the current voice-window reservation; all adapters use these same options.
pub fn generation_options(voice_num_ctx: i32) -> GenerateOptions {
    GenerateOptions {
        system: Some(MOMENTUM_SYSTEM_PROMPT.trim().to_owned()),
        temperature: Some(MOMENTUM_TEMPERATURE),
        num_predict: MOMENTUM_NUM_PREDICT,
        num_ctx: voice_num_ctx,
        json_mode: false,
        format_schema: Some(prose().schema()),
        format_schema_raw: None,
    }
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Parts {
    pub subject: EntityMeta,
    #[serde(default)]
    pub database_record: Option<crate::tools::meta::DatabaseRecord>,
    pub scout: Option<Form>,
    pub influencer: Option<Mood>,
    #[serde(default)]
    pub snapshot: Snapshot,
}
impl Parts {
    pub fn assemble(&self) -> String {
        crate::tools::context::assemble(
            MOMENTUM_SYSTEM_PROMPT,
            include_str!("context.txt"),
            self.subject.for_writing(),
            self.database_record.as_ref(),
            super::fresh::readings(
                self.scout.as_ref(),
                self.influencer.as_ref(),
                &self.snapshot,
            ),
            super::voice::VOICE,
        )
    }
    pub fn should_call(&self) -> bool {
        (self.scout.is_some() && self.influencer.is_some())
            || super::fresh::has_comparison(&self.snapshot)
    }
}

pub fn assemble(
    subject: &EntityMeta,
    scout: Option<&Form>,
    influencer: Option<&Mood>,
    snapshot: &Snapshot,
) -> String {
    Parts {
        subject: subject.clone(),
        database_record: None,
        scout: scout.cloned(),
        influencer: influencer.cloned(),
        snapshot: snapshot.clone(),
    }
    .assemble()
}

async fn load_snapshot(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: i32,
) -> Result<Snapshot> {
    let value: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'vibe_slope', vibe_slope, 'vibe_samples', vibe_samples,
            'vibe_window_start', vibe_window_start, 'vibe_window_end', vibe_window_end,
            'rating_slope', rating_slope, 'rating_samples', rating_samples,
            'rating_window_start', rating_window_start, 'rating_window_end', rating_window_end,
            'momentum_score', momentum_score, 'generated_at', generated_at)
         FROM public.momentum_scores
         WHERE entity_type=$1 AND entity_id=$2 AND sport=$3 AND season=$4
           AND reporting_date=public.character_reporting_date()
           AND (NOT (SELECT enabled FROM coverage_schedule) OR candidate_edition=(SELECT edition_date FROM coverage_schedule))
         ORDER BY generated_at DESC, id DESC LIMIT 1")
        .bind(entity_type).bind(entity_id).bind(sport).bind(season).fetch_optional(pool).await?;
    value
        .map(serde_json::from_value)
        .transpose()
        .map(|s| s.unwrap_or_default())
        .map_err(Into::into)
}

pub fn prose() -> crate::tools::form::Prose {
    crate::tools::form::Prose::new(&["blurb"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_readings_and_study_without_scores_or_hashes() {
        let subject = EntityMeta {
            name: "Jordan Sample".into(),
            entity_type: "player".into(),
            entity_id: 7,
            sport: "NFL".into(),
        };
        let scout = Form {
            body: "Full measured reading. Its qualifier stays here.".into(),
            headline: None,
            season: Some(2026),
            generated_at: Some("2026-10-01".into()),
            input_hash: Some("hidden".into()),
        };
        let snapshot = Snapshot {
            rating_slope: Some(2.0),
            rating_samples: 4,
            rating_window_start: Some("2026-09-01".into()),
            rating_window_end: Some("2026-09-30".into()),
            momentum_score: Some(71.0),
            ..Snapshot::default()
        };
        let world = assemble(&subject, Some(&scout), None, &snapshot);
        let value: serde_json::Value = serde_json::from_str(&world).unwrap();
        assert!(world.starts_with(r#"{"job":"#));
        assert_eq!(value["fresh"]["scout"]["body"], scout.body);
        assert!(value["fresh"].get("trajectory").is_none());
        assert!(value["fresh"].get("influencer").is_none());
        assert!(world.find("hidden").is_none());
        assert!(world.find("momentum_score").is_none());
    }
}

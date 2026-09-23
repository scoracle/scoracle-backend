//! Oracle application adapter: five-pillar retrieval, readiness, routing, publication, and work.

use crate::application::models::Models;
use crate::application::products::EntityKey;
use crate::application::queue::publication::ClaimPublication;
use crate::application::queue::work::{self, Item, TaskKey};
use crate::evidence::corpus::load_transfer_heat;
use crate::evidence::memories::{self, MemoryRequest, Mission};
use crate::evidence::trajectory::DEFAULT_TRAJECTORY;
use crate::plugins::insider::cognition::HeatItem;
use crate::plugins::oracle::cognition::{
    self as oracle, Assignment, Cards, SigilOutput, Subject, SynthMomentum, SynthNarrative,
    SynthRating, SynthTransfer, SynthVibe, CROWN_CARD_BODY_CAP, ORACLE_NUM_PREDICT,
    ORACLE_OUTPUT_CONTRACT_VERSION, ORACLE_TEMPERATURE,
};
use crate::runtime::ledger::{insert_generation_ledger_best_effort, LedgerEvent, LedgerSpec};
use crate::runtime::route::Role;
use crate::studio::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::studio::Studio;
use crate::util::hash_components;
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use tracing::debug;

const ORACLE_LEDGER: LedgerSpec = LedgerSpec {
    plugin_id: crate::plugins::oracle::manifest::MANIFEST.id.as_str(),
    stage: "sigil",
    lens: "oracle",
    role: Role::OracleLogic,
    product_table: "sigil_synthesis",
    output_contract_version: ORACLE_OUTPUT_CONTRACT_VERSION,
};

const PILLAR_STAGES: [TaskKey; 5] = [
    crate::plugins::journalist::manifest::TASK,
    crate::plugins::scout::manifest::TASK,
    crate::plugins::influencer::manifest::TASK,
    crate::plugins::analyst::manifest::TASK,
    crate::plugins::insider::manifest::TASK,
];

/// True when no pillar stage still owes this entity work. Failed pillars count as
/// settled at every attempt level, preserving the existing partial-read policy.
pub async fn pillars_settled(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i64,
    sport: &str,
) -> Result<bool> {
    let stages: Vec<&str> = PILLAR_STAGES.iter().map(|stage| stage.as_str()).collect();
    let settled = sqlx::query_scalar(
        r#"
        SELECT NOT EXISTS (
            SELECT 1
              FROM pipeline_work
             WHERE entity_type = $1
               AND entity_id   = $2
               AND sport       = $3
               AND stage       = ANY($4)
               AND status <> 'failed'
        )
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(&stages)
    .fetch_one(pool)
    .await
    .with_context(|| format!("pillars_settled {entity_type}/{entity_id}"))?;
    Ok(settled)
}

/// Existing Oracle barrier policy: pending/running pillars block, while failed pillars count as
/// settled at every attempt level. A later successful retry offers Oracle again.
pub async fn enqueue_oracle_if_pillars_settled(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i64,
    sport: &str,
    input_version: Option<String>,
) -> Result<bool> {
    if !pillars_settled(pool, entity_type, entity_id, sport).await? {
        debug!(%entity_type, entity_id, %sport, "oracle barrier: pillars still outstanding");
        return Ok(false);
    }
    work::enqueue(
        pool,
        &Item {
            stage: crate::plugins::oracle::manifest::TASK,
            entity_type: entity_type.to_string(),
            entity_id,
            sport: sport.to_string(),
            input_version,
            attempts: 0,
            claim_token: None,
        },
    )
    .await?;
    debug!(%entity_type, entity_id, %sport, "oracle barrier: enqueued sigil");
    Ok(true)
}

pub(crate) struct OracleBarrierReaction {
    pool: PgPool,
}

impl OracleBarrierReaction {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl crate::application::queue::outbox::EventReaction for OracleBarrierReaction {
    fn name(&self) -> &'static str {
        "oracle.completion-barrier"
    }

    fn kinds(&self) -> &[&'static str] {
        &[
            crate::plugins::influencer::adapter::VIBE_COMPLETED,
            crate::plugins::scout::adapter::RATING_COMPLETED,
            crate::plugins::analyst::adapter::MOMENTUM_COMPLETED,
            crate::plugins::scout::adapter::RATING_DEBOUNCED,
            crate::plugins::journalist::adapter::NARRATIVES_COMPLETED,
            crate::plugins::insider::adapter::TRANSFER_PUBLISHED,
        ]
    }

    async fn react(&self, event: &crate::application::queue::outbox::Event) -> Result<()> {
        enqueue_oracle_if_pillars_settled(
            &self.pool,
            &event.entity_type,
            i64::from(event.entity_id),
            &event.sport,
            event.source_input_version.clone(),
        )
        .await?;
        Ok(())
    }
}

pub async fn resolve_season(pool: &PgPool, sport: &str, want: Option<i32>) -> Result<i32> {
    if let Some(season) = want {
        return Ok(season);
    }
    sqlx::query_scalar("SELECT current_season FROM public.sports WHERE id = $1")
        .bind(sport)
        .fetch_one(pool)
        .await
        .with_context(|| format!("resolve current_season for {sport}"))
}

pub async fn load_narrative_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Vec<SynthNarrative>> {
    let rows: Vec<(String, String, i32, String, i32, Option<i32>)> = sqlx::query_as(
        r#"
        SELECT narrative_title, body, COALESCE(impact, 0), COALESCE(trajectory, $4),
               COALESCE(source_count, 0),
               EXTRACT(day FROM NOW() - source_latest_at)::int
          FROM news_summaries
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND body IS NOT NULL
           AND generated_at = (
               SELECT max(generated_at) FROM news_summaries
                WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           )
         ORDER BY impact DESC NULLS LAST
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(DEFAULT_TRAJECTORY)
    .fetch_all(pool)
    .await
    .with_context(|| format!("load narrative pillar {entity_type}/{entity_id}"))?;
    Ok(rows
        .into_iter()
        .map(
            |(title, body, impact, trajectory, source_count, source_age_days)| SynthNarrative {
                title,
                body,
                impact: f64::from(impact),
                trajectory,
                source_count,
                source_age_days,
            },
        )
        .collect())
}

pub async fn load_rating_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<Option<SynthRating>> {
    let row: Option<(Option<String>, i32, String, String)> = sqlx::query_as(
        r#"
        SELECT body, COALESCE(notability, 0),
               COALESCE(rating_trajectory, 'steady'), COALESCE(rating_trajectory_label, '')
          FROM stat_summaries
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND ($4::int IS NULL OR season = $4)
         ORDER BY generated_at DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load rating pillar {entity_type}/{entity_id}"))?;
    Ok(match row {
        Some((Some(body), notability, rating_trajectory, rating_trajectory_label)) => {
            Some(SynthRating {
                body,
                notability,
                rating_trajectory,
                rating_trajectory_label,
            })
        }
        _ => None,
    })
}

pub async fn load_vibe_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<Option<SynthVibe>> {
    let row: Option<(Option<i16>, String)> = sqlx::query_as(
        r#"
        SELECT sentiment, COALESCE(prompt, '')
          FROM vibe_scores
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
         ORDER BY generated_at DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load vibe pillar {entity_type}/{entity_id}"))?;
    Ok(match row {
        Some((Some(sentiment), prompt)) => Some(SynthVibe {
            sentiment: i32::from(sentiment),
            prompt,
        }),
        _ => None,
    })
}

pub async fn load_momentum_pillar(
    pool: &PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
    season: Option<i32>,
) -> Result<SynthMomentum> {
    #[allow(clippy::type_complexity)]
    let row: Option<(
        Option<String>,
        Option<i16>,
        Option<String>,
        Option<String>,
        serde_json::Value,
    )> = sqlx::query_as(
        r#"
        SELECT direction, score, blurb, input_hash, COALESCE(input_components, '{}'::jsonb)
          FROM momentum_summaries
         WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
           AND ($4::int IS NULL OR season = $4)
         ORDER BY generated_at DESC
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(sport)
    .bind(season)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("load momentum pillar {entity_type}/{entity_id}"))?;
    let Some((direction, score, blurb, input_hash, components)) = row else {
        return Ok(SynthMomentum::default());
    };
    Ok(SynthMomentum {
        direction: direction.filter(|value| !value.trim().is_empty()),
        blurb: blurb.filter(|value| !value.trim().is_empty()),
        input_hash,
        vibe_slope: components
            .get("momentum_vibe_slope")
            .and_then(serde_json::Value::as_f64),
        vibe_samples: components
            .get("momentum_vibe_samples")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_default() as i32,
        rating_slope: components
            .get("momentum_rating_slope")
            .and_then(serde_json::Value::as_f64),
        rating_samples: components
            .get("momentum_rating_samples")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_default() as i32,
        momentum_score: score.map(f64::from),
    })
}

fn transfer_card(item: HeatItem) -> SynthTransfer {
    SynthTransfer {
        counterparty: item.counterparty,
        heat: item.heat,
        direction: item.direction,
        stage: item.stage,
        summary: item.summary,
    }
}

pub async fn load_pillars(
    pool: &sqlx::PgPool,
    entity_type: &str,
    entity_id: i32,
    sport: &str,
) -> Result<(i32, Cards)> {
    let season = resolve_season(pool, sport, None).await?;
    let (narratives, rating, vibe, momentum, transfers) = tokio::try_join!(
        load_narrative_pillar(pool, entity_type, entity_id, sport),
        load_rating_pillar(pool, entity_type, entity_id, sport, Some(season)),
        load_vibe_pillar(pool, entity_type, entity_id, sport),
        load_momentum_pillar(pool, entity_type, entity_id, sport, Some(season)),
        load_transfer_heat(pool, entity_type, entity_id, sport),
    )?;
    Ok((
        season,
        Cards {
            narratives,
            rating,
            vibe,
            momentum,
            transfers: transfers.into_iter().map(transfer_card).collect(),
        },
    ))
}

enum Prepared {
    Debounced,
    Product {
        output: Box<SigilOutput>,
        previous_score: Option<i16>,
    },
}

async fn prepare(pool: &sqlx::PgPool, models: &Models, item: &Item) -> Result<Prepared> {
    let entity_id = item.entity_id_i32()?;
    let name = crate::evidence::corpus::lookup_entity_name(
        pool,
        &item.entity_type,
        entity_id,
        &item.sport,
    )
    .await?;
    let sport = item.sport.to_uppercase();
    let (season, cards) = load_pillars(pool, &item.entity_type, entity_id, &sport).await?;
    if cards.readiness() == oracle::Readiness::Empty {
        let backend = models.router.for_role(Role::OracleLogic);
        let assignment = Assignment {
            subject: Subject {
                entity_type: item.entity_type.clone(),
                entity_name: name,
                sport: item.sport.clone(),
            },
            season,
            cards,
            identity: None,
            input_components_json: "{}".to_string(),
            input_hash: String::new(),
            body_cap: None,
            options: oracle::generation_options(ORACLE_TEMPERATURE, models.voice_num_ctx),
        };
        let output = oracle::create(&Studio::new(backend.as_ref()), &assignment).await?;
        return Ok(Prepared::Product {
            output: Box::new(output),
            previous_score: None,
        });
    }

    let components = oracle::build_synthesis_input_components(
        &cards.narratives,
        cards.rating.as_ref(),
        cards.vibe.as_ref(),
        &cards.momentum,
        &cards.transfers,
    );
    let mut request = MemoryRequest::new(Mission::Oracle, &item.entity_type, entity_id, &sport);
    request.season = Some(season);
    let memories = memories::load(pool, request).await?;
    let input_components_json = memories.with_input_components(&components)?;
    let input_hash = hash_components(&input_components_json);
    let (previous_score, latest_hash) = crate::application::products::latest_with_hash(
        pool,
        "sigil_synthesis",
        &EntityKey {
            entity_type: item.entity_type.clone(),
            entity_id,
            sport: sport.clone(),
            season: Some(season),
        },
    )
    .await?;
    if latest_hash.as_deref() == Some(input_hash.as_str()) {
        return Ok(Prepared::Debounced);
    }
    let identity = Some(memories.render_for_model()?);
    let backend = models.router.for_role(Role::OracleLogic);
    let assignment = Assignment {
        subject: Subject {
            entity_type: item.entity_type.clone(),
            entity_name: name,
            sport: item.sport.clone(),
        },
        season,
        cards,
        identity,
        input_components_json,
        input_hash,
        body_cap: crate::studio::model::small_voice_window(models.voice_num_ctx)
            .then_some(CROWN_CARD_BODY_CAP),
        options: oracle::generation_options(ORACLE_TEMPERATURE, models.voice_num_ctx),
    };
    let output = oracle::create(&Studio::new(backend.as_ref()), &assignment).await?;
    Ok(Prepared::Product {
        output: Box::new(output),
        previous_score,
    })
}

async fn insert_sigil(
    tx: &mut Transaction<'_, Postgres>,
    item: &Item,
    sport: &str,
    output: &SigilOutput,
    previous_score: Option<i16>,
) -> Result<i64> {
    let score = output.score.map(|value| value as i16);
    let convergence = output.convergence.map(|value| value as i16);
    let row = sqlx::query(
        r#"
        INSERT INTO sigil_synthesis (
            entity_type, entity_id, sport, season, trigger_type, trigger_payload,
            score, previous_score, input_components, input_hash,
            model_version, prompt_version, convergence,
            reading, headline, omen, voiced_score,
            voiced_at, voice_model_version, voice_prompt_version
        ) VALUES ($1,$2,$3,$4,'periodic','{}'::jsonb,$5,$6,$7::jsonb,$8,$9,$10,$11,
                  $12,$13,$14,$15,
                  CASE WHEN $12 IS NOT NULL THEN NOW() END,
                  CASE WHEN $12 IS NOT NULL THEN $9 END,
                  CASE WHEN $12 IS NOT NULL THEN $10 END)
        RETURNING id
        "#,
    )
    .bind(&item.entity_type)
    .bind(item.entity_id_i32()?)
    .bind(sport)
    .bind(output.season)
    .bind(score)
    .bind(previous_score)
    .bind(&output.input_components_json)
    .bind(output.provenance.input_hash.as_deref())
    .bind(&output.provenance.model_version)
    .bind(output.provenance.prompt_version)
    .bind(convergence)
    .bind(output.reading.as_deref())
    .bind(output.headline.as_deref())
    .bind(output.omen)
    .bind(score)
    .fetch_one(&mut **tx)
    .await
    .context("persist sigil")?;
    Ok(row.get("id"))
}

async fn commit_claimed(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    prepared: &Prepared,
) -> Result<(PluginOutcome, Option<i64>)> {
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok((PluginOutcome::Superseded, None));
    };
    let product_row_id = match prepared {
        Prepared::Debounced => None,
        Prepared::Product {
            output,
            previous_score,
        } => Some(
            insert_sigil(
                publication.transaction(),
                item,
                sport,
                output,
                *previous_score,
            )
            .await?,
        ),
    };
    publication.commit_final().await?;
    Ok((PluginOutcome::Committed, product_row_id))
}

async fn record_ledger(
    pool: &PgPool,
    item: &Item,
    sport: &str,
    output: &SigilOutput,
    product_row_id: i64,
) {
    insert_generation_ledger_best_effort(
        pool,
        output,
        ORACLE_LEDGER,
        LedgerEvent {
            entity_type: &item.entity_type,
            entity_id: item.entity_id_i32().unwrap_or_default(),
            sport,
            pair_entity: None,
            trigger_type: "periodic",
            trigger_payload: serde_json::json!({}),
            product_row_ids: vec![product_row_id],
            included_evidence: serde_json::json!({
                "input_components": serde_json::from_str::<serde_json::Value>(
                    &output.input_components_json
                ).unwrap_or_else(|_| serde_json::json!({
                    "raw_input_components": &output.input_components_json
                })),
                "score": output.score,
                "convergence": output.convergence,
                "omen": output.omen,
            }),
            excluded_evidence: if output.was_called() {
                serde_json::json!([])
            } else {
                serde_json::json!([{
                    "reason": "no_narrative_rating_vibe_momentum_or_transfer_pillar"
                }])
            },
            context_budget: output.context_budget(serde_json::json!({
                "num_predict": ORACLE_NUM_PREDICT,
            })),
            parser_outcome: if output.was_called() {
                "parsed"
            } else {
                "no_call"
            },
        },
    )
    .await;
}

pub struct SigilHandler {
    pool: sqlx::PgPool,
    models: std::sync::Arc<Models>,
}

impl SigilHandler {
    pub fn new(pool: sqlx::PgPool, models: std::sync::Arc<Models>) -> Self {
        Self { pool, models }
    }
}

#[async_trait]
impl StudioPlugin for SigilHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &crate::plugins::oracle::manifest::MANIFEST
    }

    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        let pool = &self.pool;
        let models = &self.models;
        let sport = item.sport.to_uppercase();
        let prepared = prepare(pool, models, item).await?;
        let (outcome, product_row_id) = commit_claimed(pool, item, &sport, &prepared).await?;
        if let (PluginOutcome::Committed, Some(product_row_id), Prepared::Product { output, .. }) =
            (&outcome, product_row_id, &prepared)
        {
            record_ledger(pool, item, &sport, output, product_row_id).await;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests;

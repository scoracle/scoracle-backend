//! Durable, packet-free scored-board wraps after source pair verdicts.
use super::*;
use crate::plugins::insider::cognition::{self as insider, HeatItem, InsiderScore};
use crate::studio::{Generation, Studio};

enum PreparedWrap {
    Skipped {
        reason: &'static str,
        input_hash: Option<String>,
    },
    Scored {
        generation: Generation<InsiderScore>,
        previous_score: Option<i16>,
        heat: Vec<HeatItem>,
    },
}

fn version(item: &Item) -> Result<&str> {
    item.input_version
        .as_deref()
        .context("Harvester Insider wrap has no work version")
}

pub(super) async fn execute(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
    models: Option<&ExecutionCapabilities>,
) -> Result<PluginOutcome> {
    if let Some(outcome) = super::identity::execute(pool, item, models).await? {
        return Ok(outcome);
    }
    let team_id = item.entity_id_i32()?;
    let sport = item.sport.to_uppercase();
    let work_version = version(item)?;
    let next: Option<(String, i32)> = sqlx::query_as(
        "SELECT entity_type,entity_id FROM public.harvester_insider_wraps \
         WHERE team_id=$1 AND sport=$2 AND work_version=$3 AND status='pending' \
         ORDER BY entity_type,entity_id LIMIT 1",
    )
    .bind(team_id)
    .bind(&sport)
    .bind(work_version)
    .fetch_optional(pool)
    .await?;
    if let Some((entity_type, entity_id)) = next {
        return score_one(
            pool,
            backend,
            voice_num_ctx,
            item,
            &sport,
            work_version,
            &entity_type,
            entity_id,
        )
        .await;
    }
    let existing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_insider_wraps \
         WHERE team_id=$1 AND sport=$2 AND work_version=$3",
    )
    .bind(team_id)
    .bind(&sport)
    .bind(work_version)
    .fetch_one(pool)
    .await?;
    if existing == 0 {
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(PluginOutcome::Superseded);
        };
        sqlx::query(
            "INSERT INTO public.harvester_insider_wraps \
             (team_id,sport,work_version,entity_type,entity_id) \
             VALUES($1,$2,$3,'team',$1) ON CONFLICT DO NOTHING",
        )
        .bind(team_id)
        .bind(&sport)
        .bind(work_version)
        .execute(&mut **publication.transaction())
        .await?;
        sqlx::query(
            "INSERT INTO public.harvester_insider_wraps \
             (team_id,sport,work_version,entity_type,entity_id) \
             SELECT $1,$2,$3,'player',v.player_id FROM ( \
               SELECT p.subject_id AS player_id \
                 FROM public.harvester_insider_pairs p \
                 JOIN public.harvester_classifications c ON c.id=p.classification_id \
                WHERE c.entity_type='team' AND c.entity_id=$1 AND c.sport=$2 \
                  AND c.created_at>now()-interval '7 days' \
                  AND p.subject_type='player' AND p.status IN ('rumor','cleared') \
               UNION \
               SELECT tr.player_id FROM public.transfer_rumors tr \
                WHERE tr.team_id=$1 AND tr.sport=$2 AND tr.is_rumor IS TRUE \
                  AND tr.heat>0 AND tr.generated_at>now()-interval '7 days' \
             ) v JOIN public.players pl ON pl.id=v.player_id AND pl.sport=$2 \
             ON CONFLICT DO NOTHING",
        )
        .bind(team_id)
        .bind(&sport)
        .bind(work_version)
        .execute(&mut **publication.transaction())
        .await?;
        publication.commit_progress().await?;
        return Ok(PluginOutcome::deferred(
            "Insider scored-board wraps prepared",
            Duration::from_secs(1),
        ));
    }
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    record_transfer_event(
        publication.transaction(),
        item,
        TRANSFER_PUBLISHED,
        "team",
        team_id,
        item.input_version.as_deref(),
    )
    .await?;
    publication.commit_final().await?;
    Ok(PluginOutcome::Committed)
}

async fn prepare(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    sport: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<PreparedWrap> {
    let heat = load_transfer_heat(pool, entity_type, entity_id, sport).await?;
    if heat.is_empty() {
        let has_row: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM public.insider_scores \
             WHERE entity_type=$1 AND entity_id=$2 AND sport=$3)",
        )
        .bind(entity_type)
        .bind(entity_id)
        .bind(sport)
        .fetch_one(pool)
        .await?;
        if !has_row {
            return Ok(PreparedWrap::Skipped {
                reason: "empty_wire",
                input_hash: None,
            });
        }
    }
    let name =
        crate::evidence::corpus::lookup_entity_name(pool, entity_type, entity_id, sport).await?;
    let mut request = MemoryRequest::new(Mission::Insider, entity_type, entity_id, sport);
    request.include_storyline_history = false;
    let memories = memories::load(pool, request).await?;
    let components =
        memories.with_input_components(&build_insider_score_input_components(&heat))?;
    let input_hash = hash_components(&components);
    let key = EntityKey {
        entity_type: entity_type.into(),
        entity_id,
        sport: sport.into(),
        season: None,
    };
    if crate::application::products::debounce_unchanged(pool, "insider_scores", &key, &input_hash)
        .await?
    {
        return Ok(PreparedWrap::Skipped {
            reason: "unchanged_board",
            input_hash: Some(input_hash),
        });
    }
    let generation = insider::create_score(
        &Studio::new(backend),
        &name,
        &heat,
        &insider::score_options(voice_num_ctx),
        input_hash,
    )
    .await?;
    Ok(PreparedWrap::Scored {
        generation,
        previous_score: memories.previous_score,
        heat,
    })
}

#[allow(clippy::too_many_arguments)]
async fn score_one(
    pool: &PgPool,
    backend: &dyn Inference,
    voice_num_ctx: i32,
    item: &Item,
    sport: &str,
    work_version: &str,
    entity_type: &str,
    entity_id: i32,
) -> Result<PluginOutcome> {
    let prepared = prepare(pool, backend, voice_num_ctx, sport, entity_type, entity_id).await?;
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(PluginOutcome::Superseded);
    };
    let (status, score_id, product_ref) = match &prepared {
        PreparedWrap::Skipped { reason, input_hash } => (
            "skipped",
            None,
            serde_json::json!({"reason":reason,"input_hash":input_hash}),
        ),
        PreparedWrap::Scored {
            generation,
            previous_score,
            heat,
        } => {
            let row_id: i64 = sqlx::query_scalar(
                "INSERT INTO public.insider_scores \
                 (sport,entity_type,entity_id,score,previous_score,read,headline, \
                  model_version,prompt_version,input_hash,generated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW()) RETURNING id",
            )
            .bind(sport)
            .bind(entity_type)
            .bind(entity_id)
            .bind(generation.product.score)
            .bind(previous_score)
            .bind(&generation.product.read)
            .bind(generation.product.headline.as_deref())
            .bind(&generation.provenance.model_version)
            .bind(generation.provenance.prompt_version)
            .bind(generation.provenance.input_hash.as_deref())
            .fetch_one(&mut **publication.transaction())
            .await?;
            record_transfer_event(
                publication.transaction(),
                item,
                TRANSFER_PUBLISHED,
                entity_type,
                entity_id,
                Some(&row_id.to_string()),
            )
            .await?;
            (
                "scored",
                Some(row_id),
                serde_json::json!({
                    "score_id":row_id,
                    "model_version":generation.provenance.model_version,
                    "prompt_version":generation.provenance.prompt_version,
                    "input_hash":generation.provenance.input_hash,
                    "active_rumors":heat.len(),
                }),
            )
        }
    };
    let changed = sqlx::query(
        "UPDATE public.harvester_insider_wraps \
         SET status=$6,score_id=$7,product_ref=$8,updated_at=now() \
         WHERE team_id=$1 AND sport=$2 AND work_version=$3 \
           AND entity_type=$4 AND entity_id=$5 AND status='pending'",
    )
    .bind(item.entity_id_i32()?)
    .bind(sport)
    .bind(work_version)
    .bind(entity_type)
    .bind(entity_id)
    .bind(status)
    .bind(score_id)
    .bind(product_ref)
    .execute(&mut **publication.transaction())
    .await?;
    ensure!(
        changed.rows_affected() == 1,
        "Insider score wrap changed during claim"
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public.harvester_insider_wraps \
         WHERE team_id=$1 AND sport=$2 AND work_version=$3 AND status='pending'",
    )
    .bind(item.entity_id_i32()?)
    .bind(sport)
    .bind(work_version)
    .fetch_one(&mut **publication.transaction())
    .await?;
    if remaining == 0 {
        record_transfer_event(
            publication.transaction(),
            item,
            TRANSFER_PUBLISHED,
            "team",
            item.entity_id_i32()?,
            item.input_version.as_deref(),
        )
        .await?;
        publication.commit_final().await?;
    } else {
        publication.commit_progress().await?;
    }
    if let PreparedWrap::Scored {
        generation,
        previous_score,
        heat,
    } = &prepared
    {
        insert_generation_ledger_best_effort(pool,generation,INSIDER_SCORE_LEDGER,LedgerEvent {
            entity_type,entity_id,sport,pair_entity:None,trigger_type:"periodic",
            trigger_payload:serde_json::json!({"team_id":item.entity_id_i32()?,"work_version":work_version}),
            product_row_ids:vec![score_id.context("scored Insider wrap has no score row")?],
            included_evidence:serde_json::json!({
                "active_rumors":heat.len(),"score":generation.product.score,
                "previous_score":previous_score,
            }),
            excluded_evidence:serde_json::json!([]),
            context_budget:generation.context_budget(serde_json::json!({
                "num_predict":crate::studio::palette::PALETTE_NUM_PREDICT,
            })),
            parser_outcome:"parsed",
        }).await;
    }
    if remaining == 0 {
        Ok(PluginOutcome::Committed)
    } else {
        Ok(PluginOutcome::deferred(
            format!("{remaining} Insider scored-board wraps remain"),
            Duration::from_secs(1),
        ))
    }
}

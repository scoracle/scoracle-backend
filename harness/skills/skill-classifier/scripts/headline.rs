//! Persist headline decisions and failures before character delivery.
use super::{adapter, decision, hash, Source};
use crate::harness::{queue::publication::ClaimPublication, queue::work::Item};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sqlx::PgPool;

pub(super) async fn subject_meta(pool: &PgPool, candidate: &Value) -> Result<Value> {
    let entity: crate::tools::meta::EntityMeta = serde_json::from_value(candidate.clone())?;
    let record = crate::tools::meta::load_identity_record(
        pool,
        &entity.entity_type,
        entity.entity_id,
        &entity.sport,
    )
    .await?
    .context("Classifier canonical subject record missing")?;
    let mut meta = json!({"identity":entity,"identity_record":record,"origin":"canonical_meta_rs",
        "evidence_scope":"Subject identity only; not article evidence."});
    meta["database_record"] = json!(
        crate::tools::meta::load_character_identity(
            pool,
            &entity.entity_type,
            entity.entity_id,
            &entity.sport,
        )
        .await?
    );
    if entity.entity_type == "team" {
        meta["team_gender"] = json!(
            crate::tools::meta::load_team_gender(pool, entity.entity_id, &entity.sport,).await?
        );
    }
    Ok(meta)
}

/// Only people actually named in the headline; never a whole roster or query candidates.
#[cfg(test)]
pub(super) async fn named_people(pool: &PgPool, headline: &str, sport: &str) -> Result<Value> {
    Ok(sqlx::query_scalar(
        r#"WITH named AS (
            SELECT c->>'entity_type' kind,(c->>'entity_id')::integer id,c->>'name' name
            FROM jsonb_array_elements(public.classifier_identity_candidates($1,$2,'')) c
            WHERE c->>'entity_type' IN ('player','person')
        ) SELECT COALESCE(jsonb_agg(jsonb_strip_nulls(jsonb_build_object(
            'entity_type',n.kind,'entity_id',n.id,'name',n.name,
            'team',CASE WHEN pci.source IN ('roster','override') THEN t.name END,
            'team_id',CASE WHEN pci.source IN ('roster','override') THEN t.id END,
            'source',pci.source,'observed_at',pci.source_updated_at::date::text))
            ORDER BY n.kind,n.id),'[]'::jsonb)
        FROM named n LEFT JOIN public.player_current_identity pci
            ON n.kind='player' AND pci.player_id=n.id AND pci.sport=$1
        LEFT JOIN public.teams t ON t.id=pci.team_id AND t.sport=pci.sport"#,
    )
    .bind(sport)
    .bind(headline)
    .fetch_one(pool)
    .await?)
}

pub(super) async fn measure(
    pool: &PgPool,
    decision: &decision::Decision,
    item: &Item,
    before: &Source,
) -> Result<Option<Vec<Value>>> {
    let title = before.provenance["title"]
        .as_str()
        .context("RSS headline")?;
    let model = decision.identity().await?;
    let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
        return Ok(None);
    };
    adapter::fence_source(pool, item, before, &mut publication).await?;
    let source_id: i64 = sqlx::query_scalar("INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source) VALUES($1,$2,$3,$4,$5) ON CONFLICT(article_id,sport,input_hash,body_sha256,identity_hash) DO UPDATE SET input_hash=classifier_sources.input_hash RETURNING id")
        .bind(item.entity_id).bind(&item.sport).bind(adapter::input_hash(before)?)
        .bind(hash("")).bind(json!(before)).fetch_one(&mut **publication.transaction()).await?;
    publication.commit_progress().await?;
    let mut results = Vec::new();
    for candidate in &before.query_entities {
        let meta = subject_meta(pool, candidate).await?;
        let target = json!({"entity_type":meta["identity"]["entity_type"],"entity_id":meta["identity"]["entity_id"],"sport":item.sport});
        let request = crate::harness::skills::classifier::request::request(
            title,
            &before.source,
            &item.sport,
            &meta,
        )?;
        let request_hash = hash(&request.to_string());
        let cached: Option<Value> = sqlx::query_scalar("SELECT receipt->'measurement' FROM classifier_measurements WHERE sport=$1 AND request_hash=$2 AND status='source_bound_provisional' AND receipt#>>'{measurement,response,model}'=$3 AND receipt->'target'=$4 ORDER BY id DESC LIMIT 1")
            .bind(&item.sport).bind(&request_hash).bind(&model).bind(&target).fetch_optional(pool).await?;
        if let Some(receipt) = cached {
            ensure!(
                receipt["request"] == request,
                "cached headline request mismatch"
            );
            let measured = decision::Measurement::from_receipt(receipt)?;
            results.push(json!({"target":target,"measurement":measured.receipt}));
            continue;
        }
        let measured = decision
            .spectrum(title, &before.source, &item.sport, &meta)
            .await;
        let Some(mut publication) = ClaimPublication::begin(pool, item).await? else {
            return Ok(None);
        };
        adapter::fence_source(pool, item, before, &mut publication).await?;
        let (status, receipt) = match &measured {
            Ok(value) => {
                ensure!(
                    value.model == model,
                    "Classifier serving model changed during headline work"
                );
                (
                    "source_bound_provisional",
                    json!({"status":"source_bound_provisional","production_eligible":false,
                    "target":target,"measurement":value.receipt}),
                )
            }
            Err(error) => (
                "error",
                json!({"status":"error","production_eligible":false,"target":target,"request":request,
                "request_version":decision::REQUEST_VERSION,
                "error":format!("{error:#}"),"raw_response":error.downcast_ref::<crate::harness::model::ResponseFailure>().map(|failure| &failure.raw_response_body)}),
            ),
        };
        sqlx::query("INSERT INTO classifier_measurements(source_id,article_id,sport,request_hash,status,receipt) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(source_id).bind(item.entity_id).bind(&item.sport).bind(&request_hash).bind(status).bind(receipt)
            .execute(&mut **publication.transaction()).await?;
        publication.commit_progress().await?;
        let measured = measured?;
        results.push(json!({"target":target,"measurement":measured.receipt}));
    }
    ensure!(!results.is_empty(), "Classifier requires RSS subjects");
    Ok(Some(results))
}

#[cfg(test)]
#[path = "../../../tests/internal/classifier/headline.rs"]
mod gender_tests;

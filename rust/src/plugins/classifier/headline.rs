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
    if entity.entity_type == "team" {
        meta["team_gender"] = json!(
            crate::tools::meta::load_team_gender(pool, entity.entity_id, &entity.sport,).await?
        );
    }
    Ok(meta)
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
        let request = crate::plugins::classifier::character_instructions::request(
            title,
            &before.source,
            &item.sport,
            &meta,
        )?;
        let request_hash = hash(&request.to_string());
        let cached: Option<Value> = sqlx::query_scalar("SELECT receipt->'measurement' FROM classifier_measurements WHERE sport=$1 AND request_hash=$2 AND status='source_bound_provisional' AND receipt#>>'{measurement,response,model}'=$3 ORDER BY id DESC LIMIT 1")
            .bind(&item.sport).bind(&request_hash).bind(&model).fetch_optional(pool).await?;
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
mod gender_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires migrated disposable classifier_test_coverage database; run serially"]
    async fn canonical_team_gender_reaches_the_decision_target() -> Result<()> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&std::env::var("TEST_DATABASE_URL")?)
            .await?;
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await?;
        ensure!(
            database.starts_with("classifier_test_coverage"),
            "requires migrated disposable coverage test database"
        );
        sqlx::raw_sql(
            r#"
            INSERT INTO public.teams(id,sport,name,meta) VALUES
                (991801,'FOOTBALL','Cedar United','{"gender":"men"}'),
                (991802,'FOOTBALL','Cedar United','{"gender":"women"}'),
                (991803,'FOOTBALL','Cedar United','{}');
        "#,
        )
        .execute(&pool)
        .await?;
        for (id, gender, target) in [
            (
                991801,
                Some("men"),
                "the men's football team named Cedar United",
            ),
            (
                991802,
                Some("women"),
                "the women's football team named Cedar United",
            ),
            (991803, None, "Cedar United, the FOOTBALL team"),
        ] {
            let meta = subject_meta(
                &pool,
                &json!({"name":"Cedar United","entity_type":"team",
                "entity_id":id,"sport":"FOOTBALL"}),
            )
            .await?;
            let payload = crate::plugins::classifier::character_instructions::request(
                "Cedar United Women appoint Jo Vale",
                "Fixture Wire",
                "FOOTBALL",
                &meta,
            )?;
            assert_eq!(
                payload["state"]["subject_meta"]["team_gender"],
                json!(gender)
            );
            for question in payload["questions"].as_object().unwrap().values() {
                assert!(question["instructions"].as_str().unwrap().contains(target));
            }
        }
        sqlx::query(
            "DELETE FROM public.teams WHERE sport='FOOTBALL' AND id BETWEEN 991801 AND 991803",
        )
        .execute(&pool)
        .await?;
        pool.close().await;
        Ok(())
    }
}

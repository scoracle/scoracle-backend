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
        let request = crate::plugins::classifier::skills::classify::request::request(
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
mod gender_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires migrated disposable classifier_test_coverage database; run serially"]
    async fn named_people_and_explicit_league_category_reach_the_same_request_builder() -> Result<()>
    {
        let pool = crate::harness::dbtest::pool("disposable relationship context database").await;
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await?;
        ensure!(
            database.starts_with("classifier_test_coverage"),
            "requires disposable database"
        );
        sqlx::raw_sql(r#"
            INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_RELATIONS','Relation fixtures',2026);
            INSERT INTO leagues(id,sport,name,meta) VALUES(991901,'ZZ_RELATIONS','Fixture Cup','{"gender":"women"}');
            INSERT INTO teams(id,sport,name,league_id,meta) VALUES
                (991901,'ZZ_RELATIONS','Cedar FC',991901,'{}'),
                (991902,'ZZ_RELATIONS','Maple FC',991901,'{"gender":"men"}');
            INSERT INTO players(id,sport,name) VALUES(991903,'ZZ_RELATIONS','Alex Reed Jr.');
            INSERT INTO persons(id,sport,full_name,kind) VALUES(991904,'ZZ_RELATIONS','Alex Reed','player');
            INSERT INTO team_rosters(sport,season,team_id,player_id,last_seen)
                VALUES('ZZ_RELATIONS',2026,991901,991903,'2026-11-01T12:00:00Z');
            INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES
                ('player',991903,'ZZ_RELATIONS','alex reed jr','name'),
                ('person',991904,'ZZ_RELATIONS','alex reed','name');
        "#).execute(&pool).await?;
        let title = "Alex Reed Jr. may be rested";
        let people = named_people(&pool, title, "ZZ_RELATIONS").await?;
        assert_eq!(people.as_array().unwrap().len(), 1);
        assert_eq!(people[0]["name"], "Alex Reed Jr.");
        assert_eq!(people[0]["team"], "Cedar FC");
        let mut meta = subject_meta(&pool,&json!({"name":"Cedar FC","entity_type":"team","entity_id":991901,"sport":"ZZ_RELATIONS"})).await?;
        assert_eq!(meta["team_gender"], "women");
        assert_eq!(
            crate::tools::meta::load_team_gender(&pool, 991902, "ZZ_RELATIONS")
                .await?
                .as_deref(),
            Some("men")
        );
        meta["named_people"] = people;
        let request = super::super::request::request(title, "Wire", "ZZ_RELATIONS", &meta)?;
        assert!(request["state"]
            .as_str()
            .unwrap()
            .contains("recorded affiliation: this target (Cedar FC), observed 2026-11-01"));
        assert!(!request.to_string().contains("991903"));
        let both = named_people(&pool, "Alex Reed praises Alex Reed Jr.", "ZZ_RELATIONS").await?;
        assert_eq!(both.as_array().unwrap().len(), 2);
        for suffix in ["Jr.", "Sr.", "II", "III", "IV"] {
            let matched: bool = sqlx::query_scalar(
                "SELECT public.classifier_name_span_matches($1,'alex reed','person')",
            )
            .bind(format!("Alex Reed {suffix} returns"))
            .fetch_one(&pool)
            .await?;
            assert!(!matched, "{suffix}");
        }
        sqlx::raw_sql("DELETE FROM entity_name_surfaces WHERE sport='ZZ_RELATIONS'; DELETE FROM team_rosters WHERE sport='ZZ_RELATIONS'; DELETE FROM persons WHERE sport='ZZ_RELATIONS'; DELETE FROM players WHERE sport='ZZ_RELATIONS'; DELETE FROM teams WHERE sport='ZZ_RELATIONS'; DELETE FROM leagues WHERE sport='ZZ_RELATIONS'; DELETE FROM sports WHERE id='ZZ_RELATIONS';").execute(&pool).await?;
        pool.close().await;
        Ok(())
    }

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
            let payload = crate::plugins::classifier::skills::classify::request::request(
                "Cedar United Women appoint Jo Vale",
                "Fixture Wire",
                "FOOTBALL",
                &meta,
            )?;
            let state: serde_json::Value =
                serde_json::from_str(payload["state"].as_str().unwrap())?;
            assert_eq!(
                state["target"]["team_category"].as_str(),
                Some(gender.unwrap_or("unknown"))
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

//! Native acquisition and claim-fenced investigation nominations; no accuracy scoring.
use super::*;
use crate::harness::{dbtest, tools::WebBroker, Parser};
use crate::plugins::{classifier, graph::cognition::GraphParser};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires a disposable classifier_test database via TEST_DATABASE_URL"]
async fn classifier_graph_complete_source_nominations_and_recovery() -> Result<()> {
    const SPORT: &str = "ZZ_CLASSIFIER_GRAPH";
    const ARTICLE: i64 = 1041;
    let pool = dbtest::pool("disposable classifier_test database").await;
    classifier::plumbing_tests::setup_disposable(&pool).await?;
    sqlx::raw_sql(
        "CREATE OR REPLACE FUNCTION public.nrm(text) RETURNS text LANGUAGE sql IMMUTABLE AS $$SELECT trim(lower(regexp_replace($1,'[^[:alnum:]]+',' ','g')))$$;
        ALTER TABLE teams ADD COLUMN IF NOT EXISTS city text;
        ALTER TABLE teams ADD COLUMN IF NOT EXISTS country text;
        ALTER TABLE teams ADD COLUMN IF NOT EXISTS venue_name text;
        ALTER TABLE teams ADD COLUMN IF NOT EXISTS conference text;
        ALTER TABLE teams ADD COLUMN IF NOT EXISTS division text;
        ALTER TABLE teams ADD COLUMN IF NOT EXISTS league_id integer;
        ALTER TABLE persons ADD COLUMN IF NOT EXISTS created_at timestamptz DEFAULT now();
        CREATE TABLE IF NOT EXISTS leagues(id integer,sport text,name text,country text);
        CREATE TABLE IF NOT EXISTS fixtures(id integer PRIMARY KEY,start_time timestamptz);",
    )
    .execute(&pool)
    .await?;
    for migration in [
        include_str!("../../../../../sql/migrations/165_graph_stage.sql"),
        include_str!("../../../../../sql/migrations/205_investigator_substrate.sql"),
        include_str!("../../../../../sql/migrations/278_harvester_fixture_reviews.sql"),
        include_str!("../../../../../sql/migrations/279_harvester_fixture_review_input_hash.sql"),
        include_str!("../../../../../sql/migrations/288_graph_fixture_extraction_unavailable.sql"),
    ] {
        // These historical migrations contain non-repeatable ALTER statements.
        if !migration.contains("279_harvester") || !sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version='279_harvester_fixture_review_input_hash')").fetch_one(&pool).await? {
            sqlx::raw_sql(migration).execute(&pool).await?;
        }
    }
    for table in [
        "pipeline_work",
        "graph_extractions",
        "harvester_fixture_reviews",
        "entity_candidates",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(SPORT)
            .execute(&pool)
            .await?;
    }
    sqlx::query("DELETE FROM classifier_sources WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM harvester_query_provenance WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM news_articles WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entity_name_surfaces WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Graph fixture',2026) ON CONFLICT DO NOTHING").bind(SPORT).execute(&pool).await?;
    for (id, name) in [(1041, "Cedar Club"), (1042, "Birch Club")] {
        sqlx::query("INSERT INTO teams(id,sport,name) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(SPORT)
            .bind(name)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO entity_name_surfaces(sport,entity_type,entity_id,norm,surface_kind) VALUES($1,'team',$2,public.nrm($3),'name')").bind(SPORT).bind(id).bind(name).execute(&pool).await?;
    }
    let body = format!("Cedar Club reported a possible move. {}\nRiley Example denied it. Cedar Club 2–1 Birch Club \nLater correction: no move.","Publisher detail. ".repeat(90));
    sqlx::query("INSERT INTO news_articles(id,url,title,source,full_text,published_at) VALUES($1,'https://example.invalid/graph-native','Exact publisher headline','Fixture Wire',$2,now()-interval '1 day')").bind(ARTICLE).bind(&body).execute(&pool).await?;
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',1041,$2)").bind(ARTICLE).bind(SPORT).execute(&pool).await?;
    sqlx::query("SELECT classifier_enqueue_acquisition($1,$2)")
        .bind(ARTICLE)
        .bind(SPORT)
        .execute(&pool)
        .await?;
    // Other suites may leave acquisition claims; claim this fixture explicitly through the real queue.
    let items = work::claim(&pool, classifier::manifest::ACQUIRE_TASK, 100).await?;
    let item = items
        .into_iter()
        .find(|i| i.sport == SPORT)
        .context("Graph acquisition claim")?;
    classifier::adapter::AcquireHandler::new(pool.clone(), Arc::new(WebBroker::new(0)?))
        .execute(&item)
        .await?;
    let (article, candidates) = load_graph_article_context(&pool, ARTICLE, SPORT)
        .await?
        .unwrap();
    assert_eq!(article.description, body);
    assert_eq!(candidates.len(), 2);
    let source = load_source(&mut *pool.acquire().await?, ARTICLE, SPORT)
        .await?
        .unwrap();
    let snapshot_body = source.body.clone();
    let raw = r#"{"persons":[{"name":"Riley Example"},{"name":"Invented Person"}],"final_result_line":"Cedar Club 2–1 Birch Club"}"#;
    let extracted = Extracted {
        value: GraphParser {
            candidates: &candidates,
        }
        .parse(raw)?,
        raw_response: raw.into(),
        model: "fictional-graph".into(),
        built_prompt: "fixture".into(),
        request_body: json!({}),
        eval_count: 1,
        wall_ms: 0,
    };
    let prepared = Prepared::Read {
        input_hash: hash_components(&build_graph_input_components(&article, &candidates)),
        extracted: Box::new(extracted),
    };
    let claim = dbtest::claim_one(
        &pool,
        crate::plugins::graph::manifest::TASK,
        "Graph native source",
    )
    .await;
    assert_eq!(
        claim.input_version.as_deref().unwrap().split(':').next(),
        Some("classifier-source")
    );
    sqlx::query("UPDATE news_articles SET title='changed during model call' WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await?;
    assert!(commit_claimed(&pool, &claim, &prepared).await.is_err());
    sqlx::query("UPDATE news_articles SET title='Exact publisher headline' WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await?;
    sqlx::raw_sql("ALTER TABLE graph_extractions ADD CONSTRAINT reject_graph_fixture CHECK(sport<>'ZZ_CLASSIFIER_GRAPH')").execute(&pool).await?;
    assert!(commit_claimed(&pool, &claim, &prepared).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM entity_candidates WHERE sport=$1")
        .bind(SPORT)
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 0, "nomination must roll back with publication");
    sqlx::raw_sql("ALTER TABLE graph_extractions DROP CONSTRAINT reject_graph_fixture")
        .execute(&pool)
        .await?;
    assert_eq!(
        commit_claimed(&pool, &claim, &prepared).await?,
        PluginOutcome::Committed
    );
    let nominations: Vec<(String, i32)> =
        sqlx::query_as("SELECT norm_name,mention_count FROM entity_candidates WHERE sport=$1")
            .bind(SPORT)
            .fetch_all(&pool)
            .await?;
    assert_eq!(nominations, vec![("riley example".into(), 1)]);
    let status: String =
        sqlx::query_scalar("SELECT status FROM harvester_fixture_reviews WHERE article_id=$1")
            .bind(ARTICLE)
            .fetch_one(&pool)
            .await?;
    assert_eq!(status, "extraction_unavailable");
    assert!(
        read_is_current(
            &pool,
            ARTICLE,
            &hash_components(&build_graph_input_components(&article, &candidates))
        )
        .await?
    );
    assert_eq!(
        commit_claimed(&pool, &claim, &prepared).await?,
        PluginOutcome::Superseded
    );
    let mut changed = article.clone();
    changed.published = "changed".into();
    assert_ne!(
        build_graph_input_components(&article, &candidates),
        build_graph_input_components(&changed, &candidates)
    );
    sqlx::query("UPDATE classifier_sources SET body_sha256='corrupt' WHERE article_id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await?;
    assert!(load_graph_article_context(&pool, ARTICLE, SPORT)
        .await
        .is_err());
    sqlx::query("UPDATE classifier_sources SET body_sha256=$2 WHERE article_id=$1")
        .bind(ARTICLE)
        .bind(hex::encode(Sha256::digest(snapshot_body.as_bytes())))
        .execute(&pool)
        .await?;
    Ok(())
}

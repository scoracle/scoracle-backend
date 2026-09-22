use super::*;
use crate::application::queue::work::Stage;
use crate::studio::{graph::GraphParser, Parser};
use serde_json::json;
const SPORT: &str = "ZZ_GRAPH_STUDIO";
const ARTICLE: i64 = 9_700_001;
const TEAM: i32 = 9_700_002;
async fn setup() -> PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("TEST_DATABASE_URL").expect("isolated test database"))
        .await
        .unwrap();
    for table in [
        "pipeline_work",
        "graph_extractions",
        "narrative_events",
        "narrative_person_mentions",
        "narrative_persons",
        "cognition_ledger",
        "news_article_entities",
        "teams",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(SPORT)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM news_articles WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Graph tests',2026) ON CONFLICT DO NOTHING").bind(SPORT).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES($1,$2,'Graph Club')")
        .bind(TEAM)
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO news_articles(id,url_hash,url,title,source,description) VALUES($1,'studio-graph-test','https://example.test/graph','Graph story','Graph Wire','Riley praises Graph Club')").bind(ARTICLE).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO news_article_entities(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)").bind(ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await.unwrap();
    pool
}
async fn claim(pool: &PgPool, revision: &str) -> Item {
    let pending = Item {
        stage: Stage::Graph,
        entity_type: "article".into(),
        entity_id: ARTICLE,
        sport: SPORT.into(),
        input_version: Some(revision.into()),
        attempts: 0,
        claim_token: None,
    };
    work::enqueue(pool, &pending).await.unwrap();
    work::claim(pool, Stage::Graph, 1).await.unwrap().remove(0)
}
fn prepared(raw: &str) -> Prepared {
    let candidates = [GraphCandidate {
        entity_type: "team".into(),
        entity_id: TEAM,
        descriptor: "Graph Club (team)".into(),
    }];
    Prepared::Read {
        input_hash: "test-material".into(),
        extracted: Box::new(Extracted {
            value: GraphParser {
                candidates: &candidates,
            }
            .parse(raw)
            .unwrap(),
            raw_response: raw.into(),
            model: "actual-graph-model".into(),
            built_prompt: "test prompt".into(),
            request_body: json!({}),
            eval_count: 12,
            wall_ms: 5,
        }),
    }
}
fn product() -> Prepared {
    prepared(
        r#"{"relations":[{"subject":1,"predicate":"praise","object":null,"sentiment":0.4,"confidence":"reported"}],"persons":[{"name":"Riley Example","kind":"coach","team_context":1}]}"#,
    )
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE sport=$1"))
        .bind(SPORT)
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn graph_commits_relations_person_evidence_marker_and_completion_once() {
    let pool = setup().await;
    let item = claim(&pool, "v1").await;
    assert_eq!(
        commit_claimed(&pool, &item, &product()).await.unwrap(),
        PluginOutcome::Committed
    );
    assert_eq!(count(&pool, "narrative_events").await, 1);
    assert_eq!(count(&pool, "narrative_person_mentions").await, 1);
    let evidence: (i32, i32, String) = sqlx::query_as(
        "SELECT mention_count,distinct_sources,status FROM narrative_persons WHERE sport=$1",
    )
    .bind(SPORT)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(evidence, (1, 1, "candidate".into()));
    let marker:(String,String,String,i32,i32)=sqlx::query_as("SELECT model_version,prompt_version,parser_outcome,relations_n,persons_n FROM graph_extractions WHERE article_id=$1").bind(ARTICLE).fetch_one(&pool).await.unwrap();
    assert_eq!(
        marker,
        (
            "actual-graph-model".into(),
            GRAPH_PROMPT_VERSION.into(),
            "extracted".into(),
            1,
            1
        )
    );
    assert_eq!(count(&pool, "pipeline_work").await, 0);
    assert_eq!(
        commit_claimed(&pool, &item, &product()).await.unwrap(),
        PluginOutcome::Superseded
    );
    let second = claim(&pool, "v2").await;
    commit_claimed(&pool, &second, &product()).await.unwrap();
    let mentions: i32 =
        sqlx::query_scalar("SELECT mention_count FROM narrative_persons WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        mentions, 1,
        "re-extraction must not manufacture corroboration"
    );
    assert_eq!(count(&pool, "narrative_events").await, 1);
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn graph_revision_and_reclaim_fence_every_effect() {
    let pool = setup().await;
    let stale = claim(&pool, "v1").await;
    let old = claim(&pool, "v2").await;
    work::release(&pool, &old).await.unwrap();
    let current = work::claim(&pool, Stage::Graph, 1).await.unwrap().remove(0);
    for item in [&stale, &old] {
        assert_eq!(
            commit_claimed(&pool, item, &product()).await.unwrap(),
            PluginOutcome::Superseded
        );
        for table in [
            "narrative_events",
            "narrative_persons",
            "narrative_person_mentions",
            "graph_extractions",
            "cognition_ledger",
        ] {
            assert_eq!(count(&pool, table).await, 0, "{table}");
        }
    }
    assert_eq!(
        commit_claimed(&pool, &current, &product()).await.unwrap(),
        PluginOutcome::Committed
    );
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn graph_marker_failure_rolls_back_all_products_and_preserves_retry() {
    let pool = setup().await;
    let item = claim(&pool, "v1").await;
    let mut bad = product();
    if let Prepared::Read { input_hash, .. } = &mut bad {
        *input_hash = "bad\0hash".into();
    }
    assert!(commit_claimed(&pool, &item, &bad).await.is_err());
    for table in [
        "narrative_events",
        "narrative_persons",
        "narrative_person_mentions",
        "graph_extractions",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
    assert_eq!(count(&pool, "pipeline_work").await, 1);
    assert_eq!(
        commit_claimed(&pool, &item, &product()).await.unwrap(),
        PluginOutcome::Committed
    );
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn graph_fail_closed_empty_and_unchanged_have_distinct_receipts() {
    let pool = setup().await;
    let item = claim(&pool, "v1").await;
    commit_claimed(&pool, &item, &prepared("not JSON"))
        .await
        .unwrap();
    let outcome: String =
        sqlx::query_scalar("SELECT parser_outcome FROM graph_extractions WHERE article_id=$1")
            .bind(ARTICLE)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(outcome, "failed_closed");
    assert!(read_is_current(&pool, ARTICLE, "test-material")
        .await
        .unwrap());
    assert!(!read_is_current(&pool, ARTICLE, "new-material")
        .await
        .unwrap());
    sqlx::query("UPDATE graph_extractions SET prompt_version='old' WHERE article_id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!read_is_current(&pool, ARTICLE, "test-material")
        .await
        .unwrap());

    assert_eq!(count(&pool, "narrative_events").await, 0);
    let unchanged = claim(&pool, "v2").await;
    let before = count(&pool, "cognition_ledger").await;
    commit_claimed(&pool, &unchanged, &Prepared::Unchanged)
        .await
        .unwrap();
    assert_eq!(count(&pool, "cognition_ledger").await, before);
    let empty = claim(&pool, "v3").await;
    commit_claimed(&pool, &empty, &prepared(r#"{"relations":[],"persons":[]}"#))
        .await
        .unwrap();
    let outcome: String =
        sqlx::query_scalar("SELECT parser_outcome FROM graph_extractions WHERE article_id=$1")
            .bind(ARTICLE)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(outcome, "extracted");
    assert_eq!(count(&pool, "pipeline_work").await, 0);
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn graph_loader_uses_editor_material_and_ignores_duplicates_and_unlinked_articles() {
    let pool = setup().await;
    let (article, candidates) = load_graph_article_context(&pool, ARTICLE, SPORT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(article.description, "Riley praises Graph Club");
    assert_eq!(candidates.len(), 1);
    sqlx::query("INSERT INTO editor_reads(article_id,status,contract_version,read) VALUES($1,'success','ep8',$2)").bind(ARTICLE).bind(json!({"evidence_blurb":"Editor body summary"})).execute(&pool).await.unwrap();
    let (article, _) = load_graph_article_context(&pool, ARTICLE, SPORT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(article.description, "Editor body summary");
    sqlx::query("UPDATE news_articles SET duplicate_of=id WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
    assert!(load_graph_article_context(&pool, ARTICLE, SPORT)
        .await
        .unwrap()
        .is_none());
    sqlx::query("UPDATE news_articles SET duplicate_of=NULL WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("DELETE FROM news_article_entities WHERE article_id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
    assert!(load_graph_article_context(&pool, ARTICLE, SPORT)
        .await
        .unwrap()
        .is_none());
    assert!(load_graph_article_context(&pool, -999, SPORT)
        .await
        .unwrap()
        .is_none());
}

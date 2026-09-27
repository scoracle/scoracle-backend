use super::*;
use crate::application::queue::work;
use crate::plugins::graph::cognition::GraphParser;
use crate::studio::Parser;
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
        "harvester_fixture_reviews",
        "entity_candidates",
        "news_article_entities",
        "fixtures",
        "entity_name_surfaces",
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
        stage: crate::plugins::graph::manifest::TASK,
        entity_type: "article".into(),
        entity_id: ARTICLE,
        sport: SPORT.into(),
        input_version: Some(revision.into()),
        attempts: 0,
        claim_token: None,
    };
    work::enqueue(pool, &pending).await.unwrap();
    work::claim(pool, crate::plugins::graph::manifest::TASK, 1)
        .await
        .unwrap()
        .remove(0)
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
#[ignore = "requires isolated TEST_DATABASE_URL with migration 269"]
async fn graph_prefers_exact_harvester_context_over_editor_blurb() {
    let pool = setup().await;
    let body = "Exact quote";
    sqlx::query(
        "UPDATE news_articles SET full_text=$2,title='Exact publisher headline' WHERE id=$1",
    )
    .bind(ARTICLE)
    .bind(body)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)")
        .bind(ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO harvester_classifications \
         (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
          body_sha256,headline,model_input_start,model_input_end,model_input_text, \
          context_start,context_end,context_text,distributions,model_provenance) \
         VALUES($1,'team',$2,$3,'harvest-context-v1','test','irrelevant', \
                $4,'Exact publisher headline',0,11,'Exact quote',0,11,'Exact quote','{}'::jsonb,'{}'::jsonb)",
    )
    .bind(ARTICLE).bind(TEAM).bind(SPORT)
    .bind(hex::encode(Sha256::digest(body.as_bytes())))
    .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO harvester_entity_mentions(article_id,entity_type,entity_id,sport,matched_norm,body_sha256) VALUES($1,'team',$2,$3,'graph club','hash')")
        .bind(ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM news_article_entities WHERE article_id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
    let (article, candidates) = load_graph_article_context(&pool, ARTICLE, SPORT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(article.title, "Exact publisher headline");
    assert_eq!(article.description, "Exact quote");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].entity_id, TEAM);
    sqlx::query("UPDATE news_articles SET full_text='drift' WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
    assert!(load_graph_article_context(&pool, ARTICLE, SPORT)
        .await
        .is_err());
    sqlx::query("DELETE FROM news_articles WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
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
#[ignore = "requires isolated TEST_DATABASE_URL with Harvester migrations; run serially"]
async fn graph_nominates_only_source_anchored_unknown_people_for_investigator() {
    let pool = setup().await;
    let body = "Graph Club introduced Riley Example as its new coach. The club expects him to lead training tomorrow.";
    sqlx::query(
        "UPDATE news_articles SET full_text=$2,title='Graph Club announcement' WHERE id=$1",
    )
    .bind(ARTICLE)
    .bind(body)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)")
        .bind(ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO harvester_classifications \
         (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
          body_sha256,headline,model_input_start,model_input_end,model_input_text, \
          context_start,context_end,context_text,distributions,model_provenance) \
         VALUES($1,'team',$2,$3,'harvest-context-v1','graph-nomination','relevant', \
                $4,'Graph Club announcement',0,$5,$6,0,$5,$6,'{}'::jsonb,'{}'::jsonb)",
    )
    .bind(ARTICLE)
    .bind(TEAM)
    .bind(SPORT)
    .bind(hex::encode(Sha256::digest(body.as_bytes())))
    .bind(body.len() as i32)
    .bind(body)
    .execute(&pool)
    .await
    .unwrap();
    let item = claim(&pool, "source-v1").await;
    assert_eq!(
        commit_claimed(&pool, &item, &product()).await.unwrap(),
        PluginOutcome::Committed
    );
    let candidate: (i32, String, Option<String>) = sqlx::query_as(
        "SELECT c.mention_count,c.state,m.quote FROM entity_candidates c \
         JOIN candidate_mentions m ON m.candidate_id=c.id \
         WHERE c.sport=$1 AND m.article_id=$2",
    )
    .bind(SPORT)
    .bind(ARTICLE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(candidate.0, 1);
    assert_eq!(candidate.1, "pending");
    assert!(candidate.2.unwrap().contains("Riley Example"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='investigate_entity'"
        )
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    // Replaying the same article under a new claim cannot invent corroboration.
    let repeat = claim(&pool, "source-v2").await;
    commit_claimed(&pool, &repeat, &product()).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT mention_count FROM entity_candidates WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    let hallucinated = GraphPerson {
        name: "Invented Person".into(),
        kind: "coach".into(),
        team_context_type: Some("team".into()),
        team_context_id: Some(TEAM),
    };
    let mut tx = pool.begin().await.unwrap();
    nominate_harvester_person(&mut tx, ARTICLE, SPORT, &hallucinated)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM entity_candidates WHERE sport=$1")
            .bind(SPORT)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    sqlx::query("UPDATE news_articles SET full_text='publisher body drifted' WHERE id=$1")
        .bind(ARTICLE)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        nominate_harvester_person(&mut tx, ARTICLE, SPORT, &hallucinated)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL with Harvester migrations; run serially"]
async fn graph_nominates_verbatim_final_result_and_records_refused_quotes() {
    let pool = setup().await;
    const AWAY: i32 = TEAM + 1;
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES($1,$2,'Away Club')")
        .bind(AWAY)
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
    for (id, name) in [(TEAM, "Graph Club"), (AWAY, "Away Club")] {
        sqlx::query("INSERT INTO entity_name_surfaces(entity_type,entity_id,sport,norm,surface_kind) VALUES('team',$1,$2,public.nrm($3),'name')")
            .bind(id).bind(SPORT).bind(name).execute(&pool).await.unwrap();
    }
    let body = "Graph Club 2-1 Away Club. The final whistle sounded after extra time.";
    sqlx::query("UPDATE news_articles SET full_text=$2,title='Match report' WHERE id=$1")
        .bind(ARTICLE)
        .bind(body)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport) VALUES($1,'team',$2,$3)")
        .bind(ARTICLE).bind(TEAM).bind(SPORT).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO harvester_classifications \
         (article_id,entity_type,entity_id,sport,contract_version,model_revision,entity_choice, \
          body_sha256,headline,model_input_start,model_input_end,model_input_text, \
          context_start,context_end,context_text,distributions,model_provenance) \
         VALUES($1,'team',$2,$3,'harvest-context-v1','fixture-review','relevant', \
                $4,'Match report',0,$5,$6,0,$5,$6,'{}'::jsonb,'{}'::jsonb)",
    )
    .bind(ARTICLE)
    .bind(TEAM)
    .bind(SPORT)
    .bind(hex::encode(Sha256::digest(body.as_bytes())))
    .bind(body.len() as i32)
    .bind(body)
    .execute(&pool)
    .await
    .unwrap();
    let claimed = claim(&pool, "fixture-v1").await;
    let output =
        prepared(r#"{"relations":[],"persons":[],"final_result_line":"Graph Club 2-1 Away Club"}"#);
    assert_eq!(
        commit_claimed(&pool, &claimed, &output).await.unwrap(),
        PluginOutcome::Committed
    );
    let review: (String, Option<String>, Option<i32>) = sqlx::query_as(
        "SELECT status,source_quote,fixture_id FROM harvester_fixture_reviews WHERE article_id=$1",
    )
    .bind(ARTICLE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(review.0, "created");
    assert_eq!(review.1.as_deref(), Some("Graph Club 2-1 Away Club"));
    assert!(review.2.is_some());
    let fixture: (i32, i32, i32, i32, bool, String) = sqlx::query_as(
        "SELECT home_team_id,away_team_id,home_score,away_score, \
                (meta->>'needs_verification')::bool,meta->>'nominated_by' \
         FROM fixtures WHERE id=$1",
    )
    .bind(review.2)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fixture, (TEAM, AWAY, 2, 1, true, "harvester_graph".into()));
    assert_eq!(
        commit_claimed(&pool, &claimed, &output).await.unwrap(),
        PluginOutcome::Superseded
    );
    assert_eq!(count(&pool, "fixtures").await, 1);
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        fixture::review(
            &mut tx,
            ARTICLE,
            SPORT,
            "changed-source-material",
            "test-graph",
            "Graph Club 9-0 Away Club"
        )
        .await
        .unwrap(),
        Some("quote_not_found")
    );
    tx.commit().await.unwrap();
    assert_eq!(count(&pool, "fixtures").await, 1);
    assert_eq!(count(&pool, "harvester_fixture_reviews").await, 2);
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn graph_revision_and_reclaim_fence_every_effect() {
    let pool = setup().await;
    let stale = claim(&pool, "v1").await;
    let old = claim(&pool, "v2").await;
    work::release(&pool, &old).await.unwrap();
    let current = work::claim(&pool, crate::plugins::graph::manifest::TASK, 1)
        .await
        .unwrap()
        .remove(0);
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

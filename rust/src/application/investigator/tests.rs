use super::*;
use crate::application::queue::work::{self, Item, Stage};
use crate::studio::investigator::{gate::RoleClass, WikidataItem};
use crate::studio::plugin::PluginOutcome;
use serde_json::json;
use sqlx::PgPool;
const SPORT: &str = "ZZ_INVESTIGATOR";
const ID: i32 = 9_600_001;
const DOC: i64 = 9_600_002;
const VENUE_DOC: i64 = 9_600_003;
async fn setup() -> PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("TEST_DATABASE_URL").expect("isolated test database"))
        .await
        .unwrap();
    sqlx::query("DELETE FROM entity_relationships WHERE subject_sport=$1 OR object_sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
    for table in [
        "pipeline_work",
        "data_fetch_ledger",
        "entity_candidates",
        "entity_facts",
        "entity_aliases",
        "entity_external_ids",
        "persons",
        "players",
        "fixtures",
        "teams",
        "entity_name_surfaces",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
            .bind(SPORT)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO sports(id, display_name,current_season) VALUES ($1,'Investigator tests',2026) ON CONFLICT DO NOTHING").bind(SPORT).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES($1,$2,'Investigator Club')")
        .bind(ID)
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO source_documents(id,url,retained_excerpt) VALUES ($1,'https://example.test/investigator','Riley Example Investigator Club'),($2,'https://example.test/venue','Test Arena') ON CONFLICT DO NOTHING").bind(DOC).bind(VENUE_DOC).execute(&pool).await.unwrap();
    pool
}
async fn claim(pool: &PgPool, kind: &str, revision: &str) -> Item {
    let item = Item {
        stage: Stage::InvestigateEntity,
        entity_type: kind.into(),
        entity_id: ID.into(),
        sport: SPORT.into(),
        input_version: Some(revision.into()),
        attempts: 0,
        claim_token: None,
    };
    work::enqueue(pool, &item).await.unwrap();
    work::claim(pool, Stage::InvestigateEntity, 1)
        .await
        .unwrap()
        .remove(0)
}
async fn candidate(pool: &PgPool) -> CandidateRow {
    sqlx::query("INSERT INTO entity_candidates(id,idempotency_key,norm_name,sport) VALUES($1,'studio-investigator-test','riley example',$2)").bind(i64::from(ID)).bind(SPORT).execute(pool).await.unwrap();
    load_candidate(pool, ID.into()).await.unwrap().unwrap()
}
fn accepted(c: CandidateRow) -> Decision {
    Decision::Accept {
        candidate: c,
        sport: SPORT.into(),
        item: Box::new(WikidataItem {
            qid: "QTEST".into(),
            coach_of_teams: vec!["QCLUB".into()],
            label: "Riley Example".into(),
            aliases: vec!["R. Example".into()],
            source_document_id: DOC,
            ..Default::default()
        }),
        kind: "coach".into(),
        role: RoleClass::Coach,
        teams: vec![ID],
        plan: json!({"arm":"test"}),
    }
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
async fn candidate_publication_is_atomic_and_stale_claim_has_no_effects() {
    let pool = setup().await;
    let c = candidate(&pool).await;
    let old = claim(&pool, "candidate", "v1").await;
    let current = claim(&pool, "candidate", "v2").await;
    let decision = accepted(c);
    let mappings = [TeamMapping {
        team_id: ID,
        sport: SPORT.into(),
        qid: "QCLUB".into(),
        document_id: DOC,
    }];
    assert_eq!(
        commit_claimed(&pool, &old, &mappings, &decision)
            .await
            .unwrap(),
        PluginOutcome::Superseded
    );
    assert_eq!(count(&pool, "persons").await, 0);
    assert_eq!(count(&pool, "entity_external_ids").await, 0);
    assert_eq!(
        commit_claimed(&pool, &current, &mappings, &decision)
            .await
            .unwrap(),
        PluginOutcome::Committed
    );
    assert_eq!(count(&pool, "persons").await, 1);
    let team: i32 = sqlx::query_scalar("SELECT team_id FROM persons WHERE sport=$1")
        .bind(SPORT)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        team, ID,
        "new team mapping participates in this same publication"
    );

    assert_eq!(count(&pool, "entity_external_ids").await, 2);
    assert_eq!(
        load_candidate(&pool, ID.into())
            .await
            .unwrap()
            .unwrap()
            .state,
        "accepted"
    );
    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM acquisition_runs WHERE candidate_id=$1")
            .bind(i64::from(ID))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(runs, 1);
    assert_eq!(
        commit_claimed(&pool, &current, &mappings, &decision)
            .await
            .unwrap(),
        PluginOutcome::Superseded
    );
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn failed_source_write_rolls_back_person_mapping_audit_and_completion() {
    let pool = setup().await;
    let c = candidate(&pool).await;
    let item = claim(&pool, "candidate", "v1").await;
    let mut decision = accepted(c);
    if let Decision::Accept { item, .. } = &mut decision {
        item.source_document_id = -999;
    }
    let mappings = [TeamMapping {
        team_id: ID,
        sport: SPORT.into(),
        qid: "QCLUB".into(),
        document_id: DOC,
    }];
    assert!(commit_claimed(&pool, &item, &mappings, &decision)
        .await
        .is_err());
    assert_eq!(count(&pool, "persons").await, 0);
    assert_eq!(count(&pool, "entity_external_ids").await, 0);
    assert_eq!(
        load_candidate(&pool, ID.into())
            .await
            .unwrap()
            .unwrap()
            .state,
        "pending"
    );
    let running:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pipeline_work WHERE sport=$1 AND claim_token=$2::uuid AND status='running')").bind(SPORT).bind(&item.claim_token).fetch_one(&pool).await.unwrap();
    assert!(running);
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn refusal_records_audit_and_completes_without_creating_identity() {
    let pool = setup().await;
    let c = candidate(&pool).await;
    let item = claim(&pool, "candidate", "v1").await;
    let decision = Decision::Refuse {
        candidate: c,
        state: "ambiguous".into(),
        resolved: None,
        plan: json!({"arm":"prose","model":"test-model","contract":"ip1"}),
        reason: "two namesakes".into(),
    };
    assert_eq!(
        commit_claimed(&pool, &item, &[], &decision).await.unwrap(),
        PluginOutcome::Committed
    );
    assert_eq!(count(&pool, "persons").await, 0);
    let model: String =
        sqlx::query_scalar("SELECT model_version FROM acquisition_runs WHERE candidate_id=$1")
            .bind(i64::from(ID))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(model, "test-model");
    assert_eq!(
        load_candidate(&pool, ID.into())
            .await
            .unwrap()
            .unwrap()
            .state,
        "ambiguous"
    );
}
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn team_facts_keep_distinct_sources_and_stale_attempt_does_not_stamp() {
    let pool = setup().await;
    let old = claim(&pool, "team", "v1").await;
    let current = claim(&pool, "team", "v2").await;
    let decision = Decision::Team {
        team_id: ID,
        sport: SPORT.into(),
        venue: Some(("Test Arena".into(), VENUE_DOC)),
        logo: Some(("https://example.test/logo".into(), DOC)),
    };
    assert_eq!(
        commit_claimed(&pool, &old, &[], &decision).await.unwrap(),
        PluginOutcome::Superseded
    );
    let stamped: bool =
        sqlx::query_scalar("SELECT meta ? 'investigated_at' FROM teams WHERE id=$1")
            .bind(ID)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!stamped);
    assert_eq!(
        commit_claimed(&pool, &current, &[], &decision)
            .await
            .unwrap(),
        PluginOutcome::Committed
    );
    let facts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT fact_type,source_document_id FROM entity_facts WHERE sport=$1 ORDER BY fact_type",
    )
    .bind(SPORT)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        facts,
        vec![("logo_url".into(), DOC), ("venue_name".into(), VENUE_DOC)]
    );
}

#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn player_reclaim_fences_facts_ids_and_cooldown() {
    let pool = setup().await;
    sqlx::query("INSERT INTO players(id,sport,name) VALUES($1,$2,'Riley Example')")
        .bind(ID)
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
    let seeded = sqlx::query("INSERT INTO entity_fact_policy(entity_type,fact_type,tier) VALUES('player','date_of_birth','evidenced') ON CONFLICT DO NOTHING").execute(&pool).await.unwrap().rows_affected() == 1;
    let old = claim(&pool, "player", "v1").await;
    work::release(&pool, &old).await.unwrap();
    let current = work::claim(&pool, Stage::InvestigateEntity, 1)
        .await
        .unwrap()
        .remove(0);
    let decision = Decision::Player {
        player_id: ID,
        sport: SPORT.into(),
        it: Box::new(WikidataItem {
            qid: "QPLAYER".into(),
            label: "Riley Example".into(),
            date_of_birth: Some("+1990-01-01T00:00:00Z".into()),
            source_document_id: DOC,
            ..Default::default()
        }),
    };
    assert_eq!(
        commit_claimed(&pool, &old, &[], &decision).await.unwrap(),
        PluginOutcome::Superseded
    );
    assert_eq!(count(&pool, "entity_facts").await, 0);
    let stamped: bool =
        sqlx::query_scalar("SELECT meta ? 'investigated_at' FROM players WHERE id=$1")
            .bind(ID)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!stamped);
    assert_eq!(
        commit_claimed(&pool, &current, &[], &decision)
            .await
            .unwrap(),
        PluginOutcome::Committed
    );
    let stamp: bool =
        sqlx::query_scalar("SELECT meta ? 'investigated_at' FROM players WHERE id=$1")
            .bind(ID)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stamp);
    assert_eq!(count(&pool, "entity_external_ids").await, 1);
    let dob: String = sqlx::query_scalar("SELECT date_of_birth::text FROM players WHERE id=$1")
        .bind(ID)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dob, "1990-01-01");
    if seeded {
        sqlx::query("DELETE FROM entity_fact_policy WHERE entity_type='player' AND fact_type='date_of_birth'").execute(&pool).await.unwrap();
    }
}

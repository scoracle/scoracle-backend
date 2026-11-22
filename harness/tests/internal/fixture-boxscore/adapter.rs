use super::*;
use crate::harness::queue::work;
#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL; run serially"]
async fn terminal_acquisition_and_ledger_commit_only_with_exact_claim() {
    let pool = sqlx::PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    let sport = "ZZ_INVESTIGATOR_BOX";
    let id = 9_610_001;
    for sql in [
        "DELETE FROM pipeline_work WHERE sport=$1",
        "DELETE FROM data_fetch_ledger WHERE sport=$1",
        "DELETE FROM fixtures WHERE sport=$1",
        "DELETE FROM teams WHERE sport=$1",
    ] {
        sqlx::query(sql).bind(sport).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES($1,'Boxscore tests',2026) ON CONFLICT DO NOTHING").bind(sport).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO teams(id,sport,name) VALUES($1,$3,'Home'),($2,$3,'Away')")
        .bind(id)
        .bind(id + 1)
        .bind(sport)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO fixtures(id,sport,season,home_team_id,away_team_id,start_time) VALUES($1,$2,2026,$1,$3,NOW())").bind(id).bind(sport).bind(id+1).execute(&pool).await.unwrap();
    let fixture = load_fixture(&pool, id).await.unwrap().unwrap();
    let pending = Item {
        stage: crate::harness::skills::fixture_boxscore::manifest::TASK,
        entity_type: "fixture".into(),
        entity_id: id.into(),
        sport: sport.into(),
        input_version: Some("v1".into()),
        attempts: 0,
        claim_token: None,
    };
    work::enqueue(&pool, &pending).await.unwrap();
    let old = work::claim(
        &pool,
        crate::harness::skills::fixture_boxscore::manifest::TASK,
        1,
    )
    .await
    .unwrap()
    .remove(0);
    work::release(&pool, &old).await.unwrap();
    let current = work::claim(
        &pool,
        crate::harness::skills::fixture_boxscore::manifest::TASK,
        1,
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(
        persist_record(
            &pool,
            &old,
            &fixture,
            PersistRecord::terminal("none", "not_supported", None)
        )
        .await
        .unwrap(),
        SkillOutcome::Superseded
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fixture_boxscore_fetches WHERE fixture_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        persist_record(
            &pool,
            &current,
            &fixture,
            PersistRecord::terminal("none", "not_supported", None)
        )
        .await
        .unwrap(),
        SkillOutcome::Committed
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM data_fetch_ledger WHERE sport=$1")
        .bind(sport)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

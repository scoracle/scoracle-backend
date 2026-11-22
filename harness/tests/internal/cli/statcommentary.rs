#[tokio::test]
#[ignore = "requires isolated TEST_DATABASE_URL with Harvester migrations"]
async fn harvester_player_selection_executes_on_migrated_schema() {
    let pool = PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    sqlx::query(&enum_current_season_sql(true))
        .bind("NBA")
        .bind(2026)
        .bind(RATING_PROMPT_VERSION)
        .fetch_all(&pool)
        .await
        .unwrap();
}

use super::*;

const SPORT: &str = "ZZ_LEDGER_PLUGIN";

#[tokio::test]
#[ignore = "requires isolated migrated TEST_DATABASE_URL"]
async fn generation_diagnostics_retain_the_owning_plugin_identity() {
    let pool = crate::harness::dbtest::pool_with_limit(1).await.unwrap();
    sqlx::query("DELETE FROM cognition_ledger WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
    let generation = Generation::uncalled(
        (),
        "test-model".to_string(),
        "test-prompt-v1",
        vec![41],
        Some("test-input-hash".to_string()),
    );
    let spec = LedgerSpec {
        plugin_id: "scoracle.test.ledger",
        stage: "rating",
        lens: "test-ledger",
        role: crate::harness::skills::scout::manifest::ROUTE,
        product_table: "stat_summaries",
        output_contract_version: "test-output-v1",
    };
    insert_generation_ledger_best_effort(
        &pool,
        &generation,
        spec,
        LedgerEvent {
            entity_type: "team",
            entity_id: 9_100_001,
            sport: SPORT,
            pair_entity: None,
            trigger_type: "test",
            trigger_payload: serde_json::Value::Null,
            product_row_ids: Vec::new(),
            included_evidence: serde_json::json!([]),
            excluded_evidence: serde_json::json!([]),
            context_budget: serde_json::json!({}),
            parser_outcome: "uncalled",
        },
    )
    .await;
    let plugin_id: String = sqlx::query_scalar(
        "SELECT plugin_id FROM cognition_ledger WHERE sport=$1 AND lens='test-ledger'",
    )
    .bind(SPORT)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(plugin_id, spec.plugin_id);
    sqlx::query("DELETE FROM cognition_ledger WHERE sport=$1")
        .bind(SPORT)
        .execute(&pool)
        .await
        .unwrap();
}

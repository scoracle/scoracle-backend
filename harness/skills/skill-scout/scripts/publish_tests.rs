//! Unit and exact-publication tests for the Scout application adapter.

use super::super::performance::RatingExclusions;
use super::super::request::RATING_PROMPT_VERSION;
use super::super::RatingProduct;
use super::*;
use crate::harness::Generation;
fn rating_product(
    skipped_no_stats: bool,
    skipped_unchanged: bool,
    body: Option<&str>,
) -> RatingOutput {
    Generation::uncalled(
        RatingProduct {
            availability: Default::default(),
            season: 2026,
            skipped_no_stats,
            abstained: false,
            skipped_unchanged,
            body: body.map(str::to_string),
            headline: body.map(|_| "Test Team owns the middle".to_string()),
            card_score: body.map(|_| 67.7),
            notability: body.map(|_| 74),
            notability_components: serde_json::json!({"spread": 74}),
            rating_trajectory: body.map(|_| "rising".to_string()),
            rating_trajectory_label: body.map(|_| "Gaining ground".to_string()),
            rating_trajectory_components: serde_json::json!({"delta": 3.2}),
            input_components: if body.is_some() {
                r#"{"prompt_version":"rating-commentary-v21","rating_id":81}"#.to_string()
            } else {
                "{}".to_string()
            },
            exclusions: RatingExclusions::default(),
        },
        "test-rating-model".to_string(),
        RATING_PROMPT_VERSION,
        Vec::new(),
        body.map(|_| "rating-input-hash".to_string()),
    )
}

#[test]
fn an_abstained_card_publishes_a_marker_instead_of_being_debounced() {
    let mut output = rating_product(false, false, None);
    output.product.abstained = true;
    assert!(matches!(prepare(&output), Prepared::Product(_)));
}

/// Exact publication-contract acceptance against an isolated database containing migrations
/// 256-259. Ordinary test runs compile but ignore these cases; opt in with TEST_DATABASE_URL.
#[path = "../../../tests/internal/scout/publish_tests.rs"]
mod postgres_publication_fencing_tests;

//! Exact publication and five-pillar readiness tests for the Oracle application boundary.

use super::super::*;
use super::*;
use crate::harness::queue::work::TaskKey;
use crate::harness::skills::oracle::SigilSynthesis;
use crate::harness::Generation;

fn crown(score: Option<i32>) -> SigilOutput {
    Generation::uncalled(
        SigilSynthesis {
            score,
            reading: score.map(|_| "Test Team stand where the five signs meet.".to_string()),
            season: 2026,
            input_components_json: if score.is_some() {
                r#"{"narrative_titles":["Test"]}"#.to_string()
            } else {
                "{}".to_string()
            },
            headline: score.map(|_| "Test Team meet the signs".to_string()),
            convergence: score.map(|_| 67),
            omen: score.map(|_| "crossroads"),
        },
        "test-oracle-model".to_string(),
        crate::harness::skills::oracle::request::ORACLE_PROMPT_VERSION,
        Vec::new(),
        score.map(|_| "oracle-input-hash".to_string()),
    )
}

#[test]
fn readiness_waits_on_five_pillars_and_never_on_sigil_itself() {
    let names: Vec<&str> = PILLAR_STAGES.iter().map(|stage| stage.as_str()).collect();
    assert_eq!(
        names,
        ["narratives", "rating", "vibe", "momentum", "transfers"]
    );
    assert!(!PILLAR_STAGES.contains(&crate::harness::skills::oracle::manifest::TASK));
}

#[path = "../../../tests/internal/oracle/publish_tests.rs"]
mod postgres_oracle_tests;

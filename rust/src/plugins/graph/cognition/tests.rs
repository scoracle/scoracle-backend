//! Unit tests for the Studio Graph contract.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to Studio, so these run exactly as they did inline.

use super::*;

fn candidates() -> Vec<GraphCandidate> {
    vec![
        GraphCandidate {
            entity_type: "player".into(),
            entity_id: 10,
            descriptor: "Morgan Rogers, an English midfielder, currently at Aston Villa.".into(),
        },
        GraphCandidate {
            entity_type: "team".into(),
            entity_id: 18,
            descriptor: "Chelsea (team)".into(),
        },
    ]
}

#[test]
fn parser_fail_closed() {
    let c = candidates();
    let p = GraphParser { candidates: &c };
    assert!(p.parse("").unwrap().is_none());
    assert!(p.parse("I could not find relations.").unwrap().is_none());
    assert!(p.parse("{not json").unwrap().is_none());
}

#[test]
fn parser_valid_relation_resolves_numbers() {
    let c = candidates();
    let p = GraphParser { candidates: &c };
    let out = p
        .parse(
            r#"{"relations":[{"subject":1,"predicate":"trade_rumor","object":2,"sentiment":0.4,"confidence":"reported"}],"persons":[]}"#,
        )
        .unwrap()
        .unwrap();
    assert_eq!(out.relations.len(), 1);
    let r = &out.relations[0];
    assert_eq!((r.subject_type.as_str(), r.subject_id), ("player", 10));
    assert_eq!(
        (r.object_type.as_deref(), r.object_id),
        (Some("team"), Some(18))
    );
    assert_eq!(r.predicate, "trade_rumor");
    assert_eq!(r.confidence, "reported");
}

#[test]
fn parser_drops_invalid_entries_keeps_valid() {
    let c = candidates();
    let p = GraphParser { candidates: &c };
    let out = p
        .parse(
            r#"{"relations":[
                {"subject":9,"predicate":"injury","object":null,"sentiment":0,"confidence":"reported"},
                {"subject":1,"predicate":"levitation","object":2,"sentiment":0,"confidence":"reported"},
                {"subject":1,"predicate":"praise","object":7,"sentiment":0,"confidence":"reported"},
                {"subject":1,"predicate":"injury","object":null,"sentiment":-9,"confidence":"maybe"},
                {"subject":1,"predicate":"injury","object":null,"sentiment":-9,"confidence":"reported"}
            ],"persons":[]}"#,
        )
        .unwrap()
        .unwrap();
    // Only the last survives: bad subject, bad predicate, dangling object, bad
    // confidence are each dropped; sentiment -9 clamps to -1.
    assert_eq!(out.relations.len(), 1);
    assert_eq!(out.relations[0].sentiment, Some(-1.0));
}

#[test]
fn parser_person_discovery_rules() {
    let c = candidates();
    let p = GraphParser { candidates: &c };
    let out = p
        .parse(
            r#"{"relations":[],"persons":[
                {"name":"Enzo Maresca","kind":"coach","team_context":2},
                {"name":"Enzo Maresca","kind":"coach","team_context":2},
                {"name":"","kind":"agent","team_context":null},
                {"name":"Morgan Rogers","kind":"other","team_context":null},
                {"name":"Rafaela Pimenta","kind":"superagent","team_context":1}
            ]}"#,
        )
        .unwrap()
        .unwrap();
    // Duplicate, empty, and already-listed names drop; unknown kind maps to
    // "other"; a PLAYER team_context is dropped (kept person, no tie).
    assert_eq!(out.persons.len(), 2);
    assert_eq!(out.persons[0].name, "Enzo Maresca");
    assert_eq!(out.persons[0].kind, "coach");
    assert_eq!(out.persons[0].team_context_id, Some(18));
    assert_eq!(out.persons[1].name, "Rafaela Pimenta");
    assert_eq!(out.persons[1].kind, "other");
    assert_eq!(out.persons[1].team_context_id, None);
}

#[test]
fn prompt_numbers_candidates() {
    let c = candidates();
    let prompt = build_graph_prompt("skysports", "2026-07-19", "Title here", "Body", &c);
    assert!(prompt.contains("1. Morgan Rogers"));
    assert!(prompt.contains("2. Chelsea (team)"));
    assert!(prompt.contains("Return the JSON now."));
}

use serde_json::json;
struct Model {
    response: Option<String>,
    requests: std::sync::Mutex<Vec<(String, crate::studio::model::GenerateOptions)>>,
}

#[async_trait::async_trait]
impl crate::studio::model::Inference for Model {
    async fn generate(
        &self,
        prompt: &str,
        opts: &crate::studio::model::GenerateOptions,
    ) -> Result<(crate::studio::model::GenerateResult, serde_json::Value)> {
        self.requests
            .lock()
            .unwrap()
            .push((prompt.to_string(), opts.clone()));
        let response = self
            .response
            .clone()
            .ok_or_else(|| anyhow::anyhow!("transport unavailable"))?;
        Ok((
            crate::studio::model::GenerateResult {
                response,
                thinking: String::new(),
                model: "actual-graph-model".into(),
                total_duration: std::time::Duration::from_millis(12),
                prompt_eval_count: 5,
                eval_count: 10,
                completion_reason: Some("stop".into()),
                raw_response_body: String::new(),
            },
            self.request_body(prompt, opts),
        ))
    }
    fn model(&self) -> &str {
        "configured-graph-model"
    }
    fn request_body(
        &self,
        prompt: &str,
        opts: &crate::studio::model::GenerateOptions,
    ) -> serde_json::Value {
        json!({"prompt": prompt, "schema": opts.format_schema_raw, "budget": opts.num_predict})
    }
}

fn assignment() -> Assignment {
    Assignment {
        article: GraphArticle {
            source: "Wire".into(),
            published: "2026-09-19".into(),
            title: "Title".into(),
            description: "Body".into(),
        },
        candidates: candidates(),
    }
}
#[tokio::test]
async fn studio_graph_uses_prepared_evidence_and_actual_model_provenance() {
    let model=Model { response:Some(r#"{"relations":[{"subject":1,"predicate":"praise","object":2,"confidence":"reported"}],"persons":[]}"#.into()),requests:Default::default() };
    let assignment = assignment();
    let result = crate::plugins::graph::cognition::extract_graph(&Studio::new(&model), &assignment)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.model, "actual-graph-model");
    assert_eq!(result.value.unwrap().relations[0].object_id, Some(18));
    let calls = model.requests.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0,
        build_graph_prompt(
            "Wire",
            "2026-09-19",
            "Title",
            "Body",
            &assignment.candidates
        )
    );
    assert_eq!(
        calls[0].1.num_ctx,
        crate::studio::model::LOCAL_STAGE_NUM_CTX
    );
    assert_eq!(calls[0].1.system.as_deref(), Some(GRAPH_SYSTEM_PROMPT));
}
#[tokio::test]
async fn studio_graph_distinguishes_no_material_fail_closed_and_transport_failure() {
    let model = Model {
        response: None,
        requests: Default::default(),
    };
    let mut empty = assignment();
    empty.candidates.clear();
    assert!(
        crate::plugins::graph::cognition::extract_graph(&Studio::new(&model), &empty)
            .await
            .unwrap()
            .is_none()
    );
    assert!(model.requests.lock().unwrap().is_empty());
    assert!(
        crate::plugins::graph::cognition::extract_graph(&Studio::new(&model), &assignment())
            .await
            .is_err()
    );
    let model = Model {
        response: Some("unparseable".into()),
        requests: Default::default(),
    };
    assert!(
        crate::plugins::graph::cognition::extract_graph(&Studio::new(&model), &assignment())
            .await
            .unwrap()
            .unwrap()
            .value
            .is_none()
    );
}

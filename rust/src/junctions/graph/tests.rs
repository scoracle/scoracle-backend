//! Unit tests for this junction.
//!
//! Split out of `mod.rs` so the stage module reads as the stage and nothing else.
//! `super` still resolves to the junction, so these run exactly as they did inline.

use super::*;

fn candidates() -> Vec<GraphCandidate> {
    vec![
        GraphCandidate {
            entity_type: "player".into(),
            entity_id: 10,
            descriptor: "Morgan Rogers, an English midfielder, currently at Aston Villa."
                .into(),
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

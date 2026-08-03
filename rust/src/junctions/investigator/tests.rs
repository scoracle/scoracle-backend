//! The 5.7 adversarial fixture gate — every case in
//! `fixtures/investigate_entity/cases.json` must decide exactly as written (100% or the
//! phase does not close). Cases feed [`gate::decide`] directly: the nrm name screen and
//! the team discriminator arrive as recorded inputs (they belong to SQL — mig 198), so
//! what these fixtures pin is the DECISION LOGIC — the part a silent edit could bend.

use super::discover::WikidataItem;
use super::gate::{decide, RoleClass, Verdict};
use serde_json::Value;

fn item_from_json(v: &Value) -> WikidataItem {
    let strs = |key: &str| -> Vec<String> {
        v.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    WikidataItem {
        label: v.get("label").and_then(Value::as_str).unwrap_or("").to_string(),
        description: v
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        occupations: strs("occupations"),
        member_of_teams: strs("member_of_teams"),
        coach_of_teams: strs("coach_of_teams"),
        ..Default::default()
    }
}

fn bools(v: &Value, key: &str) -> Vec<bool> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_bool).collect())
        .unwrap_or_default()
}

fn verdict_label(v: &Verdict) -> String {
    match v {
        Verdict::Accept { role, .. } => {
            let kind = match role {
                RoleClass::Player => "player",
                RoleClass::Coach => "coach",
                RoleClass::Executive => "executive",
                RoleClass::Owner => "owner",
                RoleClass::Agent => "agent",
                RoleClass::Official => "official",
                RoleClass::Unknown => "unknown",
            };
            format!("accept:{kind}")
        }
        Verdict::Ambiguous { .. } => "ambiguous".to_string(),
        Verdict::RejectedNotSport => "rejected_not_sport".to_string(),
        Verdict::RejectedInsufficientEvidence => "rejected_insufficient_evidence".to_string(),
    }
}

#[test]
fn adversarial_fixture_gate_is_one_hundred_percent() {
    let raw = include_str!("../../../fixtures/investigate_entity/cases.json");
    let doc: Value = serde_json::from_str(raw).expect("cases.json parses");
    let cases = doc["cases"].as_array().expect("cases array");
    assert!(cases.len() >= 10, "the adversarial set must not shrink silently");

    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let sport = case["sport"].as_str().expect("sport");
        let items: Vec<WikidataItem> = case["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(item_from_json)
            .collect();
        let name_agreed = bools(case, "name_agreed");
        let team_matched = bools(case, "team_matched");
        let expect = case["expect"].as_str().expect("expect");

        let got = verdict_label(&decide(sport, &items, &name_agreed, &team_matched));
        if got != expect {
            failures.push(format!("{name}: expected {expect}, got {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "gate fixtures failed:\n{}",
        failures.join("\n")
    );
}

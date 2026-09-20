//! The 5.7 adversarial fixture gate — every case in
//! `fixtures/investigate_entity/cases.json` must decide exactly as written (100% or the
//! phase does not close). Cases feed [`gate::decide`] directly: the nrm name screen and
//! the team discriminator arrive as recorded inputs (they belong to SQL — mig 198), so
//! what these fixtures pin is the DECISION LOGIC — the part a silent edit could bend.

use super::gate::{decide, RoleClass, Verdict};
use super::WikidataItem;
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
        label: v
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
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
    assert!(
        cases.len() >= 10,
        "the adversarial set must not shrink silently"
    );

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

use super::{Assignment, Studio};
use anyhow::Result;
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
                model: "actual-investigator-model".into(),
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
        "configured-investigator-model"
    }
    fn request_body(
        &self,
        prompt: &str,
        opts: &crate::studio::model::GenerateOptions,
    ) -> serde_json::Value {
        json!({"prompt": prompt, "schema": opts.format_schema_raw, "budget": opts.num_predict})
    }
}

#[tokio::test]
async fn studio_validates_only_the_page_text_actually_shown() {
    let extract = format!(
        "Riley Example is a basketball coach for Test Club. {} Hidden Club",
        "x".repeat(4000)
    );
    let assignment = Assignment {
        sought_name: "Riley Example",
        descriptor: Some("basketball coach"),
        sport: "NBA",
        title: "Riley Example",
        description: "Basketball coach",
        extract: &extract,
    };
    let model = Model { response: Some(json!({"subject_kind":"person", "sought_name_evidence":"Riley Example", "occupation_phrase":"basketball coach", "team_names":["Test Club", "Hidden Club", "Invented Club"]}).to_string()), requests: Default::default() };
    let result = Studio::new(&model)
        .investigate_prose(&assignment)
        .await
        .unwrap();
    assert_eq!(result.model, "actual-investigator-model");
    let read = result.value.unwrap();
    assert_eq!(read.team_names, ["Test Club"]);
    assert_eq!(read.sought_name_evidence, "Riley Example");
    let requests = model.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].0.contains("Hidden Club"));
    assert_eq!(
        requests[0].1.num_ctx,
        crate::studio::model::LOCAL_STAGE_NUM_CTX
    );
    assert_eq!(
        requests[0].1.format_schema_raw.as_deref(),
        Some(super::prompt::INVESTIGATOR_PROSE_SCHEMA_RAW)
    );
}

#[tokio::test]
async fn studio_distinguishes_abstention_from_transport_failure() {
    let assignment = Assignment {
        sought_name: "Riley",
        descriptor: None,
        sport: "NBA",
        title: "Riley",
        description: "",
        extract: "",
    };
    for response in [Some("no JSON".to_string()), None] {
        let model = Model {
            response: response.clone(),
            requests: Default::default(),
        };
        let result = Studio::new(&model).investigate_prose(&assignment).await;
        if response.is_some() {
            assert!(result.unwrap().value.is_none());
        } else {
            assert!(result.is_err());
        }
        assert_eq!(model.requests.lock().unwrap().len(), 1);
    }
}

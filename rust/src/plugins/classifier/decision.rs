//! Classifier stage 1: a System-1 decision model over llama.cpp `/v1/systemone`.
//! One call per article scores the five calibrated buckets; the character model unpacks nuance.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const CRITERIA: &str = include_str!("../../../fixtures/classifier/classification-criteria-v1.json");
pub const CRITERIA_VERSION: &str = "classification-criteria-v4";
pub const MODEL_ID: &str = "ggml-org/Kev-0.8B-GGUF";

/// HTTP client for the decision-model server. Cheap to clone; the transport is the swap point.
#[derive(Clone)]
pub struct Decision {
    url: String,
    client: reqwest::Client,
}

impl Decision {
    /// `CLASSIFIER_URL` points at the llama.cpp server exposing `/v1/systemone`.
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("CLASSIFIER_URL")
            .context("CLASSIFIER_URL is required to enable the classifier stage")?;
        Ok(Self { url, client: reqwest::Client::new() })
    }

    /// Score the five buckets for one `(article, target)`. Returns `criterion -> 0..100`.
    pub async fn spectrum(&self, body: &str, target: &str) -> Result<BTreeMap<String, f64>> {
        let criteria: Value = serde_json::from_str(CRITERIA)?;
        let payload = json!({
            "state": {"target": target, "body": body},
            "questions": criteria["questions"],
        });
        let response: Value = self
            .client
            .post(format!("{}/v1/systemone", self.url))
            .json(&payload)
            .send()
            .await
            .context("classifier decision request")?
            .error_for_status()
            .context("classifier decision status")?
            .json()
            .await
            .context("classifier decision body")?;
        let answers = response["answers"].as_object().context("decision answers")?;
        let mut spectrum = BTreeMap::new();
        for (name, answer) in answers {
            match answer["type"].as_str() {
                Some("noul") => {
                    spectrum.insert(name.clone(), answer["noul"].as_f64().context("noul")? * 100.0);
                }
                Some("choice") => {
                    for (label, probability) in answer["probabilities"].as_object().context("choice probabilities")? {
                        spectrum.insert(format!("{name}.{label}"), probability.as_f64().context("choice probability")? * 100.0);
                    }
                }
                _ => {}
            }
        }
        Ok(spectrum)
    }
}

/// Evaluate the routing gate over a spectrum (0..100). `character -> fires`.
pub fn gate(spectrum: &BTreeMap<String, f64>) -> Result<BTreeMap<String, bool>> {
    let criteria: Value = serde_json::from_str(CRITERIA)?;
    let mut result = BTreeMap::new();
    for (character, rule) in criteria["characters"].as_object().context("gate characters")? {
        let mode = rule["mode"].as_str().unwrap_or("any");
        let rules = rule["criteria"].as_object().context("gate criteria")?;
        let mut fired = Vec::new();
        for (pattern, threshold) in rules {
            let threshold = threshold.as_f64().unwrap_or(0.0);
            let value = match pattern.strip_suffix(".*") {
                Some(prefix) => spectrum
                    .iter()
                    .filter(|(key, _)| key.starts_with(&format!("{prefix}.")))
                    .map(|(_, value)| *value)
                    .fold(0.0f64, f64::max),
                None => *spectrum.get(pattern).unwrap_or(&0.0),
            };
            fired.push(value >= threshold);
        }
        let ok = if mode == "all" { fired.iter().all(|f| *f) } else { fired.iter().any(|f| *f) };
        result.insert(character.clone(), ok);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_fires_per_bucket() {
        let mut spectrum = BTreeMap::new();
        spectrum.insert("emotional_charge".to_string(), 80.0);
        spectrum.insert("availability".to_string(), 60.0);
        spectrum.insert("relevance.direct_subject".to_string(), 70.0);
        let gate = gate(&spectrum).unwrap();
        assert!(gate["influencer"], "emotional_charge 80 >= 50");
        assert!(gate["scout"], "availability 60 >= 50");
        assert!(gate["journalist"], "direct_subject 70 >= 50");
        assert!(!gate["insider"], "no movement/staffing");
    }

    #[test]
    fn criteria_declares_six_questions() {
        let criteria: Value = serde_json::from_str(CRITERIA).unwrap();
        assert_eq!(criteria["questions"].as_object().unwrap().len(), 6);
    }
}

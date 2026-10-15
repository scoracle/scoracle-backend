//! Classifier stage 1: a System-1 decision model over llama.cpp `/v1/systemone`.
//! One call per (article, target) scores six provisional predicates; characters unpack nuance.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const CRITERIA: &str = include_str!("../../../fixtures/classifier/classification-criteria-v1.json");
pub const CRITERIA_VERSION: &str = "classification-criteria-v4";
pub const MODEL_ID: &str = "ggml-org/Kev-0.8B-GGUF";

pub struct Measurement {
    pub spectrum: BTreeMap<String, f64>,
    pub model: String,
    pub request_hash: String,
    pub receipt: Value,
}

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
        Self::new(url)
    }

    pub fn new(url: String) -> Result<Self> {
        Ok(Self {
            url: url.trim_end_matches('/').into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
        })
    }

    pub async fn identity(&self) -> Result<String> {
        let inventory: Value = self
            .client
            .get(format!("{}/v1/models", self.url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let models = inventory["data"]
            .as_array()
            .context("decision model inventory")?;
        ensure!(
            models.len() == 1,
            "Classifier requires one loaded Kev model"
        );
        Ok(model_identity(&json!({"model":models[0]["id"]}))?.to_string())
    }

    /// Score one `(article, target)`. Returns `criterion -> 0..100`.
    pub async fn spectrum(&self, body: &str, target: &str) -> Result<Measurement> {
        let payload = request(body, target)?;
        let started = std::time::Instant::now();
        let response = self
            .client
            .post(format!("{}/v1/systemone", self.url))
            .json(&payload)
            .send()
            .await
            .context("classifier decision request")?;
        let status = response.status();
        let raw = response.text().await.context("classifier decision body")?;
        (|| {
            ensure!(status.is_success(), "classifier decision HTTP {status}");
            let response: Value = serde_json::from_str(&raw)?;
            let spectrum = decode(&response)?;
            let model = model_identity(&response)?.to_string();
            Ok(Measurement {
                spectrum,
                model,
                request_hash: super::hash(&payload.to_string()),
                receipt: json!({"request":payload,"response":response,"endpoint":self.url,
                    "elapsed_ms":started.elapsed().as_millis()}),
            })
        })()
        .map_err(|error| {
            crate::harness::model::ResponseFailure {
                raw_response_body: raw,
                error,
            }
            .into()
        })
    }
}

pub(crate) fn request(body: &str, target: &str) -> Result<Value> {
    let criteria: Value = serde_json::from_str(CRITERIA)?;
    Ok(json!({"state":{"target":target,"body":body},"questions":criteria["questions"]}))
}

fn model_identity(response: &Value) -> Result<&str> {
    let model = response["model"]
        .as_str()
        .context("decision model identity")?;
    let prefix = format!("{MODEL_ID}@sha256:");
    let fingerprint = model
        .strip_prefix(&prefix)
        .context("Classifier requires the fingerprinted Kev model alias")?;
    ensure!(
        fingerprint.len() == 64 && fingerprint.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid Kev weights fingerprint"
    );
    Ok(model)
}

fn decode(response: &Value) -> Result<BTreeMap<String, f64>> {
    let criteria: Value = serde_json::from_str(CRITERIA)?;
    let questions = criteria["questions"]
        .as_object()
        .context("decision questions")?;
    let answers = response["answers"]
        .as_object()
        .context("decision answers")?;
    ensure!(
        answers.len() == questions.len(),
        "incomplete decision answers"
    );
    questions
        .keys()
        .map(|name| {
            let answer = &response["answers"][name];
            ensure!(answer["type"] == "noul", "{name}: expected noul answer");
            let probability = answer["noul"].as_f64().context("decision probability")?;
            ensure!(
                probability.is_finite() && (0.0..=1.0).contains(&probability),
                "{name}: invalid probability"
            );
            Ok((name.clone(), probability * 100.0))
        })
        .collect()
}

/// Evaluate the routing gate over a spectrum (0..100). `character -> fires`.
pub fn gate(spectrum: &BTreeMap<String, f64>) -> Result<BTreeMap<String, bool>> {
    let criteria: Value = serde_json::from_str(CRITERIA)?;
    let mut result = BTreeMap::new();
    for (character, rule) in criteria["characters"]
        .as_object()
        .context("gate characters")?
    {
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
        let ok = if mode == "all" {
            fired.iter().all(|f| *f)
        } else {
            fired.iter().any(|f| *f)
        };
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
        assert_eq!(criteria["version"], CRITERIA_VERSION);
    }

    #[test]
    fn decisions_require_every_requested_probability() {
        let criteria: Value = serde_json::from_str(CRITERIA).unwrap();
        let answers: serde_json::Map<String, Value> = criteria["questions"]
            .as_object()
            .unwrap()
            .keys()
            .map(|key| (key.clone(), json!({"type":"noul", "noul":0.75})))
            .collect();
        let response = json!({"answers":answers});
        assert_eq!(decode(&response).unwrap()["movement"], 75.0);
        for invalid in [json!(null), json!(-0.1), json!(1.1), json!("0.75")] {
            let mut response = response.clone();
            response["answers"]["movement"]["noul"] = invalid;
            assert!(decode(&response).is_err());
        }
        let mut missing = response.clone();
        missing["answers"]
            .as_object_mut()
            .unwrap()
            .remove("movement");
        assert!(decode(&missing).is_err());
        let mut wrong_type = response;
        wrong_type["answers"]["movement"]["type"] = json!("choice");
        assert!(decode(&wrong_type).is_err());
    }

    #[test]
    fn model_identity_requires_the_actual_kev_weights_fingerprint() {
        let model = format!("{MODEL_ID}@sha256:{}", "0".repeat(64));
        assert_eq!(model_identity(&json!({"model":model})).unwrap(), model);
        for wrong in ["laya", MODEL_ID, "", "ggml-org/Kev-0.8B-GGUF@sha256:short"] {
            assert!(model_identity(&json!({"model":wrong})).is_err());
        }
    }
}

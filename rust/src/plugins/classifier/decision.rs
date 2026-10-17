//! Classifier stage 1: a System-1 decision model over llama.cpp `/v1/systemone`.
//! One call per RSS subject scores relevance and three independent topics.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const CRITERIA: &str =
    include_str!("../../../fixtures/classifier/classification-criteria-v6-subject.json");
pub const CRITERIA_VERSION: &str = "classification-criteria-v6-subject";

pub struct Measurement {
    pub spectrum: BTreeMap<String, f64>,
    pub model: String,
    pub request_hash: String,
    pub receipt: Value,
}

impl Measurement {
    pub(crate) fn from_receipt(receipt: Value) -> Result<Self> {
        Ok(Self {
            spectrum: decode(&receipt["response"])?,
            model: model_identity(&receipt["response"])?.to_string(),
            request_hash: super::hash(&receipt["request"].to_string()),
            receipt,
        })
    }
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
            "Classifier requires one loaded decision model"
        );
        Ok(model_identity(&json!({"model":models[0]["id"]}))?.to_string())
    }

    /// Score the complete article for one canonical subject. Returns `criterion -> 0..100`.
    pub async fn spectrum(&self, body: &str, sport: &str, meta: &Value) -> Result<Measurement> {
        let payload = request(body, sport, meta)?;
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
            Measurement::from_receipt(
                json!({"request":payload,"response":response,"endpoint":self.url,
                "elapsed_ms":started.elapsed().as_millis()}),
            )
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

pub(crate) fn request(body: &str, sport: &str, meta: &Value) -> Result<Value> {
    let entity: crate::tools::meta::EntityMeta = serde_json::from_value(meta["identity"].clone())?;
    ensure!(
        entity.sport == sport && entity.entity_id > 0 && !entity.name.trim().is_empty(),
        "invalid Classifier subject metadata"
    );
    let mut criteria: Value = serde_json::from_str(CRITERIA)?;
    for question in criteria["questions"]
        .as_object_mut()
        .context("questions")?
        .values_mut()
    {
        question["instructions"] = json!(format!("Use subject metadata only to identify the entity. Judge evidence from the article only. {}",
            question["instructions"].as_str().context("instructions")?.replace("{subject}", &entity.reference())));
    }
    Ok(
        json!({"state":{"subject_meta":meta,"article":{"sport":sport,"body":body}},"questions":criteria["questions"]}),
    )
}

fn model_identity(response: &Value) -> Result<&str> {
    let model = response["model"]
        .as_str()
        .context("decision model identity")?;
    let (name, fingerprint) = model
        .rsplit_once("@sha256:")
        .context("Classifier requires a fingerprinted decision model alias")?;
    ensure!(
        !name.is_empty()
            && !name.chars().any(char::is_whitespace)
            && !name.contains('@')
            && fingerprint.len() == 64
            && fingerprint.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid decision model weights fingerprint"
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

/// Preserve the existing source contract on character hosts while native receipts retain four topics.
pub(crate) fn consumer_spectrum(spectrum: &BTreeMap<String, f64>) -> Value {
    let mut result = json!(spectrum);
    for (role, topic) in [
        ("journalist", "subject"),
        ("influencer", "emotion"),
        ("scout", "availability"),
        ("insider", "movement"),
    ] {
        if let Some(score) = spectrum.get(topic) {
            result[role] = json!(score);
        }
    }
    result
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
        spectrum.insert("subject".to_string(), 55.0);
        spectrum.insert("emotion".to_string(), 15.0);
        spectrum.insert("availability".to_string(), 90.0);

        let gate = gate(&spectrum).unwrap();
        assert!(gate["influencer"], "subject relevance and emotion");
        assert!(gate["scout"], "subject relevance and availability");
        assert!(
            gate["journalist"],
            "neutral subject reporting remains available"
        );
        assert!(!gate["insider"], "no movement score");
        spectrum.insert("movement".to_string(), 79.9);
        assert!(!super::gate(&spectrum).unwrap()["insider"]);
        spectrum.insert("movement".to_string(), 80.0);
        assert!(super::gate(&spectrum).unwrap()["insider"]);
    }

    #[test]
    fn criteria_declares_four_subject_questions() {
        let criteria: Value = serde_json::from_str(CRITERIA).unwrap();
        assert_eq!(criteria["questions"].as_object().unwrap().len(), 4);
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
    fn relevance_is_required_and_neutral_reporting_reaches_journalist() {
        let mut spectrum = BTreeMap::from([
            ("subject".into(), 55.0),
            ("emotion".into(), 0.0),
            ("movement".into(), 0.0),
            ("availability".into(), 0.0),
        ]);
        let compatibility = consumer_spectrum(&spectrum);
        assert_eq!(compatibility["journalist"], 55.0);
        assert_eq!(compatibility["scout"], compatibility["availability"]);
        let routed = gate(&spectrum).unwrap();
        assert!(routed["journalist"]);
        assert!(!routed["influencer"] && !routed["insider"] && !routed["scout"]);
        spectrum.insert("subject".into(), 54.9);
        spectrum.insert("emotion".into(), 100.0);
        spectrum.insert("movement".into(), 100.0);
        spectrum.insert("availability".into(), 100.0);
        assert!(gate(&spectrum).unwrap().values().all(|fires| !fires));
        let meta = json!({"identity":{"name":"Cedar FC","sport":"FOOTBALL","entity_id":1,"entity_type":"team"},"identity_record":"Captain injured; new coach on record."});
        let request = request("A neutral report.", "FOOTBALL", &meta).unwrap();
        assert_eq!(request["state"]["article"]["body"], "A neutral report.");
        assert_eq!(request["state"]["subject_meta"], meta);
        assert!(request["questions"]["subject"]["instructions"]
            .as_str()
            .unwrap()
            .contains("Cedar FC, the FOOTBALL team"));
    }

    #[test]
    fn model_identity_accepts_replaceable_fingerprinted_models() {
        for name in ["ggml-org/Kev-0.8B-GGUF", "LiquidAI/d1-3B-GGUF"] {
            let model = format!("{name}@sha256:{}", "0".repeat(64));
            assert_eq!(model_identity(&json!({"model":model})).unwrap(), model);
        }
        for wrong in [
            "laya".to_string(),
            String::new(),
            "model@sha256:short".to_string(),
            format!("@sha256:{}", "0".repeat(64)),
            format!("model name@sha256:{}", "0".repeat(64)),
            format!("model@sha256:{}", "g".repeat(64)),
            format!("model@other@sha256:{}", "0".repeat(64)),
        ] {
            assert!(model_identity(&json!({"model":wrong})).is_err());
        }
    }
}

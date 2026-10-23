//! Classifier stage 1: a System-1 decision model over llama.cpp `/v1/systemone`.
//! A compact spectrum per RSS subject; publisher context only disambiguates identity.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const CRITERIA: &str =
    include_str!("../../../fixtures/classifier/classification-criteria-v10-availability.json");
const ROUTING: &str = include_str!("../../../fixtures/classifier/headline-routing-v2.json");
pub const ROUTING_VERSION: &str = "headline-routing-v2";
pub const CRITERIA_VERSION: &str = "classification-criteria-v10-availability";

pub struct Measurement {
    pub spectrum: BTreeMap<String, f64>,
    pub model: String,
    pub request_hash: String,
    pub receipt: Value,
}

impl Measurement {
    pub(crate) fn from_receipt(receipt: Value) -> Result<Self> {
        let mut response = receipt["response"].clone();
        if receipt["request"]["questions"].get("clickbait").is_some() {
            response["answers"]
                .as_object_mut()
                .context("saved decision answers")?
                .remove("clickbait");
        }
        Ok(Self {
            spectrum: decode(&response)?,
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
            .context("CLASSIFIER_URL is required to enable headline scoring")?;
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

    /// Score the RSS headline for one canonical subject. Returns `criterion -> 0..100`.
    pub async fn spectrum(
        &self,
        headline: &str,
        publisher: &str,
        sport: &str,
        meta: &Value,
    ) -> Result<Measurement> {
        let payload = request(headline, publisher, sport, meta)?;
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
        let result = (|| {
            ensure!(status.is_success(), "classifier decision HTTP {status}");
            let response: Value = serde_json::from_str(&raw)?;
            Measurement::from_receipt(
                json!({"request":payload,"response":response,"endpoint":self.url,
                "elapsed_ms":started.elapsed().as_millis()}),
            )
        })();
        if let Err(error) = &result {
            crate::harness::rejections::record(
                "classifier-decision",
                &payload.to_string(),
                &payload,
                None,
                &raw,
                error,
                1,
            )
            .await?;
        }
        result.map_err(|error| {
            crate::harness::model::ResponseFailure {
                raw_response_body: raw,
                error,
            }
            .into()
        })
    }
}

pub fn request(headline: &str, publisher: &str, sport: &str, meta: &Value) -> Result<Value> {
    ensure!(
        !headline.trim().is_empty() && !headline.contains('\0'),
        "missing or invalid RSS headline"
    );
    ensure!(!publisher.contains('\0'), "invalid RSS publisher");
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
        question["instructions"] = json!(format!("Use subject metadata only to identify the entity. Judge reporting from the RSS headline; use its publisher name only to disambiguate the subject. Publisher identity alone does not establish relevance, emotion or movement. {}",
            question["instructions"].as_str().context("instructions")?.replace("{subject}", &entity.reference())));
    }
    Ok(
        json!({"state":{"subject_meta":meta,"article":{"sport":sport,"headline":headline,"publisher":publisher}},"questions":criteria["questions"]}),
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

/// Admission is independent of downstream character selection.
pub fn admitted(spectrum: &BTreeMap<String, f64>) -> Result<bool> {
    let criteria: Value = serde_json::from_str(ROUTING)?;
    Ok(*spectrum
        .get("subject")
        .context("missing subject probability")?
        >= criteria["admission"]["subject"]
            .as_f64()
            .context("subject cutoff")?)
}

/// Character relevance is measured on the same headline, through that character's lens.
pub fn characters(spectrum: &BTreeMap<String, f64>) -> Result<Value> {
    let criteria: Value = serde_json::from_str(ROUTING)?;
    let mut selected = serde_json::Map::new();
    for (character, cutoffs) in criteria["characters"]
        .as_object()
        .context("character cutoffs")?
    {
        let mut relevant = true;
        for (criterion, cutoff) in cutoffs.as_object().context("character criteria")? {
            relevant &= spectrum
                .get(criterion)
                .context("missing character spectrum score")?
                >= &cutoff.as_f64().context("character cutoff")?;
        }
        selected.insert(character.clone(), json!(relevant));
    }
    Ok(Value::Object(selected))
}

/// Preserve model scores while recording the separate delivery policy and rejection reason.
pub fn gate(spectrum: &BTreeMap<String, f64>, headline: &str, entity_type: &str) -> Result<Value> {
    let spam = crate::tools::source::is_headline_spam(headline);
    let accepted = admitted(spectrum)? && !spam;
    let mut selected = characters(spectrum)?;
    if spam {
        for value in selected
            .as_object_mut()
            .context("character selection")?
            .values_mut()
        {
            *value = json!(false);
        }
    } else if !matches!(entity_type, "team" | "player") {
        selected["scout"] = json!(false);
    }
    Ok(
        json!({"coverage":"complete","scope":"headline","admitted":accepted,
        "characters":selected,"routing_version":ROUTING_VERSION,
        "reason":if spam {Some("livestream_spam")} else {None}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_requires_only_relevance_and_keeps_publisher_as_evidence() {
        assert!(admitted(&BTreeMap::from([("subject".into(), 55.0)])).unwrap());
        assert!(!admitted(&BTreeMap::from([("subject".into(), 54.9)])).unwrap());
        assert!(admitted(&BTreeMap::new()).is_err());
        let meta = json!({"identity":{"name":"Cedar FC","sport":"FOOTBALL","entity_id":1,"entity_type":"team"},"identity_record":"Historical identity, not article evidence."});
        let payload = request("Cedar FC update", "Fixture Wire", "FOOTBALL", &meta).unwrap();
        assert_eq!(payload["state"]["article"]["publisher"], "Fixture Wire");
        assert_eq!(payload["questions"].as_object().unwrap().len(), 4);
        assert!(payload["questions"].get("clickbait").is_none());
        assert!(payload["state"]["article"]["body"].is_null());
        assert!(payload["questions"]["subject"]["instructions"]
            .as_str()
            .unwrap()
            .contains("Publisher identity alone does not establish relevance"));
        for headline in [" ", "bad\0headline"] {
            assert!(request(headline, "Fixture Wire", "FOOTBALL", &meta).is_err());
        }
        assert!(request("Update", "bad\0publisher", "FOOTBALL", &meta).is_err());
        assert_ne!(
            super::super::hash(&payload.to_string()),
            super::super::hash(
                &request("Cedar FC update", "Another publisher", "FOOTBALL", &meta)
                    .unwrap()
                    .to_string()
            )
        );
    }

    #[test]
    fn character_lenses_select_relevant_narrative_material() {
        let mut scores = BTreeMap::from([
            ("subject".into(), 95.0),
            ("emotion".into(), 14.9),
            ("movement".into(), 79.9),
            ("availability".into(), 0.0),
        ]);
        assert_eq!(
            characters(&scores).unwrap(),
            json!({"journalist":true,"influencer":false,"insider":false,"scout":false})
        );
        scores.insert("emotion".into(), 15.0);
        scores.insert("movement".into(), 80.0);
        assert_eq!(
            characters(&scores).unwrap(),
            json!({"journalist":true,"influencer":true,"insider":true,"scout":false})
        );
        scores.insert("subject".into(), 54.9);
        assert_eq!(
            characters(&scores).unwrap(),
            json!({"journalist":false,"influencer":false,"insider":false,"scout":false})
        );
    }

    #[test]
    fn availability_routing_preserves_named_denial_and_irrelevant_calibration() {
        let receipt: Value = serde_json::from_str(include_str!(
            "../../../fixtures/classifier/availability-calibration-receipt-2026-10-22.json"
        ))
        .unwrap();
        for case in receipt["cases"].as_array().unwrap() {
            let scores = serde_json::from_value(case["spectrum"].clone()).unwrap();
            assert_eq!(
                characters(&scores).unwrap()["scout"],
                case["expected"]["availability"],
                "{}",
                case["id"]
            );
        }
        let scores = BTreeMap::from([
            ("subject".into(), 55.0),
            ("emotion".into(), 0.0),
            ("movement".into(), 0.0),
            ("availability".into(), 85.0),
        ]);
        assert_eq!(
            characters(&scores).unwrap(),
            json!({"journalist":true,"influencer":false,"insider":false,"scout":true})
        );
    }

    #[test]
    fn routing_holds_spam_and_opens_scout_at_eighty_five_without_changing_scores() {
        let mut scores = BTreeMap::from([
            ("subject".into(), 95.0),
            ("emotion".into(), 95.0),
            ("movement".into(), 95.0),
            ("availability".into(), 84.9),
        ]);
        let headline = "Broncos' Jaylen Waddle: Participating in Thursday's practice";
        assert_eq!(
            gate(&scores, headline, "player").unwrap()["characters"]["scout"],
            false
        );
        scores.insert("availability".into(), 85.0);
        assert_eq!(
            gate(&scores, headline, "player").unwrap()["characters"]["scout"],
            true
        );
        assert_eq!(
            gate(&scores, headline, "person").unwrap()["characters"]["scout"],
            false
        );
        assert_eq!(
            gate(
                &scores,
                "Clippers begin season without banned owner",
                "team"
            )
            .unwrap()["characters"]["scout"],
            true
        );
        let rejected = gate(
            &scores,
            "[+[LIVE@STREAMs]+] Cedar FC vs Maple United",
            "team",
        )
        .unwrap();
        assert_eq!(rejected["admitted"], false);
        assert_eq!(rejected["reason"], "livestream_spam");
        assert_eq!(rejected["routing_version"], ROUTING_VERSION);
        assert!(rejected["characters"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == false));
        assert_eq!(scores["subject"], 95.0);
        scores.insert("subject".into(), 54.9);
        assert_eq!(
            gate(&scores, headline, "player").unwrap()["characters"]["scout"],
            false
        );
    }

    #[test]
    fn decisions_require_every_headline_score() {
        let questions: Value = serde_json::from_str(CRITERIA).unwrap();
        let answers = questions["questions"]
            .as_object()
            .unwrap()
            .keys()
            .map(|key| (key.clone(), json!({"type":"noul","noul":0.75})))
            .collect::<serde_json::Map<_, _>>();
        let response = json!({"answers":answers});
        assert_eq!(decode(&response).unwrap()["subject"], 75.0);
        for invalid in [json!(null), json!(-0.1), json!(1.1), json!("0.75")] {
            let mut changed = response.clone();
            changed["answers"]["subject"]["noul"] = invalid;
            assert!(decode(&changed).is_err());
        }
        assert!(decode(&json!({"answers":{}})).is_err());
        let mut extra = response;
        extra["answers"]["invented"] = json!({"type":"noul","noul":0.9});
        assert!(decode(&extra).is_err());
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

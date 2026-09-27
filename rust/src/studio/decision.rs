//! Typed classification without text generation. Applications own IO and dispatch.
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChoiceQuestion {
    #[serde(rename = "type")]
    pub kind: String,
    pub instructions: String,
    pub criteria: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub state: String,
    pub questions: BTreeMap<String, ChoiceQuestion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionResponse {
    pub answers: BTreeMap<String, ChoiceAnswer>,
    /// Provider records actual checkpoint, runtime, device and input coverage here.
    pub provenance: Value,
    /// Exact provider envelope retained by the transport for offline inspection.
    #[serde(default, skip_deserializing)]
    pub raw_response: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChoiceAnswer {
    pub choice: String,
    pub probabilities: BTreeMap<String, f64>,
}

#[async_trait]
pub trait DecisionModel: Send + Sync {
    async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse>;
}

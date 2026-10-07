//! Non-generative model plugin. Consumers own questions, validation and routing policy.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum PredicateQuestion {
    #[serde(rename = "noul")]
    Boolean {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionRequest {
    pub state: String,
    pub questions: BTreeMap<String, PredicateQuestion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionResponse {
    pub answers: BTreeMap<String, ProbabilityAnswer>,
    /// Actual checkpoint, runtime, device and complete input coverage.
    pub provenance: Value,
    #[serde(default, skip_deserializing)]
    pub raw_response: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbabilityAnswer {
    #[serde(rename = "noul")]
    pub probability: f64,
}

#[async_trait]
pub trait DecisionModel: Send + Sync {
    async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse>;
}

/// Bind this capability to a deployment; callers name the configuration in prompt.rs.
pub fn bind(endpoint_env: &str) -> Result<std::sync::Arc<dyn DecisionModel>> {
    let endpoint = std::env::var(endpoint_env)
        .map_err(|_| anyhow::anyhow!("{endpoint_env} is required for System 1 scoring"))?;
    Ok(std::sync::Arc::new(
        crate::harness::providers::system_one::SystemOneClient::new(endpoint)?,
    ))
}

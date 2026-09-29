//! Explicit endpoint for local typed decisions; no generative fallback or transport retry.
use crate::plugins::cognition::decision::{DecisionModel, DecisionRequest, DecisionResponse};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::Duration;

pub struct SystemOneClient {
    endpoint: String,
    http: reqwest::Client,
}

impl SystemOneClient {
    pub fn new(endpoint: String) -> Result<Self> {
        Ok(Self {
            endpoint,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()?,
        })
    }
}

#[async_trait]
impl DecisionModel for SystemOneClient {
    async fn evaluate(&self, request: &DecisionRequest) -> Result<DecisionResponse> {
        let response = self
            .http
            .post(&self.endpoint)
            .json(request)
            .send()
            .await
            .context("call semantic intake")?
            .error_for_status()
            .context("semantic intake HTTP failure")?
            .json::<serde_json::Value>()
            .await
            .context("decode typed intake response")?;
        let mut decoded: DecisionResponse =
            serde_json::from_value(response.clone()).context("validate typed intake envelope")?;
        decoded.raw_response = response;
        Ok(decoded)
    }
}

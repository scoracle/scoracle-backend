//! Text-generating model plugin. Consumers own tasks, routes and inference settings.
use crate::harness::config::{Backend, ModelSpec};
use crate::harness::model::{GenerateOptions, GenerateResult, Inference};
use crate::harness::providers::llama::LlamaClient;
use crate::harness::providers::openai::OpenAiClient;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::{sync::Arc, time::Duration};

pub fn bind(spec: &ModelSpec, timeout: Duration) -> Result<Arc<dyn Inference>> {
    match spec.backend {
        Backend::LlamaCpp => Ok(Arc::new(LlamaClient::new(
            &spec.base_url,
            &spec.model,
            timeout,
            spec.think,
        )?)),
        Backend::OpenAi => Ok(Arc::new(
            OpenAiClient::new(&spec.base_url, &spec.model, timeout)
                .context("bind OpenAI-compatible generation plugin")?,
        )),
    }
}

#[async_trait]
impl Inference for OpenAiClient {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        OpenAiClient::generate_with_body(self, prompt, opts).await
    }

    fn model(&self) -> &str {
        OpenAiClient::model(self)
    }

    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
        OpenAiClient::request_body(self, prompt, opts)
    }
}

//! Ollama HTTP client — mirrors the Go `internal/ml` OllamaClient
//! (`go/internal/ml/ollama.go`). Targets the local Ollama instance (default
//! http://localhost:11434). No external providers; all inference stays local.
//!
//! Request/response field names match the Go structs byte-for-byte so the same
//! prompt + options produce the same wire payload — the basis for the Phase 1
//! temp-0 parity test against the Go stages.

use crate::util::truncate;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
}

/// GenerateOptions tunes a single call. Defaults mean "let Ollama default."
/// `temperature` and `num_predict` are omitted from the payload when <= 0,
/// matching the Go client.
#[derive(Clone, Debug, Default)]
pub struct GenerateOptions {
    pub system: Option<String>,
    pub temperature: f64,
    pub num_predict: i32,
    pub json_mode: bool, // sets format="json"
}

/// GenerateResult holds the text output plus perf metrics. Callers doing
/// debounce / perf tuning read the metrics; callers that just want the answer
/// read `response`.
#[derive(Clone, Debug)]
pub struct GenerateResult {
    pub response: String,
    pub model: String,
    pub total_duration: Duration,
    pub eval_count: i32,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GenerateResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    response: String,
    #[serde(default)]
    total_duration: i64, // nanoseconds
    #[serde(default)]
    eval_count: i32,
    #[serde(default)]
    error: String,
}

impl OllamaClient {
    /// new builds a client. `base_url` like "http://localhost:11434", `model`
    /// like "gemma4:e4b". A zero timeout defaults to 60s (Gemma e4b on a
    /// consumer GPU is typically 2-8s but can spike).
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, timeout: Duration) -> Result<Self> {
        let timeout = if timeout.is_zero() {
            Duration::from_secs(60)
        } else {
            timeout
        };
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .context("build reqwest client")?;
        Ok(Self {
            base_url: base_url.into(),
            model: model.into(),
            http,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// generate performs a single non-streaming completion. We do NOT auto-retry
    /// — the caller (a stage handler) decides, and the work queue handles backoff.
    pub async fn generate(&self, prompt: &str, opts: &GenerateOptions) -> Result<GenerateResult> {
        let mut options = serde_json::Map::new();
        if opts.temperature > 0.0 {
            options.insert("temperature".into(), serde_json::json!(opts.temperature));
        }
        if opts.num_predict > 0 {
            options.insert("num_predict".into(), serde_json::json!(opts.num_predict));
        }

        let req = GenerateRequest {
            model: &self.model,
            prompt,
            system: opts.system.as_deref(),
            stream: false,
            format: if opts.json_mode { Some("json") } else { None },
            options: if options.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(options))
            },
        };

        let url = format!("{}/api/generate", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .context("ollama request")?;

        let status = resp.status();
        let raw = resp.text().await.context("read ollama response")?;
        if !status.is_success() {
            return Err(anyhow!("ollama HTTP {}: {}", status.as_u16(), truncate(&raw, 300)));
        }

        let parsed: GenerateResponse = serde_json::from_str(&raw)
            .with_context(|| format!("decode ollama response (body={})", truncate(&raw, 200)))?;
        if !parsed.error.is_empty() {
            return Err(anyhow!("ollama error: {}", parsed.error));
        }

        Ok(GenerateResult {
            response: parsed.response,
            model: parsed.model,
            total_duration: Duration::from_nanos(parsed.total_duration.max(0) as u64),
            eval_count: parsed.eval_count,
        })
    }

    /// ping hits /api/tags to verify Ollama is reachable. Cheap — no inference.
    pub async fn ping(&self) -> Result<()> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self.http.get(&url).send().await.context("ollama ping")?;
        if !resp.status().is_success() {
            return Err(anyhow!("ollama ping HTTP {}", resp.status().as_u16()));
        }
        Ok(())
    }
}

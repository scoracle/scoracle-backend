//! Ollama HTTP client for the local inference boundary.
//!
//! Targets the local Ollama instance (default http://localhost:11434). No external
//! providers are used in production; all live inference stays in the Rust cognition layer.

use crate::util::truncate;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
    /// `Some(false)` sends `"think": false` on every generate — for reasoning models
    /// (qwen3-class) whose thinking otherwise consumes the whole `num_predict` budget and
    /// returns an EMPTY visible response (measured live: sigil's 512-token budget produced
    /// 512 tokens of thinking, zero answer). ROLE-keyed via `COGNITION_ROUTE_<ROLE>_THINK`
    /// (see `RouteConfig`): the same model may think for one role (PEAK: 22/22 with, 21/22
    /// without) and not for another (sigil: thinking breaks the stage). `None` omits the
    /// field entirely — models without the capability reject an explicit `think`.
    think: Option<bool>,
}

/// GenerateOptions tunes a single call. Defaults mean "let Ollama default."
///
/// `temperature` is an `Option` on purpose: `None` omits the field (Ollama uses
/// its own default, ~0.8, NON-deterministic) and `Some(t)` sends exactly `t` —
/// INCLUDING `Some(0.0)`. `num_predict` is still omitted when `<= 0`.
///
/// `num_ctx` (omitted when `<= 0`) overrides Ollama's server default context window —
/// 4096 tokens on this box (verified live via `ollama ps`), far below what the models
/// support (mistral:7b 32k). A prompt + `num_predict` sum that exceeds the window
/// silently evicts the EARLIEST tokens (the system prompt) mid-generation, degrading
/// rule-following with no error anywhere. Only set it where the budget genuinely
/// needs it: KV cache scales with the window, and the GPU fit (L7) was measured at
/// the 4096 default.
#[derive(Clone, Debug, Default)]
pub struct GenerateOptions {
    pub system: Option<String>,
    pub temperature: Option<f64>,
    pub num_predict: i32,
    pub num_ctx: i32,
    pub json_mode: bool, // sets format="json"
    /// A JSON schema for Ollama's constrained decoding (`format: <schema>`), supported since
    /// Ollama 0.5. Takes precedence over `json_mode`. This is a GRAMMAR guarantee on output
    /// shape — required keys cannot be omitted, no prose can leak around the object — versus
    /// `json_mode`'s "some valid JSON" and free-text's "hopefully JSON" (the narratives
    /// balanced-brace salvager exists because of the latter).
    pub format_schema: Option<serde_json::Value>,
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
    format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
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
    /// is the resolved local model tag. A zero timeout defaults to 60s because local models on
    /// consumer GPUs are typically quick but can spike under load.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self> {
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
            think: None,
        })
    }

    /// with_think builds a client with an explicit think preference (the Router's path — the
    /// role's `ModelSpec.think`). `new` keeps `None` for the offline bins and ping clients.
    pub fn with_think(
        base_url: impl Into<String>,
        model: impl Into<String>,
        timeout: Duration,
        think: Option<bool>,
    ) -> Result<Self> {
        let mut c = Self::new(base_url, model, timeout)?;
        c.think = think;
        Ok(c)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// build_request assembles the `/api/generate` request body for `(prompt, opts)`.
    /// Single source of truth shared by `generate` (what we actually POST) and
    /// `request_body` (used by request builders and ledger capture), so stored
    /// inspection data cannot drift from the sent request shape.
    fn build_request<'a>(
        &'a self,
        prompt: &'a str,
        opts: &'a GenerateOptions,
    ) -> GenerateRequest<'a> {
        let mut options = serde_json::Map::new();
        if let Some(t) = opts.temperature {
            options.insert("temperature".into(), serde_json::json!(t));
        }
        if opts.num_predict > 0 {
            options.insert("num_predict".into(), serde_json::json!(opts.num_predict));
        }
        if opts.num_ctx > 0 {
            options.insert("num_ctx".into(), serde_json::json!(opts.num_ctx));
        }
        GenerateRequest {
            model: &self.model,
            prompt,
            system: opts.system.as_deref(),
            stream: false,
            format: match (&opts.format_schema, opts.json_mode) {
                (Some(schema), _) => Some(schema.clone()),
                (None, true) => Some(serde_json::Value::String("json".to_string())),
                (None, false) => None,
            },
            think: self.think,
            options: if options.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(options))
            },
        }
    }

    /// request_body returns the exact JSON body `generate` would POST for
    /// `(prompt, opts)`. Deterministic request builders use this for inspection,
    /// ledger capture, and eval fixtures without performing a model call.
    pub fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
        serde_json::to_value(self.build_request(prompt, opts)).unwrap_or(serde_json::Value::Null)
    }

    pub(crate) async fn generate_with_body(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        let req = self.build_request(prompt, opts);
        let request_body = serde_json::to_value(&req).unwrap_or(serde_json::Value::Null);

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
            return Err(anyhow!(
                "ollama HTTP {}: {}",
                status.as_u16(),
                truncate(&raw, 300)
            ));
        }

        let parsed: GenerateResponse = serde_json::from_str(&raw)
            .with_context(|| format!("decode ollama response (body={})", truncate(&raw, 200)))?;
        if !parsed.error.is_empty() {
            return Err(anyhow!("ollama error: {}", parsed.error));
        }

        Ok((
            GenerateResult {
                response: parsed.response,
                model: parsed.model,
                total_duration: Duration::from_nanos(parsed.total_duration.max(0) as u64),
                eval_count: parsed.eval_count,
            },
            request_body,
        ))
    }

    /// generate performs a single non-streaming completion. We do NOT auto-retry
    /// — the caller (a stage handler) decides, and the work queue handles backoff.
    pub async fn generate(&self, prompt: &str, opts: &GenerateOptions) -> Result<GenerateResult> {
        let (gen, _) = self.generate_with_body(prompt, opts).await?;
        Ok(gen)
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

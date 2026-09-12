//! Ollama HTTP client for the local inference boundary.
//!
//! Targets the local Ollama instance (default http://localhost:11434). No external
//! providers are used in production; all live inference stays in the Rust cognition layer.
//!
//! Uses `/api/chat` so reasoning stays separate from visible output. Parsers receive only
//! `message.content`; separated thinking is retained for inspection.

use crate::util::truncate;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Additional reasoning budget for think-enabled calls.
pub const THINK_NUM_PREDICT_HEADROOM: i32 = 600;

#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
    /// Role-keyed thinking preference. `None` omits the field for unsupported models.
    think: Option<bool>,
}

/// GenerateOptions tunes a single call. Defaults mean "let Ollama default."
///
/// `temperature` is an `Option` on purpose: `None` omits the field (Ollama uses
/// its own default, ~0.8, NON-deterministic) and `Some(t)` sends exactly `t` —
/// INCLUDING `Some(0.0)`. `num_predict` is still omitted when `<= 0`.
///
/// `num_ctx <= 0` sends the 4096 envelope explicitly rather than inheriting a
/// potentially much larger model default.
/// A prompt + `num_predict` sum that exceeds the window still silently evicts the
/// EARLIEST tokens (the system prompt) mid-generation; size budgets accordingly.
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
    /// The same schema as VERBATIM JSON text, for stages whose contract pins the PROPERTY ORDER
    /// (PLAN-one-rail §1a: order IS the contract). `serde_json::Value` is BTreeMap-backed, so a
    /// schema that travels as a `Value` reaches Ollama with its properties ALPHABETIZED — the
    /// grammar then forces emission in that accidental order, whatever the documented contract
    /// says. When set, this string is POSTed byte-for-byte as `format` (taking precedence over
    /// `format_schema`); callers should set `format_schema` too, since ledger/eval capture
    /// still reads the `Value` form. Stages without an order-sensitive schema leave this `None`.
    pub format_schema_raw: Option<String>,
}

/// GenerateResult holds the text output plus perf metrics. Callers doing
/// debounce / perf tuning read the metrics; callers that just want the answer
/// read `response`.
///
/// `thinking` is the model's separated reasoning when the role runs `_THINK=true`
/// (empty otherwise, and empty on models without the capability). It exists for
/// ledger capture and eval display ONLY — no parser may read it, no card may
/// carry it. `eval_count` counts thinking + answer tokens together (that shared
/// budget is why think-enabled roles need `num_predict` headroom).
#[derive(Clone, Debug)]
pub struct GenerateResult {
    pub response: String,
    pub thinking: String,
    pub model: String,
    pub total_duration: Duration,
    pub eval_count: i32,
}

#[derive(Serialize)]
struct ChatTurn<'a> {
    role: &'a str,
    content: &'a str,
}

/// The `/api/chat` request. `GenerateOptions::system` becomes the system turn and the
/// caller's prompt the user turn — the same template application `/api/generate`'s
/// `system`/`prompt` pair produced, on the endpoint that can separate thinking.
#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    messages: Vec<ChatTurn<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<serde_json::Value>,
}

/// The POSTed shape when a caller pins schema property order (`format_schema_raw`): identical
/// to [`GenerateRequest`] except `format` is `RawValue`, serialized byte-for-byte. A separate
/// struct because `serde_json::to_value` cannot represent `RawValue` — the ledger/inspection
/// copy keeps flowing through [`GenerateRequest`], whose `Value` form it always stored. For a
/// caller with only `format_schema`, the wire bytes are identical either way (`Value::to_string`
/// is the same serialization `.json(&req)` performs).
#[derive(Serialize)]
struct GenerateRequestWire<'a> {
    model: &'a str,
    messages: Vec<ChatTurn<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a serde_json::value::RawValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct ChatTurnOwned {
    #[serde(default)]
    content: String,
    #[serde(default)]
    thinking: String,
}

#[derive(Deserialize)]
struct GenerateResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    message: ChatTurnOwned,
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

    /// build_request assembles the `/api/chat` request body for `(prompt, opts)`.
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
            // Thinking shares the answer budget, so add its headroom at the client boundary.
            let headroom = if self.think == Some(true) {
                THINK_NUM_PREDICT_HEADROOM
            } else {
                0
            };
            options.insert(
                "num_predict".into(),
                serde_json::json!(opts.num_predict + headroom),
            );
        }
        // Always explicit — an omitted num_ctx inherits the MODEL's Modelfile default
        // (granite4.2: 131072), not the server's. See the GenerateOptions doc.
        options.insert(
            "num_ctx".into(),
            serde_json::json!(if opts.num_ctx > 0 {
                opts.num_ctx
            } else {
                crate::route::resolve_voice_num_ctx(None)
            }),
        );
        let mut messages = Vec::with_capacity(2);
        if let Some(system) = opts.system.as_deref() {
            messages.push(ChatTurn {
                role: "system",
                content: system,
            });
        }
        messages.push(ChatTurn {
            role: "user",
            content: prompt,
        });
        GenerateRequest {
            model: &self.model,
            messages,
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

        // Order-pinned schemas POST through the RawValue wire shape so the property order the
        // contract documents is the order the grammar enforces; everything else keeps the
        // byte-identical `.json(&req)` path it always had.
        let raw_format: Option<Box<serde_json::value::RawValue>> = match &opts.format_schema_raw {
            Some(raw) => Some(
                serde_json::value::RawValue::from_string(raw.clone())
                    .context("format_schema_raw is not valid JSON")?,
            ),
            None => None,
        };

        let url = format!("{}/api/chat", self.base_url);
        let request = self.http.post(&url);
        let request = match &raw_format {
            Some(raw) => {
                let wire = GenerateRequestWire {
                    model: req.model,
                    messages: req
                        .messages
                        .iter()
                        .map(|m| ChatTurn {
                            role: m.role,
                            content: m.content,
                        })
                        .collect(),
                    stream: req.stream,
                    format: Some(raw),
                    think: req.think,
                    options: req.options.clone(),
                };
                let body = serde_json::to_string(&wire).context("serialize wire request")?;
                request
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body)
            }
            None => request.json(&req),
        };
        let resp = request.send().await.context("ollama request")?;

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
                response: parsed.message.content,
                thinking: parsed.message.thinking,
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

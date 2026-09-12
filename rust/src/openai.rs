//! OpenAI-compatible chat-completions inference backend.
//!
//! Currently unused in production, but retained as the backend seam for compatible servers.
//!
//! Targets `POST {base_url}/v1/chat/completions`. Works against any OpenAI-compatible server;
//! oMLX is the one it was measured against.
//!
//! ## Differences from ollama that callers must know
//!
//! 1. `num_ctx` has no equivalent and is dropped; `max_tokens` bounds output only.
//! 2. Schema options are not sent because the measured server's grammar path corrupts output;
//!    fail-closed parsers enforce the wire contracts instead.
//! 3. `think` is not sent. It is an Ollama extension; a reasoning model behind an
//!    OpenAI-compatible server exposes no such switch. Roles that need it must stay on ollama.

use crate::ollama::{GenerateOptions, GenerateResult};
use crate::util::truncate;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct OpenAiClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
    /// Structured refusal body, retained so the reason reaches the dead letter.
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
    #[serde(default)]
    code: String,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    message: ChoiceMessage,
}

#[derive(Deserialize, Default)]
struct ChoiceMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    completion_tokens: i32,
    /// Seconds, oMLX's own measure. Absent on stricter OpenAI servers, hence the wall-clock
    /// fallback in `generate_with_body`.
    #[serde(default)]
    total_time: Option<f64>,
}

impl OpenAiClient {
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
            .context("build http client")?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            http,
        })
    }

    fn build_request<'a>(&'a self, prompt: &'a str, opts: &'a GenerateOptions) -> ChatRequest<'a> {
        let mut messages = Vec::with_capacity(2);
        if let Some(system) = opts.system.as_deref() {
            messages.push(Message {
                role: "system",
                content: system,
            });
        }
        messages.push(Message {
            role: "user",
            content: prompt,
        });
        ChatRequest {
            model: &self.model,
            messages,
            stream: false,
            temperature: opts.temperature,
            // Same rule as the ollama path: omitted when unset, so the server's own default applies.
            max_tokens: (opts.num_predict > 0).then_some(opts.num_predict),
        }
    }

    /// The exact bytes POSTed for `(prompt, opts)`.
    pub(crate) fn wire_body(&self, prompt: &str, opts: &GenerateOptions) -> Result<String> {
        let req = self.build_request(prompt, opts);
        serde_json::to_string(&req).context("serialize openai request")
    }

    pub(crate) async fn generate_with_body(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        let request_body = self.request_body(prompt, opts);
        let wire = self.wire_body(prompt, opts)?;

        let url = format!("{}/v1/chat/completions", self.base_url);
        let started = Instant::now();
        let resp = self
            .http
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(wire)
            .send()
            .await
            .context("openai request")?;

        let status = resp.status();
        let raw = resp.text().await.context("read openai response")?;

        // A refusal carries its reason in the body, and that reason is the useful part — oMLX's
        // `prefill_memory_exceeded` names the guard and the ceiling it hit. Surface it verbatim
        // rather than just the status code.
        if !status.is_success() {
            if let Ok(parsed) = serde_json::from_str::<ChatResponse>(&raw) {
                if let Some(e) = parsed.error {
                    return Err(anyhow!(
                        "openai HTTP {} [{}]: {}",
                        status.as_u16(),
                        e.code,
                        truncate(&e.message, 300)
                    ));
                }
            }
            return Err(anyhow!(
                "openai HTTP {}: {}",
                status.as_u16(),
                truncate(&raw, 300)
            ));
        }

        let parsed: ChatResponse = serde_json::from_str(&raw)
            .with_context(|| format!("decode openai response (body={})", truncate(&raw, 200)))?;
        if let Some(e) = parsed.error {
            return Err(anyhow!("openai error [{}]: {}", e.code, e.message));
        }
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| {
                anyhow!(
                    "openai response had no choices (body={})",
                    truncate(&raw, 200)
                )
            })?;

        // Prefer the server's own timing; fall back to wall clock so `total_duration` is never a
        // silent zero on a server that omits `usage.total_time`.
        let total_duration = parsed
            .usage
            .as_ref()
            .and_then(|u| u.total_time)
            .filter(|s| *s > 0.0)
            .map(Duration::from_secs_f64)
            .unwrap_or_else(|| started.elapsed());

        Ok((
            GenerateResult {
                response: content,
                // The OpenAI-compatible path (oMLX) has no thinking channel; empty by contract.
                thinking: String::new(),
                model: if parsed.model.is_empty() {
                    self.model.clone()
                } else {
                    parsed.model
                },
                total_duration,
                eval_count: parsed.usage.map(|u| u.completion_tokens).unwrap_or(0),
            },
            request_body,
        ))
    }

    /// ping lists models to verify the server is reachable. Cheap — no inference.
    pub async fn ping(&self) -> Result<()> {
        let url = format!("{}/v1/models", self.base_url);
        let resp = self.http.get(&url).send().await.context("openai ping")?;
        if !resp.status().is_success() {
            return Err(anyhow!("openai ping HTTP {}", resp.status().as_u16()));
        }
        Ok(())
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
        serde_json::to_value(self.build_request(prompt, opts)).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> OpenAiClient {
        OpenAiClient::new("http://voice-host:8000/", "ministral-3-14b", Duration::ZERO).unwrap()
    }

    #[test]
    fn schema_options_are_not_forwarded() {
        let opts = GenerateOptions {
            json_mode: true,
            format_schema: Some(serde_json::json!({"type":"object"})),
            format_schema_raw: Some(r#"{"type":"object"}"#.into()),
            ..Default::default()
        };
        let body = client().request_body("x", &opts);
        assert!(
            body.get("response_format").is_none(),
            "response_format leaked: {body}"
        );
        let wire = client().wire_body("x", &opts).unwrap();
        assert!(!wire.contains("response_format"), "wire: {wire}");
    }

    #[test]
    fn base_url_trailing_slash_is_trimmed() {
        assert_eq!(client().base_url, "http://voice-host:8000");
    }

    #[test]
    fn system_prompt_leads_the_message_list() {
        let opts = GenerateOptions {
            system: Some("be terse".into()),
            num_predict: 128,
            temperature: Some(0.2),
            ..Default::default()
        };
        let body = client().request_body("hello", &opts);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "be terse");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(body["max_tokens"], 128);
        assert_eq!(body["temperature"], 0.2);
    }

    /// No system prompt must not produce an empty leading message — some servers reject that.
    #[test]
    fn absent_system_prompt_sends_only_the_user_turn() {
        let body = client().request_body("hello", &GenerateOptions::default());
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
        // num_predict 0 means "server default", so the field must be absent, not 0.
        assert!(body.get("max_tokens").is_none());
    }

    /// num_ctx has no OpenAI equivalent and must not leak onto the wire as some invented field.
    #[test]
    fn num_ctx_is_dropped_not_invented() {
        let opts = GenerateOptions {
            num_ctx: 4096,
            ..Default::default()
        };
        let body = client().request_body("x", &opts);
        let text = serde_json::to_string(&body).unwrap();
        assert!(!text.contains("num_ctx"), "num_ctx leaked: {text}");
        assert!(!text.contains("4096"), "num_ctx value leaked: {text}");
    }
}

//! OpenAI-compatible chat-completions client — the second inference backend.
//!
//! ⚠ **Currently unused in production (2026-08-20):** the Mac lane is retired and every seat
//! runs on archbox's Ollama. This module stays as the proven plug point for any future
//! OpenAI-compatible server (vLLM, a returning MLX host); the notes below record what was
//! measured while it carried the four voices.
//!
//! Added for **oMLX** (`jundot/omlx`, D-T41), the MLX server on the voice host. `route.rs` has
//! carried a `Backend` enum and an `Arc<dyn Inference>` seam since L2 with the note that *"a second
//! impl (vLLM) waits until it is real, not built on speculation."* It is real now, and this is that
//! impl — **not** a protocol shim in front of `:11434`. Keeping the translation here rather than on
//! the wire means `num_ctx`'s fate (below) is visible in code and testable, instead of hidden in a
//! proxy nobody reads.
//!
//! Targets `POST {base_url}/v1/chat/completions`. Works against any OpenAI-compatible server;
//! oMLX is the one it was measured against.
//!
//! ## Four differences from ollama that callers must know
//!
//! 1. ⛔ **`num_ctx` HAS NO EQUIVALENT AND IS DROPPED.** OpenAI's schema has no context-window
//!    field — `max_tokens` bounds OUTPUT only. On oMLX this is not the silent-degradation trap it
//!    would be on llama.cpp: an over-long prompt is **rejected with HTTP 400
//!    `prefill_memory_exceeded`**, and every prompt that IS accepted is evaluated whole (D-T41
//!    verified head-recall at 1,394 / 2,549 / 3,869 / 5,189 / 6,835 tokens). **The failure mode
//!    moves from silently-wrong to loudly-failed**, which is the direction worth having — but it
//!    means an oversized prompt now DIES instead of degrading, so the voice diet (D-T35) is a
//!    precondition of routing a stage here, not an optimisation.
//! 2. ⛔ **GRAMMAR CONSTRAINTS ARE NOT SENT — measured off, not designed off (2026-08-09).**
//!    oMLX 0.5.7's xgrammar path deterministically CORRUPTS constrained output on the tekken
//!    tokenizer family (both Ministral sizes): "basketball" → "basketbal" mid-string, "Utah Jazz"
//!    → "Jazz", strings closed early then whitespace-looped to `max_tokens`. The same model, same
//!    prompt, unconstrained, is byte-perfect — so `response_format` is withheld by default and
//!    every contract rides the fail-closed parser + balanced-brace salvager it already rides.
//!    D-T41's "enforces or errors, no quiet middle" was WRONG: corrupted-but-conforming-looking
//!    output IS the quiet middle, and it would have poisoned the verbatim-containment contracts.
//!    `with_constraint(true)` keeps the grammar wire-path alive (and pinned by tests) for the day
//!    the upstream bug is fixed; `json_object`/`json_schema`/vLLM `structured_outputs` all ride
//!    the same broken compiler today, so there is no partial re-enable worth having.
//! 3. **Property order is preserved deliberately.** `format_schema_raw` exists because §1a makes
//!    property ORDER part of the contract and `serde_json::Value` is BTreeMap-backed, so a schema
//!    that travels as a `Value` arrives alphabetised and the grammar then enforces that accidental
//!    order. The raw string is embedded byte-for-byte here for exactly the same reason it is on the
//!    ollama path.
//! 4. **`think` is not sent.** It is an ollama extension; a reasoning model behind an
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
    /// Whether to send `response_format` (grammar-constrained decoding). False in production:
    /// oMLX 0.5.7's xgrammar path corrupts tekken-tokenizer output (module note §2). The field
    /// and the wire-path tests survive so re-enabling on a fixed server is a one-line change.
    constrain: bool,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

/// The wire request. `response_format` is a `RawValue` so an order-pinned schema survives
/// serialization unsorted (see the module note).
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<Box<serde_json::value::RawValue>>,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
    /// oMLX reports refusals (`prefill_memory_exceeded`, and friends) as a structured body. Parsed
    /// so the reason reaches the dead-letter instead of being flattened to "no choices".
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
            constrain: false,
        })
    }

    /// with_constraint re-enables the `response_format` grammar wire-path — test-and-probe only
    /// until oMLX's tekken corruption (module note §2) is fixed upstream.
    pub fn with_constraint(mut self, constrain: bool) -> Self {
        self.constrain = constrain;
        self
    }

    /// response_format for this call, as raw JSON text.
    ///
    /// Precedence mirrors the ollama path exactly — `format_schema_raw` beats `format_schema`
    /// beats `json_mode` — so a stage cannot get a different output contract merely by being
    /// routed to a different host.
    fn response_format(opts: &GenerateOptions) -> Result<Option<Box<serde_json::value::RawValue>>> {
        let schema_text: Option<String> = match (&opts.format_schema_raw, &opts.format_schema) {
            (Some(raw), _) => Some(raw.clone()),
            (None, Some(v)) => Some(v.to_string()),
            (None, None) => None,
        };
        let text = match schema_text {
            // `strict: true` is what makes this a grammar rather than a suggestion — the
            // equivalent of ollama's `format: <schema>`. oMLX honours it via xgrammar, which is an
            // OPT-IN build flag there (`--with-grammar`); without that build the server accepts the
            // field and ignores the constraint, so a contract stage would silently start emitting
            // free text. Verified present on the voice host before cutover (D-T41).
            Some(schema) => format!(
                r#"{{"type":"json_schema","json_schema":{{"name":"contract","strict":true,"schema":{schema}}}}}"#
            ),
            None if opts.json_mode => r#"{"type":"json_object"}"#.to_string(),
            None => return Ok(None),
        };
        Ok(Some(
            serde_json::value::RawValue::from_string(text)
                .context("response_format is not valid JSON")?,
        ))
    }

    fn build_request<'a>(
        &'a self,
        prompt: &'a str,
        opts: &'a GenerateOptions,
    ) -> Result<ChatRequest<'a>> {
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
        Ok(ChatRequest {
            model: &self.model,
            messages,
            stream: false,
            temperature: opts.temperature,
            // Same rule as the ollama path: omitted when unset, so the server's own default applies.
            max_tokens: (opts.num_predict > 0).then_some(opts.num_predict),
            // Withheld unless `constrain` — the grammar corrupts on this server (module note §2).
            response_format: if self.constrain {
                Self::response_format(opts)?
            } else {
                None
            },
        })
    }

    /// The exact bytes POSTed for `(prompt, opts)`.
    ///
    /// Separate from [`Self::request_body`] on purpose, and the distinction is load-bearing:
    /// `request_body` returns a `serde_json::Value` for the ledger, and `Value` is BTreeMap-backed,
    /// so an order-pinned schema comes back ALPHABETISED there. **These bytes do not** — a
    /// `RawValue` serializes verbatim — so the grammar the server compiles is the authored order
    /// (§1a). The ollama path has exactly the same split for exactly the same reason. A unit test
    /// pins this, because the failure it prevents is silent: the wrong contract, correctly enforced.
    pub(crate) fn wire_body(&self, prompt: &str, opts: &GenerateOptions) -> Result<String> {
        let req = self.build_request(prompt, opts)?;
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
        match self.build_request(prompt, opts) {
            Ok(req) => serde_json::to_value(&req).unwrap_or(serde_json::Value::Null),
            Err(_) => serde_json::Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> OpenAiClient {
        OpenAiClient::new("http://voice-host:8000/", "ministral-3-14b", Duration::ZERO).unwrap()
    }

    /// The grammar wire-path under test — NOT what production sends today.
    fn constrained_client() -> OpenAiClient {
        client().with_constraint(true)
    }

    /// ⛔ The production default: even a schema-carrying request sends NO `response_format` —
    /// oMLX 0.5.7's grammar corrupts tekken output, so the contract rides the fail-closed
    /// parser instead. If this test ever needs changing, the upstream bug had better be fixed.
    #[test]
    fn schema_is_withheld_by_default() {
        let opts = GenerateOptions {
            json_mode: true,
            format_schema: Some(serde_json::json!({"type":"object"})),
            format_schema_raw: Some(r#"{"type":"object"}"#.into()),
            ..Default::default()
        };
        let body = client().request_body("x", &opts);
        assert!(
            body.get("response_format").is_none(),
            "response_format leaked from the default client: {body}"
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

    #[test]
    fn json_mode_maps_to_json_object() {
        let opts = GenerateOptions {
            json_mode: true,
            ..Default::default()
        };
        let body = constrained_client().request_body("x", &opts);
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    /// §1a: property ORDER is the contract. A raw schema must reach THE WIRE in its authored
    /// order — if this ever alphabetises, the grammar enforces the wrong contract silently.
    ///
    /// Asserted against `wire_body` (the POSTed bytes), NOT `request_body`: the latter is the
    /// ledger's `Value` copy and is BTreeMap-backed, so it alphabetises by construction. The
    /// companion test below pins that difference so nobody "fixes" the wrong one.
    #[test]
    fn raw_schema_preserves_property_order_on_the_wire() {
        let opts = GenerateOptions {
            json_mode: true,
            format_schema: Some(serde_json::json!({"type":"object"})),
            format_schema_raw: Some(
                r#"{"type":"object","properties":{"zebra":{"type":"string"},"apple":{"type":"integer"}}}"#
                    .into(),
            ),
            ..Default::default()
        };
        let wire = constrained_client()
            .wire_body("x", &opts)
            .expect("serializes");
        assert!(wire.contains(r#""type":"json_schema""#), "wire: {wire}");
        assert!(wire.contains(r#""strict":true"#), "wire: {wire}");
        let zebra = wire.find("zebra").expect("zebra present");
        let apple = wire.find("apple").expect("apple present");
        assert!(
            zebra < apple,
            "authored property order must survive to the wire, got: {wire}"
        );
    }

    /// The ledger copy alphabetises and that is ACCEPTED, not a bug — it mirrors the ollama path,
    /// where the `Value` form is what the ledger always stored. Pinned so the divergence between
    /// the two surfaces stays deliberate and documented rather than being discovered again.
    #[test]
    fn ledger_body_alphabetises_while_the_wire_does_not() {
        let opts = GenerateOptions {
            format_schema_raw: Some(
                r#"{"type":"object","properties":{"zebra":{"type":"string"},"apple":{"type":"integer"}}}"#
                    .into(),
            ),
            ..Default::default()
        };
        let c = constrained_client();
        let ledger = serde_json::to_string(&c.request_body("x", &opts)).unwrap();
        let wire = c.wire_body("x", &opts).unwrap();
        assert!(
            ledger.find("apple").unwrap() < ledger.find("zebra").unwrap(),
            "ledger Value form is expected to alphabetise: {ledger}"
        );
        assert!(
            wire.find("zebra").unwrap() < wire.find("apple").unwrap(),
            "wire must NOT alphabetise: {wire}"
        );
    }

    /// With no raw form, the `Value` schema is still promoted to a strict grammar rather than
    /// falling back to json_object.
    #[test]
    fn value_schema_is_still_a_strict_grammar() {
        let opts = GenerateOptions {
            json_mode: true,
            format_schema: Some(serde_json::json!({"type":"object"})),
            ..Default::default()
        };
        let body = constrained_client().request_body("x", &opts);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(
            body["response_format"]["json_schema"]["schema"]["type"],
            "object"
        );
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

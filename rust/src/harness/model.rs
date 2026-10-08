//! Model values and transport contract; concrete adapters live in harness/providers.

use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Context window used by local model stages. Roles sharing one loaded runner must use the same
/// size or Ollama reloads it between calls.
pub const LOCAL_STAGE_NUM_CTX: i32 = 4096;

/// Context window shared by every character voice: prompt, evidence, and output reservation.
pub const VOICE_NUM_CTX_PACKET: i32 = 4096;

/// Whether a voice uses the small context envelope. Output reservations and evidence caps key on
/// this effective window.
pub fn small_voice_window(num_ctx: i32) -> bool {
    num_ctx <= VOICE_NUM_CTX_PACKET
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
    /// Input tokens reported by the provider for the complete request.
    pub prompt_eval_count: i32,
    pub eval_count: i32,
    /// Provider termination value after it passes the completion guard.
    pub completion_reason: Option<String>,
    /// Exact successful HTTP response body, retained for read-only diagnostics.
    pub raw_response_body: String,
}

/// Prepared wire request plus exact tokenizer evidence when the provider exposes it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedRequest {
    pub request: serde_json::Value,
    pub coverage: Option<InputCoverage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputCoverage {
    pub rendered_prompt: String,
    pub token_ids: Vec<i32>,
    pub context: i32,
    pub provider: serde_json::Value,
}

impl PreparedRequest {
    pub fn verify_input(&self, prompt: &str, opts: &GenerateOptions) -> Result<()> {
        let prepared = self;
        let coverage = prepared
            .coverage
            .as_ref()
            .context("exact tokenizer evidence required")?;
        ensure!(
            !coverage.token_ids.is_empty() && coverage.token_ids.iter().all(|id| *id >= 0),
            "invalid input token IDs"
        );
        ensure!(
            coverage.rendered_prompt.contains(prompt)
                && opts
                    .system
                    .as_ref()
                    .is_none_or(|system| coverage.rendered_prompt.contains(system)),
            "complete input missing from rendered template"
        );
        ensure!(
            opts.num_ctx > 0 && opts.num_predict > 0 && coverage.context > 0,
            "invalid input/output reservation"
        );
        let reserved = prepared.request["n_predict"]
            .as_u64()
            .or_else(|| prepared.request["options"]["num_predict"].as_u64())
            .or_else(|| prepared.request["max_tokens"].as_u64())
            .context("explicit transport output reservation required")?;
        ensure!(
            reserved >= opts.num_predict as u64,
            "transport reduced the requested output reservation"
        );
        ensure!(coverage.token_ids.len() as u64+reserved <= coverage.context.min(opts.num_ctx) as u64,
        "complete input plus reserved output exceeds actual model context; no source was clipped");
        Ok(())
    }
}

/// Inference — the model-call backend, the genuine swap point. `OllamaClient` is the first
/// impl; a `dyn Inference` is what a plugin-owned route resolves to. `generate` returns the exact
/// wire body it POSTed; `request_body` remains for no-call deterministic builders.
#[async_trait]
pub trait Inference: Send + Sync {
    /// Immutable loaded artifact identity when the provider exposes one. Without it,
    /// consumers retain provenance but must not reuse a mutable model tag's results.
    async fn revision(&self) -> Result<Option<String>> {
        Ok(None)
    }

    /// Providers with native tokenization prepare the exact token-ID wire request here.
    async fn prepare(&self, prompt: &str, opts: &GenerateOptions) -> Result<PreparedRequest> {
        Ok(PreparedRequest {
            request: self.request_body(prompt, opts),
            coverage: None,
        })
    }

    async fn generate_prepared(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
        prepared: &PreparedRequest,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        anyhow::ensure!(
            prepared.request == self.request_body(prompt, opts),
            "prepared model request drift"
        );
        self.generate(prompt, opts).await
    }

    /// generate performs one non-streaming completion. No auto-retry — the work queue owns
    /// backoff (the boundary the host already enforces), and returns the exact
    /// transport request body sent with the result.
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)>;

    /// model returns the concrete model id, for provenance (`model_version`).
    fn model(&self) -> &str;

    /// Local request description. For tokenizing providers, `prepare` returns the final wire body.
    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value;
}

/// A provider reports that the answer is incomplete; Studio may request a bounded rewrite.
#[derive(Debug)]
pub struct IncompleteOutput(pub String);
impl std::fmt::Display for IncompleteOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for IncompleteOutput {}

/// Provider diagnostic wrappers must preserve the bounded completion-rewrite signal.
pub fn is_incomplete_output(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<IncompleteOutput>())
}

/// Failed provider decoding retains the response for generation-attempt receipts.
#[derive(Debug)]
pub struct ResponseFailure {
    pub raw_response_body: String,
    pub error: anyhow::Error,
}
impl std::fmt::Display for ResponseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}
impl std::error::Error for ResponseFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error.as_ref())
    }
}

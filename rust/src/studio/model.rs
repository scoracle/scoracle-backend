//! Model values and transport contract; concrete adapters live in runtime/providers.

use anyhow::Result;
use async_trait::async_trait;
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

/// Inference — the model-call backend, the genuine swap point. `OllamaClient` is the first
/// impl; a `dyn Inference` is what a `Role` resolves to. `generate` returns the exact
/// wire body it POSTed; `request_body` remains for no-call deterministic builders.
#[async_trait]
pub trait Inference: Send + Sync {
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

    /// request_body returns the exact transport request body `generate` would POST for
    /// `(prompt, opts)`.
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

//! Route — the model-call seam: role → concrete model at runtime (Plan §1.1 / §2).
//!
//! A stage names a **role** (the model's JOB), never a model name; the `Router` resolves
//! that role to a concrete backend. This is the *swap seam*: the three swaps the Hardware
//! Roadmap brings — identity (`e4b` → `31B`), topology (one model → two concurrent → one
//! unified fine-tune), backend (Ollama → vLLM) — all land here, and stage code never moves.
//!
//! L0/L1 ships the MINIMAL router: every `Role` resolves to the one configured Gemma, which
//! is enough for vibe to route `EmotionalNews → Gemma` byte-identically. L2 replaces the
//! single backend with a per-role map built from `COGNITION_ROUTE_*` plus the A/B
//! `candidate_for` challenger (the eval discipline) — `for_role`'s contract does not change,
//! so that swap won't move a stage either.
//!
//! `Inference` is the one real trait under Route: the model backend. `OllamaClient` is its
//! first (today, only) impl; a second impl (vLLM) waits until it is real, not built on
//! speculation. The trait's three methods are exactly the inherent methods `OllamaClient`
//! already exposes, so the impl is a thin delegation and the wire body stays single-sourced.

use crate::ollama::{GenerateOptions, GenerateResult, OllamaClient};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Role names a model's JOB, not its name. Stages address a `Role`; the `Router` maps it to
/// a concrete model. The one place a model id may appear is the router config (L2) — never
/// in stage code. `StatsLogic` (rating/sigil reasoning), `EmotionalNews` (vibe/narratives),
/// `Multilang` (HORIZON normalize), `Sql` (SQLCoder). Derives `Hash` for the L2 role→model map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    StatsLogic,
    EmotionalNews,
    Multilang,
    Sql,
}

impl Role {
    /// as_str is the stable telemetry label for the role (it subsumes Go's
    /// `GenerateOptions.Op` — the role *is* the op label).
    pub fn as_str(self) -> &'static str {
        match self {
            Role::StatsLogic => "stats-logic",
            Role::EmotionalNews => "emotional-news",
            Role::Multilang => "multilang",
            Role::Sql => "sql",
        }
    }
}

/// Inference — the model-call backend, the genuine swap point. `OllamaClient` is the first
/// impl; a `dyn Inference` is what a `Role` resolves to. The three methods mirror the
/// inherent `OllamaClient` API so `request_body` stays the single source of truth for the
/// wire payload (the property the temp-0 parity proof leans on — the recorded body can never
/// drift from the sent one).
#[async_trait]
pub trait Inference: Send + Sync {
    /// generate performs one non-streaming completion. No auto-retry — the work queue owns
    /// backoff (the boundary the host already enforces).
    async fn generate(&self, prompt: &str, opts: &GenerateOptions) -> Result<GenerateResult>;

    /// model returns the concrete model id, for provenance (`model_version`).
    fn model(&self) -> &str;

    /// request_body returns the exact `/api/generate` body `generate` would POST for
    /// `(prompt, opts)` — recorded for the parity diff.
    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value;
}

#[async_trait]
impl Inference for OllamaClient {
    async fn generate(&self, prompt: &str, opts: &GenerateOptions) -> Result<GenerateResult> {
        // Inherent method wins method resolution, but qualify it explicitly to make the
        // delegation unambiguous (no accidental recursion into the trait method).
        OllamaClient::generate(self, prompt, opts).await
    }

    fn model(&self) -> &str {
        OllamaClient::model(self)
    }

    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
        OllamaClient::request_body(self, prompt, opts)
    }
}

/// Router maps `Role` → concrete model at runtime — the Route primitive.
///
/// L1 minimal: every role resolves to one backend (the configured Gemma). L2 replaces this
/// with a per-role map built from `COGNITION_ROUTE_*` (`from_config`) plus the A/B challenger
/// (`candidate_for`); `for_role` keeps the same shape, so stage code never moves across that
/// swap. See Plan §2.
pub struct Router {
    /// L1: every `Role` resolves to this single backend. L2 makes this a `HashMap<Role, …>`.
    default_backend: Arc<dyn Inference>,
}

impl Router {
    /// single wires every role to one backend — the L1 router (every `Role` → the one
    /// configured Gemma). Enough for vibe to route `EmotionalNews → Gemma` byte-identically.
    pub fn single(backend: Arc<dyn Inference>) -> Self {
        Self {
            default_backend: backend,
        }
    }

    /// for_role resolves a role to the concrete model backing it — the incumbent a stage
    /// uses. L1 always returns the single backend; this is the one place a role becomes a
    /// concrete model, so stage code never names one.
    pub fn for_role(&self, _role: Role) -> Arc<dyn Inference> {
        Arc::clone(&self.default_backend)
    }
}

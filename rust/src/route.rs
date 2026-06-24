//! Route — the model-call seam: role → concrete model at runtime (Plan §1.1 / §2).
//!
//! A stage names a **role** (the model's JOB), never a model name; the `Router` resolves
//! that role to a concrete backend. This is the *swap seam*: the three swaps the Hardware
//! Roadmap brings — identity (`e4b` → `31B`), topology (one model → two concurrent → one
//! unified fine-tune), backend (Ollama → vLLM) — all land here, and stage code never moves.
//!
//! L2 ships the config-driven router: `Router::from_config` builds the per-role map from
//! [`RouteConfig`] (the `COGNITION_ROUTE_*` table), one `Arc<dyn Inference>` per DISTINCT
//! model (so roles sharing a model share a backend), plus the optional A/B `candidate_for`
//! challenger. With nothing configured every role resolves to the one Gemma, so this moved
//! ZERO bytes vs the L1 single router — `for_role`'s contract is unchanged, which is why the
//! identity/topology/backend swaps (Plan §2.1) never move a stage.
//!
//! `Inference` is the one real trait under Route: the model backend. `OllamaClient` is its
//! first (today, only) impl; a second impl (vLLM) waits until it is real, not built on
//! speculation. The trait's three methods are exactly the inherent methods `OllamaClient`
//! already exposes, so the impl is a thin delegation and the wire body stays single-sourced.

use crate::config::{Backend, ModelSpec, RouteConfig};
use crate::ollama::{GenerateOptions, GenerateResult, OllamaClient};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

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
    /// all is every role, so config and router can populate the full map — keeping
    /// `Router::for_role` total (a role always resolves to a model).
    pub fn all() -> [Role; 4] {
        [
            Role::StatsLogic,
            Role::EmotionalNews,
            Role::Multilang,
            Role::Sql,
        ]
    }

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

    /// env_suffix is the `COGNITION_ROUTE_<SUFFIX>` env-key tail naming this role's model —
    /// the role's stable *config* identity (UPPER_SNAKE), distinct from `as_str` (the kebab
    /// telemetry label). The one mapping from a role to where its model id is configured.
    pub fn env_suffix(self) -> &'static str {
        match self {
            Role::StatsLogic => "STATS_LOGIC",
            Role::EmotionalNews => "EMOTIONAL_NEWS",
            Role::Multilang => "MULTILANG",
            Role::Sql => "SQL",
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

/// Router maps `Role` → concrete model at runtime — the Route primitive (Plan §2).
///
/// Built by `from_config` from the [`RouteConfig`] table: `incumbents` is what every role
/// resolves to (`for_role`); `candidates` is the optional A/B challenger per role
/// (`candidate_for`, eval-only). Roles that resolve to the same model share ONE backend Arc.
/// `for_role` keeps the same shape across every config swap (identity/topology/backend, Plan
/// §2.1), which is why a model change never moves stage code — it is a config line + an eval
/// win, never an edit.
pub struct Router {
    /// The incumbent backend each role resolves to. Populated for every `Role` (the config
    /// covers `Role::all`), so `for_role` is total.
    incumbents: HashMap<Role, Arc<dyn Inference>>,
    /// The optional A/B challenger per role — present only where a `*_CANDIDATE` was
    /// configured. NEVER served; read only by `bin/eval` via `candidate_for`.
    candidates: HashMap<Role, Arc<dyn Inference>>,
}

impl Router {
    /// from_config builds the router from the `COGNITION_ROUTE_*` table: one
    /// `Arc<dyn Inference>` per DISTINCT (backend, model, base_url) — so the all-Gemma default
    /// builds exactly one backend shared by every role (byte-identical to the L1 single
    /// router) — wired to each role's incumbent, plus any configured A/B challenger. `timeout`
    /// is the shared per-call budget (`OLLAMA_TIMEOUT_SECONDS`); per-backend timeouts move
    /// into `ModelSpec` when topology splits (HORIZON).
    pub fn from_config(cfg: &RouteConfig, timeout: Duration) -> Result<Self> {
        // Cache keyed by the spec's identity, so two roles naming the same model get the same
        // backend Arc rather than two clients hammering one Ollama.
        let mut built: HashMap<String, Arc<dyn Inference>> = HashMap::new();
        let mut incumbents = HashMap::with_capacity(cfg.roles.len());
        for (role, spec) in &cfg.roles {
            incumbents.insert(*role, build_backend(&mut built, spec, timeout)?);
        }
        let mut candidates = HashMap::with_capacity(cfg.candidates.len());
        for (role, spec) in &cfg.candidates {
            candidates.insert(*role, build_backend(&mut built, spec, timeout)?);
        }
        Ok(Self {
            incumbents,
            candidates,
        })
    }

    /// for_role resolves a role to the incumbent model backing it — the one a stage uses, and
    /// the one place a role becomes a concrete model (stage code never names one). Total by
    /// construction: `from_config` populates every role, so the lookup cannot miss.
    pub fn for_role(&self, role: Role) -> Arc<dyn Inference> {
        Arc::clone(self.incumbents.get(&role).unwrap_or_else(|| {
            unreachable!("RouteConfig::from_env populates every Role::all, so for_role is total")
        }))
    }

    /// candidate_for returns the optional A/B challenger for a role — the backend `bin/eval`
    /// scores against the incumbent. `None` unless `COGNITION_ROUTE_<ROLE>_CANDIDATE` is set.
    /// The router NEVER routes serving traffic here; adoption is a human editing the config on
    /// a measured win (Plan §2.2).
    pub fn candidate_for(&self, role: Role) -> Option<Arc<dyn Inference>> {
        self.candidates.get(&role).map(Arc::clone)
    }
}

/// build_backend returns the `Arc<dyn Inference>` for a spec, constructing one per distinct
/// (backend, model, base_url) and reusing it across roles. The `match` on `spec.backend` is
/// where a new backend (vLLM) plugs in — one arm, alongside its new `impl Inference`.
fn build_backend(
    built: &mut HashMap<String, Arc<dyn Inference>>,
    spec: &ModelSpec,
    timeout: Duration,
) -> Result<Arc<dyn Inference>> {
    let key = format!("{:?}|{}|{}", spec.backend, spec.base_url, spec.model);
    if let Some(existing) = built.get(&key) {
        return Ok(Arc::clone(existing));
    }
    let backend: Arc<dyn Inference> = match spec.backend {
        Backend::Ollama => Arc::new(
            OllamaClient::new(&spec.base_url, &spec.model, timeout)
                .with_context(|| format!("build ollama backend for {}", spec.model))?,
        ),
    };
    built.insert(key, Arc::clone(&backend));
    Ok(backend)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(model: &str) -> ModelSpec {
        ModelSpec {
            backend: Backend::Ollama,
            model: model.to_string(),
            base_url: "http://localhost:11434".to_string(),
        }
    }

    // OllamaClient::new only builds a reqwest client (no network), so from_config is testable
    // offline; these lock the L2 invariants without an env var (which would race other tests).

    #[test]
    fn shares_one_backend_per_distinct_model() {
        let mut roles = HashMap::new();
        roles.insert(Role::EmotionalNews, spec("gemma4:e4b"));
        roles.insert(Role::StatsLogic, spec("gemma4:e4b")); // same model → shared Arc
        roles.insert(Role::Sql, spec("sqlcoder:7b")); // distinct → its own Arc
        let cfg = RouteConfig {
            roles,
            candidates: HashMap::new(),
        };
        let router = Router::from_config(&cfg, Duration::from_secs(60)).unwrap();

        assert!(Arc::ptr_eq(
            &router.for_role(Role::EmotionalNews),
            &router.for_role(Role::StatsLogic),
        ));
        assert!(!Arc::ptr_eq(
            &router.for_role(Role::EmotionalNews),
            &router.for_role(Role::Sql),
        ));
        assert_eq!(router.for_role(Role::EmotionalNews).model(), "gemma4:e4b");
        assert_eq!(router.for_role(Role::Sql).model(), "sqlcoder:7b");
    }

    #[test]
    fn candidate_for_is_none_without_a_challenger() {
        let roles = Role::all().into_iter().map(|r| (r, spec("gemma4:e4b"))).collect();
        let router = Router::from_config(
            &RouteConfig {
                roles,
                candidates: HashMap::new(),
            },
            Duration::from_secs(60),
        )
        .unwrap();
        assert!(router.candidate_for(Role::EmotionalNews).is_none());
    }

    #[test]
    fn candidate_for_resolves_a_configured_challenger() {
        let roles = Role::all().into_iter().map(|r| (r, spec("gemma4:e4b"))).collect();
        let mut candidates = HashMap::new();
        candidates.insert(Role::EmotionalNews, spec("mistral:7b"));
        let router =
            Router::from_config(&RouteConfig { roles, candidates }, Duration::from_secs(60))
                .unwrap();
        assert_eq!(
            router.candidate_for(Role::EmotionalNews).unwrap().model(),
            "mistral:7b"
        );
        assert!(router.candidate_for(Role::StatsLogic).is_none()); // only EmotionalNews has one
    }
}

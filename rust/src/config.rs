//! Environment configuration. Variable names mirror the Go backend
//! (`go/internal/config/config.go`) so the Rust Cognition Harness and the Go API read
//! the same `.env.local`. DB URL precedence matches Go: DATABASE_PRIVATE_URL
//! wins over DATABASE_URL.

use crate::embed::Pooling;
use crate::route::Role;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub db_max_conns: u32,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub ollama_timeout: Duration,
    /// Periodic drain even without a NOTIFY (Go worker default: 30s).
    pub safety_net: Duration,
    /// A 'running' row idle longer than this is recovered to 'pending'. Aligned with
    /// the Go `derive.StaleLease` (30 min) so the Rust Cognition Harness and the Go drainer
    /// agree on what counts as a crashed lease when they share the queue — longer than
    /// any single item's processing budget, so a slow-but-alive worker is never stolen.
    pub stale_lease: Duration,
    /// Role → model map (the Route primitive's config, Plan §2.1). Every role defaults to
    /// `ollama_model` on `ollama_base_url`, so an un-configured deploy is all-Gemma and
    /// byte-identical to the L1 single router; `COGNITION_ROUTE_*` overrides per role.
    pub route: RouteConfig,
    /// Embedding-model config (the Embed primitive, Plan §1.4) — `COGNITION_EMBED_*`. Read by
    /// the experiment harness and (once the hybrid Resolve gate lands) the service; the model
    /// is named here, never in stage code (the same boundary the router holds for generation).
    pub embed: EmbedConfig,
    /// Embedding-Resolve hybrid policy (the Plan §1.3 gate) — `COGNITION_RESOLVE_*`. The cosine
    /// bands that auto-decide a candidate without a model call; the ambiguous middle goes to the
    /// model. Defaults are the conservative band the L4 experiment measured (AUC 0.88).
    pub resolve: ResolveConfig,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = env_opt("DATABASE_PRIVATE_URL")
            .or_else(|| env_opt("DATABASE_URL"))
            .ok_or_else(|| anyhow!("DATABASE_PRIVATE_URL or DATABASE_URL must be set"))?;

        // Bound as locals: they are both their own `Config` fields AND the per-role defaults
        // the route map falls back to (so an un-configured deploy resolves every role to the
        // one Ollama model — the byte-identical-to-L1 default).
        let ollama_base_url = env_or("OLLAMA_BASE_URL", "http://localhost:11434");
        let ollama_model = env_or("OLLAMA_MODEL", "mistral:7b");
        let route = RouteConfig::from_env(&ollama_model, &ollama_base_url);

        Ok(Self {
            database_url,
            db_max_conns: env_int("COGNITION_DB_MAX_CONNS", 5) as u32,
            ollama_base_url,
            ollama_model,
            ollama_timeout: Duration::from_secs(env_int("OLLAMA_TIMEOUT_SECONDS", 60) as u64),
            safety_net: Duration::from_secs(env_int("COGNITION_SAFETY_NET_SECONDS", 30) as u64),
            // 1800s = 30 min = Go derive.StaleLease.
            stale_lease: Duration::from_secs(env_int("COGNITION_STALE_LEASE_SECONDS", 1800) as u64),
            route,
            embed: EmbedConfig::from_env(),
            resolve: ResolveConfig::from_env(),
        })
    }
}

/// EmbedConfig names the embedding model the Embed primitive loads (Plan §1.4). The default is
/// BGE-small-en-v1.5 (BERT-arch, strong English, fast on CPU) with its correct `Cls` pooling;
/// `nomic-embed-text` is the multilingual upgrade (it also unlocks the §1.5 Multilang HORIZON),
/// swapped via `COGNITION_EMBED_MODEL` + `COGNITION_EMBED_POOLING=mean` — config, never code.
#[derive(Clone, Debug)]
pub struct EmbedConfig {
    /// HF repo id, e.g. `BAAI/bge-small-en-v1.5`.
    pub model_repo: String,
    /// Git revision / branch to pin (`main` by default).
    pub revision: String,
    /// The model's pooling (BGE → `Cls`; MiniLM/nomic → `Mean`).
    pub pooling: Pooling,
    /// Truncate inputs to this many tokens (a news title+blurb is short; bounds CPU cost).
    pub max_tokens: usize,
}

impl EmbedConfig {
    /// from_env reads `COGNITION_EMBED_*`, defaulting to BGE-small-en-v1.5 / cls / 256 tokens.
    pub fn from_env() -> Self {
        Self {
            model_repo: env_or("COGNITION_EMBED_MODEL", "BAAI/bge-small-en-v1.5"),
            revision: env_or("COGNITION_EMBED_REVISION", "main"),
            pooling: Pooling::from_str_or_cls(&env_or("COGNITION_EMBED_POOLING", "cls")),
            max_tokens: env_int("COGNITION_EMBED_MAX_TOKENS", 256) as usize,
        }
    }
}

/// ResolveConfig is the embedding-Resolve hybrid's cosine bands (Plan §1.3). A candidate whose
/// article↔identity cosine is `≥ keep_threshold` is auto-kept and one `< drop_threshold` is
/// auto-dropped — both WITHOUT a model call (the cheap CPU pre-filter). The `[drop, keep)` middle
/// is the ambiguous band the model adjudicates. Defaults are the conservative band the L4
/// experiment measured on the live vetted-label set (AUC 0.88): keep 0.75 (≈97% agree with Gemma),
/// drop 0.60 (≈0% genuine links lost) → Gemma runs on ≈45% of secondary links (≈55% GPU saved).
#[derive(Clone, Debug)]
pub struct ResolveConfig {
    /// cosine ≥ this → auto-keep (no model call).
    pub keep_threshold: f32,
    /// cosine < this → auto-drop (no model call). Should be ≤ `keep_threshold`.
    pub drop_threshold: f32,
}

impl Default for ResolveConfig {
    fn default() -> Self {
        Self {
            keep_threshold: 0.75,
            drop_threshold: 0.60,
        }
    }
}

impl ResolveConfig {
    /// from_env reads `COGNITION_RESOLVE_{KEEP,DROP}_THRESHOLD`, defaulting to the measured
    /// conservative band. A wider band (raise keep / lower drop) sends more to the model
    /// (safer, less savings); a narrower band saves more GPU at some precision cost.
    pub fn from_env() -> Self {
        let d = Self::default();
        Self {
            keep_threshold: env_float("COGNITION_RESOLVE_KEEP_THRESHOLD", d.keep_threshold),
            drop_threshold: env_float("COGNITION_RESOLVE_DROP_THRESHOLD", d.drop_threshold),
        }
    }
}

/// Backend selects which `impl Inference` a [`ModelSpec`] constructs (Plan §2.1). Ollama is
/// the only backend built today; vLLM lands as a second variant + a second `impl Inference`
/// when it is real, not on speculation — at which point this enum and the match in
/// `Router::from_config` each grow one arm. It is the *committed shape* of the backend swap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Ollama,
}

/// ModelSpec is the concrete model a [`Role`] resolves to — and the ONE place a model id may
/// appear (Plan §1.1 boundary; stage code names a `Role`, never this). `backend` selects the
/// impl, `model` is the concrete id (`mistral:7b`), `base_url` is where that backend lives —
/// a role on a second GPU/port is simply a different `base_url` (the topology swap, Plan §2.1).
#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub backend: Backend,
    pub model: String,
    pub base_url: String,
}

/// RouteConfig is the role → model map driving the [`Router`](crate::route::Router) (Plan §2.1).
/// `roles` is the incumbent each `Role` resolves to; `candidates` is the optional A/B
/// challenger per role (eval-only, NEVER served — Plan §2.2). Built from `COGNITION_ROUTE_*`
/// with every role defaulting to the one Ollama model, so an un-configured deploy is all-Gemma
/// and byte-identical to the L1 single router.
#[derive(Clone, Debug)]
pub struct RouteConfig {
    /// The incumbent model each role resolves to (`for_role`). Populated for EVERY role
    /// (`Role::all`), so the router's `for_role` is total — a role always resolves.
    pub roles: HashMap<Role, ModelSpec>,
    /// The optional A/B challenger per role (`candidate_for`) — present only when
    /// `COGNITION_ROUTE_<ROLE>_CANDIDATE` is set. Run by `bin/eval` against the incumbent;
    /// adoption is a human editing `COGNITION_ROUTE_<ROLE>`, never an auto-promote.
    pub candidates: HashMap<Role, ModelSpec>,
}

impl RouteConfig {
    /// from_env reads `COGNITION_ROUTE_<ROLE>` for every role (e.g.
    /// `COGNITION_ROUTE_EMOTIONAL_NEWS`), each defaulting to `default_model` on `base_url` —
    /// so with nothing configured every role is the one Gemma and routing moves zero bytes
    /// vs L1. `COGNITION_ROUTE_<ROLE>_CANDIDATE` adds the optional eval challenger. Today
    /// every backend is Ollama on the shared `base_url`; per-role backend/base_url overrides
    /// are the topology/backend swaps (HORIZON — Plan §2.1), added when they are real.
    pub fn from_env(default_model: &str, base_url: &str) -> Self {
        let mut roles = HashMap::new();
        let mut candidates = HashMap::new();
        for role in Role::all() {
            let key = format!("COGNITION_ROUTE_{}", role.env_suffix());
            roles.insert(
                role,
                ModelSpec {
                    backend: Backend::Ollama,
                    model: env_or(&key, default_model),
                    base_url: base_url.to_string(),
                },
            );
            if let Some(candidate_model) = env_opt(&format!("{key}_CANDIDATE")) {
                candidates.insert(
                    role,
                    ModelSpec {
                        backend: Backend::Ollama,
                        model: candidate_model,
                        base_url: base_url.to_string(),
                    },
                );
            }
        }
        Self { roles, candidates }
    }
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn env_or(key: &str, default: &str) -> String {
    env_opt(key).unwrap_or_else(|| default.to_string())
}

fn env_int(key: &str, default: i64) -> i64 {
    env_opt(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_float(key: &str, default: f32) -> f32 {
    env_opt(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

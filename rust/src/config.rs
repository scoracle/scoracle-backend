//! Environment configuration. Variable names mirror the Go backend
//! (`go/internal/config/config.go`) so the Rust Cognition Harness and the Go API read
//! the same `.env.local`. DB URL precedence matches Go: DATABASE_PRIVATE_URL
//! wins over DATABASE_URL.

use crate::embed::Pooling;
use crate::route::Role;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub db_max_conns: u32,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub ollama_timeout: Duration,
    /// The GPU governor — the max concurrent model calls the Router permits across ALL roles
    /// (one shared semaphore, since there is one GPU). Reads `OLLAMA_MAX_CONCURRENT`, the SAME
    /// var the Go worker's model gate reads, so Go derive and the Rust worker agree on the box's
    /// concurrency budget during a transition overlap. Default 1 (the single-GPU governor); the
    /// worker's sequential drain is an implicit 1, so this only bites under future parallelism or
    /// a brief Go+Rust overlap. Clamped to ≥1 (0 would dead-lock every call).
    pub ollama_max_concurrent: usize,
    /// Periodic drain even without a NOTIFY (Go worker default: 30s).
    pub safety_net: Duration,
    /// A 'running' row idle longer than this is recovered to 'pending'. Aligned with
    /// the Go `derive.StaleLease` (30 min) so the Rust Cognition Harness and the Go drainer
    /// agree on what counts as a crashed lease when they share the queue — longer than
    /// any single item's processing budget, so a slow-but-alive worker is never stolen.
    pub stale_lease: Duration,
    /// Role → model map (the Route primitive's config, Plan §2.1). Every role defaults to
    /// `ollama_model` on `ollama_base_url`, so an un-configured deploy is single-local-model;
    /// `COGNITION_ROUTE_*` overrides per role.
    pub route: RouteConfig,
    /// Embedding-model config (the Embed primitive, Plan §1.4) — `COGNITION_EMBED_*`. Read by
    /// the experiment harness and (once the hybrid Resolve gate lands) the service; the model
    /// is named here, never in stage code (the same boundary the router holds for generation).
    pub embed: EmbedConfig,
    /// Embedding-Resolve hybrid policy (the Plan §1.3 gate) — `COGNITION_RESOLVE_*`. The cosine
    /// bands that auto-decide a candidate without a model call; the ambiguous middle goes to the
    /// model. Defaults are the conservative band the L4 experiment measured (AUC 0.88).
    pub resolve: ResolveConfig,
    /// Scrub article bucket policy (plan F2). The scrub model emits a bucket when it is already
    /// called; this config governs the candle fallback.
    pub scrub: ScrubConfig,
    /// Per-item ceiling on one stage handler run. A wedged await inside a handler (model
    /// call, DB acquire, embed) fails the item after this long instead of stalling the
    /// drain forever (2026-07-15 incident follow-up). Zero disables.
    pub handler_timeout: Duration,
    /// The worker supervisor's no-progress threshold: a busy drain whose heartbeat is
    /// older than this is declared wedged and the process exits for a clean systemd
    /// restart (`Restart=always`). Must exceed the longest legitimately beat-free
    /// stretch of a single stage handler. Zero disables.
    pub watchdog: Duration,
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
            db_max_conns: env_u32("COGNITION_DB_MAX_CONNS", 5)?,
            ollama_base_url,
            ollama_model,
            ollama_timeout: Duration::from_secs(env_u64("OLLAMA_TIMEOUT_SECONDS", 60)?),
            // ≥1: a 0-permit semaphore would block every model call forever.
            ollama_max_concurrent: env_usize("OLLAMA_MAX_CONCURRENT", 1)?.max(1),
            safety_net: Duration::from_secs(env_u64("COGNITION_SAFETY_NET_SECONDS", 30)?),
            // 1800s = 30 min = Go derive.StaleLease.
            stale_lease: Duration::from_secs(env_u64("COGNITION_STALE_LEASE_SECONDS", 1800)?),
            route,
            embed: EmbedConfig::from_env()?,
            resolve: ResolveConfig::from_env()?,
            scrub: ScrubConfig::from_env()?,
            // 900s = 15 min: generous over the slowest observed item (a narratives batch
            // item ran ~4-7 min under the 07-15 catch-up load) yet far under stale-lease.
            handler_timeout: Duration::from_secs(env_u64(
                "COGNITION_HANDLER_TIMEOUT_SECONDS",
                900,
            )?),
            // 2700s = 45 min: generous over the slowest single handler; a wedge self-heals
            // in ≤45 min instead of the incident's 34 hours.
            watchdog: Duration::from_secs(env_u64("COGNITION_WATCHDOG_SECONDS", 2700)?),
        })
    }
}

/// ScrubConfig drives the scrub source-aware novelty gate (`COGNITION_NOVELTY_*`). After the
/// relevance gate keeps an article's entities, the novelty pass compares the article against recent
/// CANONICAL coverage of those same entities and — FIRST-SEEN wins — suppresses a near-duplicate
/// that is either the SAME outlet reposting or a near-VERBATIM syndication, while letting genuine
/// cross-outlet corroboration pass through (every distinct source counted). The retired
/// `narratives::dedup_corpus` collapsed source-BLIND and destroyed that corroboration signal; this
/// gate is the corrected version, run at the tip of the spear so a widened net stays clean.
#[derive(Clone, Debug)]
pub struct ScrubConfig {
    /// Article↔article cosine at/above which two pieces are "the same story" (the near-dup line).
    /// Mirrors the retired narratives `DEDUP_THRESHOLD` (0.85) — the same measured collapse point.
    pub novelty_cosine: f32,
    /// Token-Jaccard at/above which two articles are near-VERBATIM (syndicated wire copy) — which
    /// suppresses even ACROSS outlets. Below it, a different outlet on the same story is treated as
    /// independent corroboration and passes through.
    pub verbatim_jaccard: f32,
    /// How far back to look for a canonical original to dedup against. Reposts/syndication are
    /// recent; the default matches the narratives news lookback (72h) so anything still inside a
    /// corpus window can be collapsed at the gate.
    pub novelty_lookback: Duration,
}

impl Default for ScrubConfig {
    fn default() -> Self {
        Self {
            novelty_cosine: 0.85,
            verbatim_jaccard: 0.90,
            novelty_lookback: Duration::from_secs(259_200),
        }
    }
}

impl ScrubConfig {
    /// from_env reads `COGNITION_NOVELTY_{COSINE,VERBATIM_JACCARD,LOOKBACK_SECONDS}`, defaulting to
    /// the tuned band (0.85 / 0.90 / 72h). Raising `novelty_cosine` or `verbatim_jaccard` suppresses
    /// LESS (more pass-through, the safe direction); lowering them suppresses more aggressively.
    pub fn from_env() -> Result<Self> {
        let d = Self::default();
        Ok(Self {
            novelty_cosine: env_f32("COGNITION_NOVELTY_COSINE", d.novelty_cosine)?,
            verbatim_jaccard: env_f32("COGNITION_NOVELTY_VERBATIM_JACCARD", d.verbatim_jaccard)?,
            novelty_lookback: Duration::from_secs(env_u64(
                "COGNITION_NOVELTY_LOOKBACK_SECONDS",
                d.novelty_lookback.as_secs(),
            )?),
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
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            model_repo: env_or("COGNITION_EMBED_MODEL", "BAAI/bge-small-en-v1.5"),
            revision: env_or("COGNITION_EMBED_REVISION", "main"),
            pooling: Pooling::from_str_or_cls(&env_or("COGNITION_EMBED_POOLING", "cls")),
            max_tokens: env_usize("COGNITION_EMBED_MAX_TOKENS", 256)?,
        })
    }
}

/// ResolveConfig is the embedding-Resolve hybrid's cosine bands (Plan §1.3). The live resolve
/// gate is ASYMMETRIC and uses only the keep line: a candidate whose article↔identity cosine is
/// `≥ keep_threshold` is auto-kept WITHOUT a model call (the cheap CPU pre-filter); everything
/// below goes to the local model — the proxy never auto-drops. `drop_threshold` is retained only for
/// the offline banding analysis (the shadow/experiment bins); the live resolve gate and — since the
/// n9 candle rework retired the per-article relevance tags — narratives both ignore it. Defaults are
/// the conservative band the L4 experiment measured on
/// the live vetted-label set (AUC 0.88): keep 0.75 (high agreement with the model adjudicator),
/// drop 0.60 (near-zero genuine links lost in the shadow set).
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
    /// conservative band. Raising keep sends more candidates to the model (safer, less
    /// savings); lowering it saves more GPU at some precision cost. The drop line only moves
    /// narratives' relevance banding, never the resolve gate.
    pub fn from_env() -> Result<Self> {
        let d = Self::default();
        Ok(Self {
            keep_threshold: env_f32("COGNITION_RESOLVE_KEEP_THRESHOLD", d.keep_threshold)?,
            drop_threshold: env_f32("COGNITION_RESOLVE_DROP_THRESHOLD", d.drop_threshold)?,
        })
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
    /// Per-ROLE think preference (`COGNITION_ROUTE_<ROLE>_THINK`, and `..._CANDIDATE_THINK`
    /// for the A/B challenger): `Some(false)` disables a reasoning model's thinking for this
    /// role's calls. Role-keyed, not model-keyed — the same model may think for one role and
    /// not another (PEAK keeps thinking at 22/22; sigil's 512-token budget requires no-think).
    pub think: Option<bool>,
}

/// RouteConfig is the role → model map driving the [`Router`](crate::route::Router) (Plan §2.1).
/// `roles` is the incumbent each `Role` resolves to; `candidates` is the optional A/B
/// challenger per role (eval-only, NEVER served — Plan §2.2). Built from `COGNITION_ROUTE_*`
/// with every role defaulting to the one Ollama model, so an un-configured deploy is
/// single-local-model and byte-identical to the L1 single router.
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
    /// so with nothing configured every role is the one local model and routing moves zero bytes
    /// vs L1. `COGNITION_ROUTE_<ROLE>_CANDIDATE` adds the optional eval challenger. Today
    /// every backend is Ollama on the shared `base_url`; per-role backend/base_url overrides
    /// are the topology/backend swaps (HORIZON — Plan §2.1), added when they are real.
    pub fn from_env(default_model: &str, base_url: &str) -> Self {
        let mut roles = HashMap::new();
        let mut candidates = HashMap::new();
        let parse_think = |key: &str| -> Option<bool> {
            env_opt(key).and_then(|v| match v.to_lowercase().as_str() {
                "false" | "0" | "no" => Some(false),
                "true" | "1" | "yes" => Some(true),
                _ => None,
            })
        };
        for role in Role::all() {
            let key = format!("COGNITION_ROUTE_{}", role.env_suffix());
            roles.insert(
                role,
                ModelSpec {
                    backend: Backend::Ollama,
                    model: env_or(&key, default_model),
                    base_url: base_url.to_string(),
                    think: parse_think(&format!("{key}_THINK")),
                },
            );
            if let Some(candidate_model) = env_opt(&format!("{key}_CANDIDATE")) {
                candidates.insert(
                    role,
                    ModelSpec {
                        backend: Backend::Ollama,
                        model: candidate_model,
                        base_url: base_url.to_string(),
                        think: parse_think(&format!("{key}_CANDIDATE_THINK")),
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

fn env_u32(key: &str, default: u32) -> Result<u32> {
    let Some(raw) = env_opt(key) else {
        return Ok(default);
    };
    raw.parse::<u32>()
        .with_context(|| format!("{key} must be an unsigned 32-bit integer, got {raw:?}"))
}

fn env_u64(key: &str, default: u64) -> Result<u64> {
    let Some(raw) = env_opt(key) else {
        return Ok(default);
    };
    raw.parse::<u64>()
        .with_context(|| format!("{key} must be an unsigned integer, got {raw:?}"))
}

fn env_usize(key: &str, default: usize) -> Result<usize> {
    let Some(raw) = env_opt(key) else {
        return Ok(default);
    };
    raw.parse::<usize>()
        .with_context(|| format!("{key} must be an unsigned integer, got {raw:?}"))
}

fn env_f32(key: &str, default: f32) -> Result<f32> {
    let Some(raw) = env_opt(key) else {
        return Ok(default);
    };
    let value = raw
        .parse::<f32>()
        .with_context(|| format!("{key} must be a finite float, got {raw:?}"))?;
    if !value.is_finite() {
        anyhow::bail!("{key} must be a finite float, got {raw:?}");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_u32_rejects_invalid_numeric_value() {
        let key = "__SCORACLE_TEST_BAD_U32";
        std::env::set_var(key, "five");
        let err = env_u32(key, 5).unwrap_err();
        std::env::remove_var(key);
        assert!(format!("{err:#}").contains(key));
    }

    #[test]
    fn env_u64_rejects_negative_value() {
        let key = "__SCORACLE_TEST_BAD_U64";
        std::env::set_var(key, "-1");
        let err = env_u64(key, 60).unwrap_err();
        std::env::remove_var(key);
        assert!(format!("{err:#}").contains(key));
    }

    #[test]
    fn env_f32_rejects_non_finite_value() {
        let key = "__SCORACLE_TEST_BAD_F32";
        std::env::set_var(key, "NaN");
        let err = env_f32(key, 0.5).unwrap_err();
        std::env::remove_var(key);
        assert!(format!("{err:#}").contains(key));
    }
}

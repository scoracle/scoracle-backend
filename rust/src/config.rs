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
    /// Near-verbatim novelty policy — `COGNITION_NOVELTY_*`.
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
    /// Global ceiling on claimed items in flight across ALL stages
    /// (`COGNITION_DRAIN_CONCURRENCY`). `None` when unset, which is the normal case: the worker
    /// then derives it from the sum of every registered stage's `max_in_flight`, so the
    /// per-stage caps are the single source of truth and adding a stage needs no arithmetic here.
    ///
    /// This is a CLAIM bound, not a GPU bound — the per-host semaphores in `route.rs` remain the
    /// only thing deciding how many model calls actually run on a machine. Set it only to
    /// throttle: `1` restores the old strictly-sequential drain.
    pub drain_concurrency: Option<usize>,
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

        // ≥1: a 0-permit semaphore would block every model call forever.
        let ollama_max_concurrent = env_usize("OLLAMA_MAX_CONCURRENT", 1)?.max(1);

        Ok(Self {
            database_url,
            // 25, raised from 5 when the drain went concurrent (2026-07-26). 5 was sized for a
            // drain that ran ONE item at a time; with up to `drain_concurrency` handlers in
            // flight — each holding a connection, and some holding a transaction plus a query —
            // the pool starved and items failed with "pool timed out while waiting for an open
            // connection". Comfortably above the sum of the stage caps (11), and Postgres here
            // allows 100 with ~22 in use. A pool max is a ceiling, not a preallocation.
            db_max_conns: env_u32("COGNITION_DB_MAX_CONNS", 25)?,
            ollama_base_url,
            ollama_model,
            // 600s = 10 min. The old default was 60s, from L1 when every call was a short
            // local mistral completion. It has been wrong since the first narrative generation
            // and every real deploy overrode it in `.env.local`; a default that no running
            // system uses is a trap for the next reader, so it now states the real budget.
            ollama_timeout: Duration::from_secs(env_u64("OLLAMA_TIMEOUT_SECONDS", 600)?),
            ollama_max_concurrent,
            safety_net: Duration::from_secs(env_u64("COGNITION_SAFETY_NET_SECONDS", 30)?),
            // 1800s = 30 min = Go derive.StaleLease.
            stale_lease: Duration::from_secs(env_u64("COGNITION_STALE_LEASE_SECONDS", 1800)?),
            route,
            embed: EmbedConfig::from_env()?,
            scrub: ScrubConfig::from_env()?,
            // 1200s = 20 min: generous over the slowest observed item (a narratives batch
            // item ran ~4-7 min under the 07-15 catch-up load) yet still under stale-lease,
            // with room for the semaphore wait a concurrent drain adds on a busy host.
            // Matches the deployed value; the former 900 default was overridden everywhere.
            handler_timeout: Duration::from_secs(env_u64(
                "COGNITION_HANDLER_TIMEOUT_SECONDS",
                1200,
            )?),
            // 2700s = 45 min: generous over the slowest single handler; a wedge self-heals
            // in ≤45 min instead of the incident's 34 hours.
            watchdog: Duration::from_secs(env_u64("COGNITION_WATCHDOG_SECONDS", 2700)?),
            drain_concurrency: match env_opt("COGNITION_DRAIN_CONCURRENCY") {
                Some(raw) => Some(raw.parse::<usize>().map(|n| n.max(1)).with_context(|| {
                    format!("COGNITION_DRAIN_CONCURRENCY must be an unsigned integer, got {raw:?}")
                })?),
                None => None,
            },
        })
    }
}

/// ScrubConfig drives the near-verbatim novelty gate (`COGNITION_NOVELTY_*`). The novelty pass
/// compares a freshly-scrubbed article against recent CANONICAL coverage of its own entities and —
/// FIRST-SEEN wins — suppresses only a near-VERBATIM copy. `novelty_cosine` is gone with the
/// embedder: the cosine prefilter was the half of the rule that had no text check behind it, and it
/// produced every measured false suppression.
#[derive(Clone, Debug)]
pub struct ScrubConfig {
    /// Token-Jaccard at/above which two articles are near-VERBATIM (syndicated wire copy) and the
    /// second carries no new prose. Below it, anyone writing their own words — including the same
    /// outlet filing a second piece — passes through and is read.
    pub verbatim_jaccard: f32,
    /// How far back to look for a canonical original to dedup against. Reposts/syndication are
    /// recent; the default matches the narratives news lookback (72h) so anything still inside a
    /// corpus window can be collapsed at the gate.
    pub novelty_lookback: Duration,
}

impl Default for ScrubConfig {
    fn default() -> Self {
        Self {
            verbatim_jaccard: 0.90,
            novelty_lookback: Duration::from_secs(259_200),
        }
    }
}

impl ScrubConfig {
    /// from_env reads `COGNITION_NOVELTY_{VERBATIM_JACCARD,LOOKBACK_SECONDS}`, defaulting to the
    /// tuned band (0.90 / 72h). Raising `verbatim_jaccard` suppresses LESS (more pass-through, the
    /// safe direction); lowering it suppresses more aggressively.
    pub fn from_env() -> Result<Self> {
        let d = Self::default();
        Ok(Self {
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
    /// Per-BACKEND concurrency budget, keyed by `base_url` — the machine's budget, not the
    /// role's. Six characters sharing one host share one entry, which is the point: the
    /// semaphore models a physical GPU, so it must be keyed by the thing that has the GPU.
    /// Any `base_url` absent here falls back to `OLLAMA_MAX_CONCURRENT`.
    pub backend_concurrency: HashMap<String, usize>,
}

impl RouteConfig {
    /// from_env reads `COGNITION_ROUTE_<ROLE>` for every role (e.g.
    /// `COGNITION_ROUTE_EMOTIONAL_NEWS`), each defaulting to `default_model` on `base_url` —
    /// so with nothing configured every role is the one local model and routing moves zero bytes
    /// vs L1. `COGNITION_ROUTE_<ROLE>_CANDIDATE` adds the optional eval challenger.
    ///
    /// **The topology split (Plan §2.1) is now real.** `COGNITION_ROUTE_<ROLE>_BASE_URL` puts a
    /// role on a different machine — the router already keys backends by
    /// `(backend, model, base_url)`, so two roles on two hosts get two clients with no further
    /// ceremony. The intended shape is one model per machine: The Editor on the box with the
    /// small fast GPU, the six characters on the box with the memory for a larger model.
    ///
    /// Nothing about the DB moves. The remote host runs `ollama serve` and nothing else; the
    /// harness stays here, owns Postgres, and a remote generation is an ordinary HTTP response
    /// persisted by the same code that persists a local one.
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
            // A role's host: its own override, else the shared default. Trailing slashes are
            // trimmed so `http://mac:11434` and `http://mac:11434/` are ONE backend, not two
            // clients hammering one Ollama (the cache key is the string).
            let role_base = normalize_base_url(
                &env_opt(&format!("{key}_BASE_URL")).unwrap_or_else(|| base_url.to_string()),
            );
            roles.insert(
                role,
                ModelSpec {
                    backend: Backend::Ollama,
                    model: env_or(&key, default_model),
                    base_url: role_base.clone(),
                    think: parse_think(&format!("{key}_THINK")),
                },
            );
            if let Some(candidate_model) = env_opt(&format!("{key}_CANDIDATE")) {
                candidates.insert(
                    role,
                    ModelSpec {
                        backend: Backend::Ollama,
                        model: candidate_model,
                        // A challenger defaults to its role's host, not the global one, so
                        // A/B-ing a remote role does not silently pull the challenger local.
                        base_url: normalize_base_url(
                            &env_opt(&format!("{key}_CANDIDATE_BASE_URL"))
                                .unwrap_or_else(|| role_base.clone()),
                        ),
                        think: parse_think(&format!("{key}_CANDIDATE_THINK")),
                    },
                );
            }
        }
        Self {
            roles,
            candidates,
            backend_concurrency: parse_backend_concurrency(&env_or(
                "COGNITION_BACKEND_CONCURRENCY",
                "",
            )),
        }
    }
}

/// normalize_base_url strips trailing slashes so two spellings of one host cannot become two
/// backends with two independent concurrency budgets — which would silently double the load on
/// a GPU the governor believes it is protecting.
fn normalize_base_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

/// parse_backend_concurrency reads `COGNITION_BACKEND_CONCURRENCY` — a comma-separated
/// `<base_url>=<permits>` list, e.g.
/// `http://localhost:11434=3,http://mac-mini:11434=1`.
///
/// Keyed by host rather than by role because the semaphore models a GPU: the six characters
/// sharing one machine must share one budget, and the Editor on its own machine must not be
/// throttled by their traffic. Malformed entries are SKIPPED rather than fatal — a typo here
/// should cost the default budget, not refuse to boot the pipeline.
fn parse_backend_concurrency(raw: &str) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        // rsplit_once: base URLs contain no '=', but split from the right regardless so a
        // query-string-bearing URL could never eat the permit count.
        let Some((url, permits)) = entry.rsplit_once('=') else {
            continue;
        };
        let Ok(n) = permits.trim().parse::<usize>() else {
            continue;
        };
        let url = normalize_base_url(url);
        // An empty host would never match a spec's base_url anyway; drop it rather than
        // carry a junk entry that makes the parsed map lie about how many hosts are configured.
        if url.is_empty() {
            continue;
        }
        // 0 permits would block every call to that host forever.
        out.insert(url, n.max(1));
    }
    out
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

    // --- the topology split: per-host concurrency budgets ---

    #[test]
    fn backend_concurrency_parses_a_two_host_split() {
        let m = parse_backend_concurrency(
            "http://localhost:11434=3, http://mac-mini:11434=1",
        );
        assert_eq!(m.get("http://localhost:11434"), Some(&3));
        assert_eq!(m.get("http://mac-mini:11434"), Some(&1));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn backend_concurrency_is_empty_when_unset() {
        assert!(parse_backend_concurrency("").is_empty());
    }

    #[test]
    fn backend_concurrency_skips_malformed_entries_without_failing_boot() {
        // A typo should cost that host the default budget, not refuse to start the pipeline.
        let m = parse_backend_concurrency("garbage,http://a:1=2,http://b:1=notanumber,=5");
        assert_eq!(m.get("http://a:1"), Some(&2));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn backend_concurrency_clamps_zero_to_one() {
        // 0 permits would block every call to that host forever.
        assert_eq!(parse_backend_concurrency("http://a:1=0").get("http://a:1"), Some(&1));
    }

    #[test]
    fn backend_concurrency_key_matches_the_normalized_base_url() {
        // The budget is looked up by the spec's base_url, so both sides must normalize the
        // same way -- otherwise a trailing slash silently drops the host to the default budget.
        let m = parse_backend_concurrency("http://mac-mini:11434/=2");
        assert_eq!(m.get(&normalize_base_url("http://mac-mini:11434")), Some(&2));
    }

    #[test]
    fn normalize_base_url_collapses_trailing_slashes_and_padding() {
        assert_eq!(normalize_base_url("  http://mac:11434/  "), "http://mac:11434");
        assert_eq!(normalize_base_url("http://mac:11434"), "http://mac:11434");
    }

}

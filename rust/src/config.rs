//! Environment configuration. Variable names mirror the Go backend
//! (`go/internal/config/config.go`) so the Rust Cognition Harness and the Go API read
//! the same `.env.local`. DB URL precedence matches Go: DATABASE_PRIVATE_URL
//! wins over DATABASE_URL.

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
    /// Fallback governor budget for a host absent from `COGNITION_BACKEND_CONCURRENCY`.
    /// Defaults to one and is clamped to at least one.
    pub ollama_max_concurrent: usize,
    /// Periodic drain even without a NOTIFY (Go worker default: 30s).
    pub safety_net: Duration,
    /// A `running` row idle longer than this is recovered to `pending`. It must exceed any
    /// single item's processing budget so a slow-but-alive worker is not stolen.
    pub stale_lease: Duration,
    /// Role-to-model map. Every role defaults to `ollama_model` on `ollama_base_url`;
    /// `COGNITION_ROUTE_*` overrides per role.
    pub route: RouteConfig,
    /// Per-item ceiling on one stage handler run. A wedged await inside a handler (model
    /// call, DB acquire) fails the item after this long instead of stalling the drain. Zero
    /// disables.
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
    /// Whether the Desk compiles packets (`COGNITION_PACKET_COMPILE`, default off). Storyline
    /// assembly is unconditional; this remains an operational brake on compilation cost.
    pub packet_compile: bool,
    /// The context window EVERY voice on this host requests (`VOICE_NUM_CTX`, else the 4096
    /// packet envelope). Resolved once at boot because two items in one drain must not disagree
    /// about the window, or the shared runner reloads between them.
    pub voice_num_ctx: i32,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = env_opt("DATABASE_PRIVATE_URL")
            .or_else(|| env_opt("DATABASE_URL"))
            .ok_or_else(|| anyhow!("DATABASE_PRIVATE_URL or DATABASE_URL must be set"))?;

        // These fields are also the per-role route defaults.
        let ollama_base_url = env_or("OLLAMA_BASE_URL", "http://localhost:11434");
        let ollama_model = env_or("OLLAMA_MODEL", "mistral:7b");
        let route = RouteConfig::from_env(&ollama_model, &ollama_base_url);

        // ≥1: a 0-permit semaphore would block every model call forever.
        let ollama_max_concurrent = env_usize("OLLAMA_MAX_CONCURRENT", 1)?.max(1);

        // Resolve the voice window once so handlers in one drain cannot disagree and reload the
        // shared runner between calls.
        let voice_num_ctx =
            crate::route::resolve_voice_num_ctx(env_opt("VOICE_NUM_CTX").as_deref());

        Ok(Self {
            database_url,
            // The default exceeds the sum of stage caps; a pool max is a ceiling, not a
            // preallocation.
            db_max_conns: env_u32("COGNITION_DB_MAX_CONNS", 25)?,
            ollama_base_url,
            ollama_model,
            // Ten minutes is the normal model-call budget.
            ollama_timeout: Duration::from_secs(env_u64("OLLAMA_TIMEOUT_SECONDS", 600)?),
            ollama_max_concurrent,
            safety_net: Duration::from_secs(env_u64("COGNITION_SAFETY_NET_SECONDS", 30)?),
            // Thirty-minute stale lease.
            stale_lease: Duration::from_secs(env_u64("COGNITION_STALE_LEASE_SECONDS", 1800)?),
            route,
            // Twenty minutes, including time waiting for a busy host; still below stale_lease.
            handler_timeout: Duration::from_secs(env_u64(
                "COGNITION_HANDLER_TIMEOUT_SECONDS",
                1200,
            )?),
            // Forty-five-minute no-progress watchdog.
            watchdog: Duration::from_secs(env_u64("COGNITION_WATCHDOG_SECONDS", 2700)?),
            drain_concurrency: match env_opt("COGNITION_DRAIN_CONCURRENCY") {
                Some(raw) => Some(raw.parse::<usize>().map(|n| n.max(1)).with_context(|| {
                    format!("COGNITION_DRAIN_CONCURRENCY must be an unsigned integer, got {raw:?}")
                })?),
                None => None,
            },
            packet_compile: env_bool("COGNITION_PACKET_COMPILE", false),
            voice_num_ctx,
        })
    }
}

/// Backend used to construct a [`ModelSpec`]'s `Inference` implementation. `OpenAi` names the
/// `/v1/chat/completions` protocol, not a vendor, and cannot carry `num_ctx` or `think`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
    Ollama,
    OpenAi,
}

impl Backend {
    /// Parse `COGNITION_ROUTE_<ROLE>_BACKEND`. Unknown values fall back to Ollama.
    pub fn from_env_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" | "omlx" | "mlx" => Backend::OpenAi,
            _ => Backend::Ollama,
        }
    }
}

/// Concrete model and host for a [`Role`]. Stage code names roles, never model ids.
#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub backend: Backend,
    pub model: String,
    pub base_url: String,
    /// Per-role think preference. `Some(false)` disables thinking for this role's calls.
    pub think: Option<bool>,
}

/// Role-to-model configuration for [`Router`](crate::route::Router). Candidates are eval-only;
/// an unconfigured deployment routes every role to the default Ollama model.
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
    /// Read each role's model, backend, host, think preference, and optional eval candidate from
    /// `COGNITION_ROUTE_<ROLE>*`. Unset roles use `default_model` on `base_url`.
    pub fn from_env(default_model: &str, base_url: &str) -> Self {
        let mut roles = HashMap::new();
        let mut candidates = HashMap::new();
        // Thinking is off unless a role opts in. `_THINK=omit` withholds the field for a backend
        // that rejects an explicit `false`.
        let parse_think = |key: &str| -> Option<bool> {
            match env_opt(key).as_deref().map(str::to_lowercase).as_deref() {
                Some("true" | "1" | "yes") => Some(true),
                Some("omit") => None,
                _ => Some(false),
            }
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
                    backend: env_opt(&format!("{key}_BACKEND"))
                        .map(|v| Backend::from_env_str(&v))
                        .unwrap_or(Backend::Ollama),
                    model: env_or(&key, default_model),
                    base_url: role_base.clone(),
                    think: parse_think(&format!("{key}_THINK")),
                },
            );
            if let Some(candidate_model) = env_opt(&format!("{key}_CANDIDATE")) {
                candidates.insert(
                    role,
                    ModelSpec {
                        // A challenger inherits its role's backend unless told otherwise, so an
                        // A/B never silently compares two ENGINES when it means to compare models.
                        backend: env_opt(&format!("{key}_CANDIDATE_BACKEND"))
                            .or_else(|| env_opt(&format!("{key}_BACKEND")))
                            .map(|v| Backend::from_env_str(&v))
                            .unwrap_or(Backend::Ollama),
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

/// A switch, not a number: anything but the affirmative set is off, and an unset key is the
/// default. Deliberately total — a typo in a deploy env must not fail a boot, it must leave the
/// switch where the default put it (and the boot line logs the resolved value).
fn env_bool(key: &str, default: bool) -> bool {
    match env_opt(key) {
        Some(raw) => matches!(
            raw.trim().to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        None => default,
    }
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

    // --- the topology split: per-host concurrency budgets ---

    #[test]
    fn backend_concurrency_parses_a_two_host_split() {
        let m = parse_backend_concurrency("http://localhost:11434=3, http://mac-mini:11434=1");
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
        assert_eq!(
            parse_backend_concurrency("http://a:1=0").get("http://a:1"),
            Some(&1)
        );
    }

    #[test]
    fn backend_concurrency_key_matches_the_normalized_base_url() {
        // The budget is looked up by the spec's base_url, so both sides must normalize the
        // same way -- otherwise a trailing slash silently drops the host to the default budget.
        let m = parse_backend_concurrency("http://mac-mini:11434/=2");
        assert_eq!(
            m.get(&normalize_base_url("http://mac-mini:11434")),
            Some(&2)
        );
    }

    #[test]
    fn normalize_base_url_collapses_trailing_slashes_and_padding() {
        assert_eq!(
            normalize_base_url("  http://mac:11434/  "),
            "http://mac:11434"
        );
        assert_eq!(normalize_base_url("http://mac:11434"), "http://mac:11434");
    }
}

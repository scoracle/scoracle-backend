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
//! challenger. With nothing configured every role resolves to the one local model, so this moved
//! ZERO bytes vs the L1 single router — `for_role`'s contract is unchanged, which is why the
//! identity/topology/backend swaps (Plan §2.1) never move a stage.
//!
//! `Inference` is the one real trait under Route: the model backend. `OllamaClient` is its
//! first (today, only) impl; a second impl (vLLM) waits until it is real, not built on
//! speculation. The trait's three methods are exactly the inherent methods `OllamaClient`
//! already exposes, so the impl is a thin delegation and the wire body stays single-sourced.

use crate::config::{Backend, ModelSpec, RouteConfig};
use crate::ollama::{GenerateOptions, GenerateResult, OllamaClient};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

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

/// GovernedInference is the GPU governor (the operational prerequisite the Cutover Plan §94.2
/// names) — a decorator over any [`Inference`] backend that acquires a SHARED semaphore permit
/// before each `generate`. The Router wraps every backend it builds in this, sharing ONE
/// semaphore (`OLLAMA_MAX_CONCURRENT`), so the total in-flight model calls across ALL roles and
/// models never exceeds the budget — there is one GPU, so one budget. The worker's sequential
/// drain is already an implicit 1; this makes the bound explicit so a brief Go+Rust transition
/// overlap (Go's own model gate + the Rust worker) and any future parallel drain stay bounded,
/// and it sits at the model-call SEAM so no caller can bypass it (every `for_role(_).generate`
/// is governed, unlike a check in one handler). `model`/`request_body` are pure/local (no GPU),
/// so they delegate WITHOUT a permit — only `generate`, the call that hits the GPU, is gated.
struct GovernedInference {
    inner: Arc<dyn Inference>,
    gpu: Arc<Semaphore>,
}

#[async_trait]
impl Inference for GovernedInference {
    async fn generate(&self, prompt: &str, opts: &GenerateOptions) -> Result<GenerateResult> {
        // The permit is held for the whole call and released on drop — success OR error — so a
        // failed/timed-out call never leaks one. `acquire` only errors if the semaphore is
        // closed, which we never do, so surface that as an error rather than panic.
        let _permit = self
            .gpu
            .acquire()
            .await
            .map_err(|e| anyhow!("gpu governor semaphore closed: {e}"))?;
        self.inner.generate(prompt, opts).await
    }

    fn model(&self) -> &str {
        self.inner.model()
    }

    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
        self.inner.request_body(prompt, opts)
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
    /// `Arc<dyn Inference>` per DISTINCT (backend, model, base_url) — so the single-model default
    /// builds exactly one backend shared by every role (byte-identical to the L1 single
    /// router) — wired to each role's incumbent, plus any configured A/B challenger. `timeout`
    /// is the shared per-call budget (`OLLAMA_TIMEOUT_SECONDS`); per-backend timeouts move
    /// into `ModelSpec` when topology splits (HORIZON).
    ///
    /// `max_concurrent` is the GPU governor budget (`OLLAMA_MAX_CONCURRENT`): ONE semaphore,
    /// built here and shared by every backend, caps total in-flight model calls across all
    /// roles (one GPU → one budget). Clamped to ≥1 (0 would block every call forever).
    pub fn from_config(
        cfg: &RouteConfig,
        timeout: Duration,
        max_concurrent: usize,
    ) -> Result<Self> {
        // One shared GPU governor across every backend this router builds.
        let gpu = Arc::new(Semaphore::new(max_concurrent.max(1)));
        // Cache keyed by the spec's identity, so two roles naming the same model get the same
        // backend Arc rather than two clients hammering one Ollama.
        let mut built: HashMap<String, Arc<dyn Inference>> = HashMap::new();
        let mut incumbents = HashMap::with_capacity(cfg.roles.len());
        for (role, spec) in &cfg.roles {
            incumbents.insert(*role, build_backend(&mut built, spec, timeout, &gpu)?);
        }
        let mut candidates = HashMap::with_capacity(cfg.candidates.len());
        for (role, spec) in &cfg.candidates {
            candidates.insert(*role, build_backend(&mut built, spec, timeout, &gpu)?);
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
/// where a new backend (vLLM) plugs in — one arm, alongside its new `impl Inference`. Every
/// constructed backend is wrapped in [`GovernedInference`] sharing the one `gpu` semaphore, so
/// the cached (and role-shared) Arc is already governed — the bound is impossible to bypass.
fn build_backend(
    built: &mut HashMap<String, Arc<dyn Inference>>,
    spec: &ModelSpec,
    timeout: Duration,
    gpu: &Arc<Semaphore>,
) -> Result<Arc<dyn Inference>> {
    let key = format!("{:?}|{}|{}", spec.backend, spec.base_url, spec.model);
    if let Some(existing) = built.get(&key) {
        return Ok(Arc::clone(existing));
    }
    let raw: Arc<dyn Inference> = match spec.backend {
        Backend::Ollama => Arc::new(
            OllamaClient::new(&spec.base_url, &spec.model, timeout)
                .with_context(|| format!("build ollama backend for {}", spec.model))?,
        ),
    };
    // Wrap in the shared GPU governor before caching — so every role resolving to this model
    // shares both the one backend AND the one concurrency budget.
    let backend: Arc<dyn Inference> = Arc::new(GovernedInference {
        inner: raw,
        gpu: Arc::clone(gpu),
    });
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
        roles.insert(Role::EmotionalNews, spec("local-news:latest"));
        roles.insert(Role::StatsLogic, spec("local-news:latest")); // same model → shared Arc
        roles.insert(Role::Sql, spec("sqlcoder:7b")); // distinct → its own Arc
        let cfg = RouteConfig {
            roles,
            candidates: HashMap::new(),
        };
        let router = Router::from_config(&cfg, Duration::from_secs(60), 1).unwrap();

        assert!(Arc::ptr_eq(
            &router.for_role(Role::EmotionalNews),
            &router.for_role(Role::StatsLogic),
        ));
        assert!(!Arc::ptr_eq(
            &router.for_role(Role::EmotionalNews),
            &router.for_role(Role::Sql),
        ));
        assert_eq!(
            router.for_role(Role::EmotionalNews).model(),
            "local-news:latest"
        );
        assert_eq!(router.for_role(Role::Sql).model(), "sqlcoder:7b");
    }

    #[test]
    fn candidate_for_is_none_without_a_challenger() {
        let roles = Role::all()
            .into_iter()
            .map(|r| (r, spec("local-news:latest")))
            .collect();
        let router = Router::from_config(
            &RouteConfig {
                roles,
                candidates: HashMap::new(),
            },
            Duration::from_secs(60),
            1,
        )
        .unwrap();
        assert!(router.candidate_for(Role::EmotionalNews).is_none());
    }

    #[test]
    fn candidate_for_resolves_a_configured_challenger() {
        let roles = Role::all()
            .into_iter()
            .map(|r| (r, spec("local-news:latest")))
            .collect();
        let mut candidates = HashMap::new();
        candidates.insert(Role::EmotionalNews, spec("candidate-news:latest"));
        let router = Router::from_config(
            &RouteConfig { roles, candidates },
            Duration::from_secs(60),
            1,
        )
        .unwrap();
        assert_eq!(
            router.candidate_for(Role::EmotionalNews).unwrap().model(),
            "candidate-news:latest"
        );
        assert!(router.candidate_for(Role::StatsLogic).is_none()); // only EmotionalNews has one
    }

    // --- GPU governor (GovernedInference) ------------------------------------------------
    // A mock backend that records the PEAK number of concurrent generate() calls — so a test
    // can assert the shared semaphore caps in-flight model calls at the configured budget.
    struct PeakCounter {
        current: Arc<std::sync::atomic::AtomicUsize>,
        peak: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl Inference for PeakCounter {
        async fn generate(&self, _p: &str, _o: &GenerateOptions) -> Result<GenerateResult> {
            use std::sync::atomic::Ordering::SeqCst;
            let now = self.current.fetch_add(1, SeqCst) + 1;
            self.peak.fetch_max(now, SeqCst);
            // Hold the permit across an await so concurrent callers actually contend.
            tokio::time::sleep(Duration::from_millis(15)).await;
            self.current.fetch_sub(1, SeqCst);
            Ok(GenerateResult {
                response: String::new(),
                model: "mock".to_string(),
                total_duration: Duration::ZERO,
                eval_count: 0,
            })
        }
        fn model(&self) -> &str {
            "mock"
        }
        fn request_body(&self, _p: &str, _o: &GenerateOptions) -> serde_json::Value {
            serde_json::Value::Null
        }
    }

    /// fire N concurrent generate() calls through a governor with `permits` permits and return
    /// the peak observed concurrency. Deterministic even on the current-thread test runtime:
    /// the sleep yields, so all callers that CAN acquire a permit do before any releases.
    async fn peak_under_governor(permits: usize, n: usize) -> usize {
        use std::sync::atomic::AtomicUsize;
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let governed: Arc<dyn Inference> = Arc::new(GovernedInference {
            inner: Arc::new(PeakCounter {
                current,
                peak: Arc::clone(&peak),
            }),
            gpu: Arc::new(Semaphore::new(permits)),
        });
        let opts = GenerateOptions::default();
        let mut handles = Vec::new();
        for _ in 0..n {
            let g = Arc::clone(&governed);
            let o = opts.clone();
            handles.push(tokio::spawn(async move { g.generate("x", &o).await }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        peak.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[tokio::test]
    async fn governor_serializes_with_one_permit() {
        // The single-GPU default: 5 concurrent calls, 1 permit ⇒ peak concurrency is exactly 1.
        assert_eq!(peak_under_governor(1, 5).await, 1);
    }

    #[tokio::test]
    async fn governor_allows_exactly_the_budget() {
        // 2 permits ⇒ up to 2 in flight (and, with 6 contenders, exactly 2 — the bound is the
        // budget, not a hard-coded 1).
        assert_eq!(peak_under_governor(2, 6).await, 2);
    }
}

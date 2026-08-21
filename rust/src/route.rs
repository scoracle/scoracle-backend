//! Route — the model-call seam: role → concrete model at runtime (Plan §1.1 / §2).
//!
//! A stage names a **role** (the model's JOB), never a model name; the `Router` resolves
//! that role to a concrete backend. Every CHARACTER stage owns its role (the identity split:
//! a character's voice must never silently flip with a sibling's route change), while utility
//! calls (graph extraction, identity adjudication) share `EmotionalNews`.
//! This is the *swap seam*: the three swaps the Hardware
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
use crate::openai::OpenAiClient;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// The window every ARCHBOX-LOCAL model stage requests — the Editor, graph and the Insider's
/// transfer call alike.
///
/// **The uniformity is the point, not the number.** ollama keys a loaded runner on its context
/// size, so two roles on the same host and model asking for different sizes force an
/// unload-and-reload on every alternation between them. That is a settled diagnosis, not a
/// theory: it is what `graph` (`num_ctx: 0`) did against The Editor's 8192 on the local card
/// (matching them took Archbox's reloads to zero), and what the Mac's mixed 8192/16384 era cost
/// before [`VOICE_NUM_CTX_PACKET`] unified the voices — roughly a fifth of the host's wall clock
/// spent reloading weights it already had (measured 2026-07-26). These stages share archbox's
/// single runner (`MAX_LOADED_MODELS=1`), so they move together or not at all.
///
/// Anchored here in Phase 9 (9.1). It previously lived as `article_reader::ARTICLE_NUM_CTX` and was
/// borrowed across the tree from inside the legacy reader; when that module was demolished the
/// constant had to outlive it, and `route.rs` — which already owns the voice windows — is where a
/// per-host window belongs. **Value unchanged at 4096** (D-T29), so the demolition moved no numbers.
/// `EDITOR_NUM_CTX` still exists as the Editor's own name for it and is pinned equal by test.
pub const LOCAL_STAGE_NUM_CTX: i32 = 4096;

/// The window EVERY character voice requests (§7's envelope): prompt + memory + packet render +
/// reservation, all inside 4096. This is the number the whole diet is sized against — a voice
/// that still needed the legacy 16384 would mean the render or the memory block had quietly
/// grown back. All seats share archbox's one pinned runner (single-box, 2026-08-20), so the
/// uniformity argument on [`LOCAL_STAGE_NUM_CTX`] applies directly: one window, or the runner
/// reloads on every alternation. (The legacy 16384 constant this replaced left with the legacy
/// corpus — the 08-06 rail cutover; `VOICE_NUM_CTX` survives only as the env override's name.)
pub const VOICE_NUM_CTX_PACKET: i32 = 4096;

/// Whether an effective voice window is a SMALL one — the 4096 envelope rather than the 16384 the
/// legacy corpus was sized for.
///
/// **Every output reservation and every context cap in the six voices keys on THIS.** It survived
/// the Phase 9 rail prune deliberately: the window is set by `VOICE_NUM_CTX`, not by which corpus
/// a voice reads, so it stayed a live knob when the rail taxonomy did not (that `Rail` enum is
/// now deleted outright). The arithmetic it defends is unchanged and rail-agnostic — a `num_predict` larger than the window is the silent
/// system-prompt eviction `narratives_decode_budget` has documented since it was written (and D-T40
/// re-measured on the Editor).
pub fn small_voice_window(num_ctx: i32) -> bool {
    num_ctx <= VOICE_NUM_CTX_PACKET
}

/// The effective voice window: the `VOICE_NUM_CTX` env override when set, else `VOICE_NUM_CTX_PACKET`.
///
/// The override exists because a pinned single-runner host wants ONE window — uniformity is what
/// keeps it loaded (§3). An unparseable or absurd value resolves to the default rather than
/// failing a boot, the same total-parse discipline `RAIL` used to carry.
pub fn resolve_voice_num_ctx(raw: Option<&str>) -> i32 {
    raw.and_then(|v| v.trim().parse::<i32>().ok())
        .filter(|n| *n >= 512)
        .unwrap_or(VOICE_NUM_CTX_PACKET)
}

#[async_trait]
impl Inference for OpenAiClient {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        OpenAiClient::generate_with_body(self, prompt, opts).await
    }

    fn model(&self) -> &str {
        OpenAiClient::model(self)
    }

    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
        OpenAiClient::request_body(self, prompt, opts)
    }
}

/// Role names a model's JOB, not its name. Stages address a `Role`; the `Router` maps it to
/// a concrete model. The one place a model id may appear is the router config (L2) — never
/// in stage code. Derives `Hash` for the L2 role→model map.
///
/// The character voices each hold their own route seam as an *identity* split — an earned route
/// change for one voice must never silently flip another (`MomentumLogic` out of `StatsLogic`
/// 2026-07-11, `NarrativeLogic` 2026-07-12, `TransferLogic`/`VibeLogic` 2026-07-22,
/// `OracleLogic` from day one; un-configured, every seat resolves to the default model, so each
/// split alone moved zero behavior). `OracleLogic` backs the crown (the Sigil): the ONE call
/// that reads the five pillar cards and emits the reading + score (the panel's `SynthesisLogic`
/// was folded in and retired, 2026-07-21). `EmotionalNews` is UTILITY-only: graph extraction
/// and transfer identity adjudication — calls with no character voice. `Multilang` is the
/// HORIZON normalize role; `Sql` is the SQLCoder role.
/// *(The `ArticleReader` role — the legacy rail's reader seat — was deleted in Phase 9 (9.1);
/// `Role::all()` is 11 seats now, not 12.)*
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    StatsLogic,
    MomentumLogic,
    NarrativeLogic,
    /// The Editor (PLAN-one-rail Phase 3): reads every arrival on the ep1 contract.
    /// Settled by hardware (§4 ruling) — `COGNITION_ROUTE_EDITOR` on archbox. It shadowed the
    /// legacy `ArticleReader` seat until cutover; that role was deleted in Phase 9 (9.1), and
    /// `COGNITION_ROUTE_ARTICLE_READER` retired with it on BOTH machines.
    Editor,
    /// The Investigator (PLAN-one-rail Phases 4–5): box-score retrieval and entity discovery.
    /// Rides the SAME pinned archbox model as the Editor (§3 — `MAX_LOADED_MODELS=1` makes any
    /// other tag evict the incumbent); `COGNITION_ROUTE_INVESTIGATOR`. Its only v1 model
    /// calls are describe-only page triage — numbers never enter rows through this role.
    Investigator,
    TransferLogic,
    VibeLogic,
    OracleLogic,
    EmotionalNews,
    Multilang,
    Sql,
}

impl Role {
    /// all is every role, so config and router can populate the full map — keeping
    /// `Router::for_role` total (a role always resolves to a model).
    pub fn all() -> [Role; 11] {
        [
            Role::StatsLogic,
            Role::MomentumLogic,
            Role::NarrativeLogic,
            Role::Editor,
            Role::Investigator,
            Role::TransferLogic,
            Role::VibeLogic,
            Role::OracleLogic,
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
            Role::MomentumLogic => "momentum-logic",
            Role::NarrativeLogic => "narrative-logic",
            Role::Editor => "editor",
            Role::Investigator => "investigator",
            Role::TransferLogic => "transfer-logic",
            Role::VibeLogic => "vibe-logic",
            Role::OracleLogic => "oracle-logic",
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
            Role::MomentumLogic => "MOMENTUM_LOGIC",
            Role::NarrativeLogic => "NARRATIVE_LOGIC",
            Role::Editor => "EDITOR",
            Role::Investigator => "INVESTIGATOR",
            Role::TransferLogic => "TRANSFER_LOGIC",
            Role::VibeLogic => "VIBE_LOGIC",
            Role::OracleLogic => "ORACLE_LOGIC",
            Role::EmotionalNews => "EMOTIONAL_NEWS",
            Role::Multilang => "MULTILANG",
            Role::Sql => "SQL",
        }
    }
}

/// Inference — the model-call backend, the genuine swap point. `OllamaClient` is the first
/// impl; a `dyn Inference` is what a `Role` resolves to. `generate` returns the exact
/// wire body it POSTed; `request_body` remains for no-call deterministic builders.
#[async_trait]
pub trait Inference: Send + Sync {
    /// generate performs one non-streaming completion. No auto-retry — the work queue owns
    /// backoff (the boundary the host already enforces), and returns the exact
    /// `/api/generate` body sent with the result.
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)>;

    /// model returns the concrete model id, for provenance (`model_version`).
    fn model(&self) -> &str;

    /// request_body returns the exact `/api/generate` body `generate` would POST for
    /// `(prompt, opts)`.
    fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value;
}

#[async_trait]
impl Inference for OllamaClient {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
        // Inherent method wins method resolution, but qualify it explicitly to make the
        // delegation unambiguous (no accidental recursion into the trait method).
        OllamaClient::generate_with_body(self, prompt, opts).await
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
/// models never exceeds the budget — there is one GPU, so one budget. It makes the bound
/// explicit so the concurrent drain stays bounded no matter how stages multiply,
/// and it sits at the model-call SEAM so no caller can bypass it (every `for_role(_).generate`
/// is governed, unlike a check in one handler). `model`/`request_body` are pure/local (no GPU),
/// so they delegate WITHOUT a permit — only `generate`, the call that hits the GPU, is gated.
struct GovernedInference {
    inner: Arc<dyn Inference>,
    gpu: Arc<Semaphore>,
}

#[async_trait]
impl Inference for GovernedInference {
    async fn generate(
        &self,
        prompt: &str,
        opts: &GenerateOptions,
    ) -> Result<(GenerateResult, serde_json::Value)> {
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
    /// The GPU governor budget is **per host**, keyed by `base_url`: one semaphore per distinct
    /// machine, sized from `COGNITION_BACKEND_CONCURRENCY` with `max_concurrent`
    /// (`OLLAMA_MAX_CONCURRENT`) as the fallback. Clamped to ≥1 (0 would block forever).
    ///
    /// It was one global semaphore until the 2026-08 two-host era, on the reasoning "one GPU →
    /// one budget". That premise dies the moment a role lives on another machine: a single
    /// permit shared across two hosts makes them take turns, so the remote box idles while the
    /// local one works and the split buys nothing. Keyed by host, the two drain concurrently.
    /// The per-host keying is KEPT post-consolidation (2026-08-20, single box): it is the
    /// env-only rollback/re-expansion path — add a role's `_BASE_URL` and its host gets its
    /// own governor with no code change.
    ///
    /// Single-host deploys are unaffected: every role resolves to one `base_url`, so one
    /// semaphore is built and the behaviour is byte-identical to the global-budget version.
    pub fn from_config(
        cfg: &RouteConfig,
        timeout: Duration,
        max_concurrent: usize,
    ) -> Result<Self> {
        // One governor per distinct host, created on first sight of that host.
        let mut governors: HashMap<String, Arc<Semaphore>> = HashMap::new();
        // Cache keyed by the spec's identity, so two roles naming the same model get the same
        // backend Arc rather than two clients hammering one Ollama.
        let mut built: HashMap<String, Arc<dyn Inference>> = HashMap::new();
        let mut incumbents = HashMap::with_capacity(cfg.roles.len());
        for (role, spec) in &cfg.roles {
            let gpu = governor_for(&mut governors, cfg, spec, max_concurrent);
            incumbents.insert(*role, build_backend(&mut built, spec, timeout, &gpu)?);
        }
        let mut candidates = HashMap::with_capacity(cfg.candidates.len());
        for (role, spec) in &cfg.candidates {
            let gpu = governor_for(&mut governors, cfg, spec, max_concurrent);
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

/// governor_for returns the semaphore guarding the host a spec lives on, creating it the first
/// time that host is seen. Every backend on one `base_url` shares it, so six characters on one
/// machine share that machine's budget while a Editor on another machine keeps its own.
fn governor_for(
    governors: &mut HashMap<String, Arc<Semaphore>>,
    cfg: &RouteConfig,
    spec: &ModelSpec,
    default_max_concurrent: usize,
) -> Arc<Semaphore> {
    if let Some(existing) = governors.get(&spec.base_url) {
        return Arc::clone(existing);
    }
    let permits = cfg
        .backend_concurrency
        .get(&spec.base_url)
        .copied()
        .unwrap_or(default_max_concurrent)
        .max(1);
    let gpu = Arc::new(Semaphore::new(permits));
    governors.insert(spec.base_url.clone(), Arc::clone(&gpu));
    gpu
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
    let key = format!(
        "{:?}|{}|{}|{:?}",
        spec.backend, spec.base_url, spec.model, spec.think
    );
    if let Some(existing) = built.get(&key) {
        return Ok(Arc::clone(existing));
    }
    let raw: Arc<dyn Inference> = match spec.backend {
        Backend::Ollama => Arc::new(
            OllamaClient::with_think(&spec.base_url, &spec.model, timeout, spec.think)
                .with_context(|| format!("build ollama backend for {}", spec.model))?,
        ),
        // oMLX and anything else speaking `/v1/chat/completions` (D-T41). `think` is deliberately
        // NOT threaded through: it is an ollama extension, so a role that needs it must stay on
        // ollama rather than have the flag silently dropped here.
        Backend::OpenAi => Arc::new(
            OpenAiClient::new(&spec.base_url, &spec.model, timeout)
                .with_context(|| format!("build openai backend for {}", spec.model))?,
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

    /// The window resolves from the env override when it is sane, and from the default otherwise
    /// — including for junk, which must never fail a boot (the total-parse discipline `RAIL`
    /// used to carry, kept after the Phase 9 prune deleted the rail itself).
    #[test]
    fn voice_window_override_beats_the_default_and_junk_falls_back() {
        assert_eq!(resolve_voice_num_ctx(None), VOICE_NUM_CTX_PACKET);
        assert_eq!(resolve_voice_num_ctx(Some("4096")), 4096);
        assert_eq!(resolve_voice_num_ctx(Some(" 8192 ")), 8192);
        for junk in ["", "big", "-1", "0", "511"] {
            assert_eq!(
                resolve_voice_num_ctx(Some(junk)),
                VOICE_NUM_CTX_PACKET,
                "{junk:?} must fall back, never fail a boot"
            );
        }
    }

    /// Everything that has to fit inside the window keys on the WINDOW. The pin that matters:
    /// 4096 is a small window whatever set it, so a host pinned there by env gets the small
    /// reservations rather than a 4,000-token reservation it cannot hold. This is why
    /// `small_voice_window` outlived the rail: it keys on the WINDOW, which is still a live knob.
    #[test]
    fn small_window_is_a_property_of_the_window() {
        assert!(small_voice_window(VOICE_NUM_CTX_PACKET));
        assert!(small_voice_window(2048));
        assert!(!small_voice_window(16384)); // the legacy corpus's window
        assert!(small_voice_window(resolve_voice_num_ctx(Some("4096"))));
    }

    fn spec(model: &str) -> ModelSpec {
        ModelSpec {
            backend: Backend::Ollama,
            model: model.to_string(),
            base_url: "http://localhost:11434".to_string(),
            think: None,
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
            backend_concurrency: HashMap::new(),
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
    fn character_role_split_is_inert_by_default() {
        // The 2026-07-22 identity split: un-configured, TransferLogic and VibeLogic resolve to
        // the same shared backend as every other default role — the split moves zero behavior
        // until a human sets COGNITION_ROUTE_{TRANSFER,VIBE}_LOGIC.
        let roles = Role::all()
            .into_iter()
            .map(|r| (r, spec("local-news:latest")))
            .collect();
        let router = Router::from_config(
            &RouteConfig {
                roles,
                candidates: HashMap::new(),
                backend_concurrency: HashMap::new(),
            },
            Duration::from_secs(60),
            1,
        )
        .unwrap();
        assert!(Arc::ptr_eq(
            &router.for_role(Role::TransferLogic),
            &router.for_role(Role::EmotionalNews),
        ));
        assert!(Arc::ptr_eq(
            &router.for_role(Role::VibeLogic),
            &router.for_role(Role::EmotionalNews),
        ));
    }

    #[test]
    fn character_roles_have_stable_config_and_telemetry_identities() {
        // Ledger rows key on as_str and deploys key on env_suffix — lock both spellings.
        assert_eq!(Role::TransferLogic.as_str(), "transfer-logic");
        assert_eq!(Role::VibeLogic.as_str(), "vibe-logic");
        assert_eq!(Role::Editor.as_str(), "editor");
        assert_eq!(Role::TransferLogic.env_suffix(), "TRANSFER_LOGIC");
        assert_eq!(Role::VibeLogic.env_suffix(), "VIBE_LOGIC");
        assert_eq!(Role::Editor.env_suffix(), "EDITOR");
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
                backend_concurrency: HashMap::new(),
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
            &RouteConfig { roles, candidates, backend_concurrency: HashMap::new() },
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
        async fn generate(
            &self,
            _p: &str,
            _o: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            use std::sync::atomic::Ordering::SeqCst;
            let now = self.current.fetch_add(1, SeqCst) + 1;
            self.peak.fetch_max(now, SeqCst);
            // Hold the permit across an await so concurrent callers actually contend.
            tokio::time::sleep(Duration::from_millis(15)).await;
            self.current.fetch_sub(1, SeqCst);
            Ok((
                GenerateResult {
                    response: String::new(),
                    model: "mock".to_string(),
                    total_duration: Duration::ZERO,
                    eval_count: 0,
                },
                serde_json::Value::Null,
            ))
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

    // ---------------------------------------------------------------------------
    // The topology split: one governor per HOST.
    // ---------------------------------------------------------------------------

    const ARCHBOX: &str = "http://localhost:11434";
    const MAC: &str = "http://mac-mini:11434";

    fn spec_on(model: &str, base_url: &str) -> ModelSpec {
        ModelSpec {
            backend: Backend::Ollama,
            model: model.to_string(),
            base_url: base_url.to_string(),
            think: None,
        }
    }

    fn cfg_with(budgets: &[(&str, usize)]) -> RouteConfig {
        RouteConfig {
            roles: HashMap::new(),
            candidates: HashMap::new(),
            backend_concurrency: budgets
                .iter()
                .map(|(u, n)| (u.to_string(), *n))
                .collect(),
        }
    }

    #[test]
    fn one_governor_per_host_shared_within_a_host() {
        let cfg = cfg_with(&[]);
        let mut g = HashMap::new();
        // Two different models on the SAME host share one budget — six characters on one
        // machine must not each get their own permit.
        let a = governor_for(&mut g, &cfg, &spec_on("mistral", ARCHBOX), 1);
        let b = governor_for(&mut g, &cfg, &spec_on("gemma3:4b", ARCHBOX), 1);
        assert!(Arc::ptr_eq(&a, &b), "same host must share one governor");
        // A different host gets its OWN budget — this is what stops the two boxes taking turns.
        let c = governor_for(&mut g, &cfg, &spec_on("mistral", MAC), 1);
        assert!(!Arc::ptr_eq(&a, &c), "distinct hosts must not share a governor");
        assert_eq!(g.len(), 2);
    }

    #[test]
    fn per_host_budget_overrides_the_global_default() {
        // Archbox reads with 3 in flight; the Mac generates one character at a time.
        let cfg = cfg_with(&[(ARCHBOX, 3)]);
        let mut g = HashMap::new();
        let arch = governor_for(&mut g, &cfg, &spec_on("gemma3:4b", ARCHBOX), 1);
        let mac = governor_for(&mut g, &cfg, &spec_on("mistral-nemo:12b", MAC), 1);
        assert_eq!(arch.available_permits(), 3, "configured host uses its budget");
        assert_eq!(mac.available_permits(), 1, "unlisted host falls back to the default");
    }

    #[test]
    fn a_zero_budget_cannot_deadlock_a_host() {
        // 0 permits would block every call to that host forever; clamp to 1.
        let cfg = cfg_with(&[(MAC, 0)]);
        let mut g = HashMap::new();
        let mac = governor_for(&mut g, &cfg, &spec_on("mistral", MAC), 1);
        assert_eq!(mac.available_permits(), 1);
    }

    #[test]
    fn single_host_deploys_build_exactly_one_governor() {
        // The regression that matters most: with no split configured, behaviour must be
        // byte-identical to the old single global semaphore.
        let roles: HashMap<Role, ModelSpec> = Role::all()
            .into_iter()
            .map(|r| (r, spec("local-news:latest")))
            .collect();
        let cfg = RouteConfig {
            roles,
            candidates: HashMap::new(),
            backend_concurrency: HashMap::new(),
        };
        let mut g = HashMap::new();
        for spec in cfg.roles.values() {
            governor_for(&mut g, &cfg, spec, 1);
        }
        assert_eq!(g.len(), 1, "one host ⇒ one budget, as before the split");
    }

    #[tokio::test]
    async fn two_hosts_run_concurrently_rather_than_taking_turns() {
        // The whole point of the split. Two hosts, one permit each: the pair must reach a
        // combined peak of 2 in flight. Under the old ONE-global-semaphore design this would
        // be 1 — the remote box idling while the local one worked.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let make = |permits: usize| -> Arc<dyn Inference> {
            Arc::new(GovernedInference {
                inner: Arc::new(PeakCounter {
                    current: Arc::clone(&current),
                    peak: Arc::clone(&peak),
                }),
                gpu: Arc::new(Semaphore::new(permits)),
            })
        };
        let hosts = [make(1), make(1)];
        let opts = GenerateOptions::default();
        let mut handles = Vec::new();
        for host in &hosts {
            for _ in 0..3 {
                let g = Arc::clone(host);
                let o = opts.clone();
                handles.push(tokio::spawn(async move { g.generate("x", &o).await }));
            }
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }
}

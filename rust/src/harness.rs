//! The capability library — the Cognition Harness context plus its primitives.
//!
//! `Harness` is the one capability context handed to every stage composition (Plan §1.0): it
//! generalizes the `(pool, ollama)` pair the old `StageHandler` received — the pool stays, the
//! single `OllamaClient` is replaced by the `Router` (role → model), and the CPU-bound
//! `Embedder` hangs off here too. Built once at boot (`main.rs`) and shared by every stage.
//!
//! The six primitives are *methods on `Harness`* (or a free fn, for `cluster`), not six
//! `dyn` traits — the primitives aren't swapped at runtime, the *models* and *parsers* are.
//! The only two real traits are the genuine swap points: `Inference` (the model backend, in
//! [`crate::route`]) and `Parser<T>` (the per-stage output plug-in, here).
//!
//! L0/L1 ships REAL impls for the three vibe needs — **Route** (via the `Router`), **Extract**
//! (`Harness::extract`), and **Persist** (the `Provenance` envelope + `debounce_unchanged`).
//! **Resolve**, **Embed**, and **Normalize** are shaped stubs (`unimplemented!()` bodies with
//! real signatures + types), so the floor is drawn for the HORIZON stages without building
//! infrastructure on speculation. See Plan §1.

use crate::ollama::{GenerateOptions, GenerateResult};
use crate::route::{Role, Router};
use anyhow::{Context, Result};
use sqlx::PgPool;

/// Harness — the capability context handed to every stage composition. Built once at boot.
pub struct Harness {
    /// The Postgres pool (builds on `db::build_pool`). The queue host clones this for its own
    /// mechanics; the primitives read it for their corpus loads and provenance writes.
    pub pool: PgPool,
    /// Route primitive — owns the `Inference` backend(s) per role.
    pub router: Router,
    /// Embed+cluster capability (candle). `None` until narratives lands (HORIZON) — the only
    /// optional resource, because it carries a heavy dependency the core does not need yet.
    pub embedder: Option<Embedder>,
}

// ===========================================================================
// Extract + validate (Plan §1.2) — REAL. The heart of the fail-closed claim.
// ===========================================================================

/// Parser turns a raw model response into a validated `T` — or the fail-closed marker.
///
/// * `Ok(Some(t))` — valid.
/// * `Ok(None)` — FAIL-CLOSED: the model failed / was unparseable / under-committed. The
///   caller persists the UNKNOWN marker (sentiment NULL, `is_rumor` NULL), NEVER a
///   fabricated-valid row. Validity is encoded *in `T`*, so an uncommitted field is
///   unrepresentable as a served row.
/// * `Err(_)` — transport / programming error → the work item fails and backs off.
pub trait Parser<T> {
    fn parse(&self, raw: &str) -> Result<Option<T>>;
}

/// Extracted carries the parsed value (or the fail-closed `None`) plus the provenance Persist
/// needs. `request_body` is the *exact* wire body that was sent (sourced from the same
/// `Inference::request_body` the call used), so it can never drift from what was POSTed —
/// the property the Phase-1 temp-0 proof leans on.
#[derive(Debug)]
pub struct Extracted<T> {
    /// `None` = the fail-closed marker.
    pub value: Option<T>,
    /// Which concrete model answered (echoed in the `GenerateResult`).
    pub model: String,
    /// The exact user prompt sent to the model.
    pub built_prompt: String,
    /// The exact `/api/generate` wire body (for the parity diff / archive).
    pub request_body: serde_json::Value,
    /// Tokens the model evaluated (perf/telemetry; not all stages persist it).
    pub eval_count: i32,
}

impl Harness {
    /// extract is `route(role) → generate(prompt, opts) → parser.parse(response)` in one
    /// call, with the fail-closed contract enforced at the type boundary. A parse failure
    /// surfaces as the parser's `Err` (item fails + backs off); a fail-closed `Ok(None)`
    /// flows through as `Extracted.value == None` for the caller to persist as the marker.
    pub async fn extract<T, P: Parser<T>>(
        &self,
        role: Role,
        prompt: &str,
        opts: &GenerateOptions,
        parser: &P,
    ) -> Result<Extracted<T>> {
        let backend = self.router.for_role(role);
        let gen: GenerateResult = backend
            .generate(prompt, opts)
            .await
            .context("model generate")?;
        // The wire body, taken from the SAME backend + opts the call used (single source of
        // truth) — so the recorded request can't drift from the one that was sent.
        let request_body = backend.request_body(prompt, opts);
        let value = parser.parse(&gen.response)?;
        Ok(Extracted {
            value,
            model: gen.model,
            built_prompt: prompt.to_string(),
            request_body,
            eval_count: gen.eval_count,
        })
    }
}

// ===========================================================================
// Persist-with-provenance (Plan §1.6) — REAL. The moat envelope + debounce.
// ===========================================================================

/// The provenance envelope every product row carries — the append-only archive (output +
/// exactly how it was derived) IS the moat. This is deliberately NOT a generic row-writer
/// (that would fight Postgres-as-serializer); each stage keeps its typed `INSERT` and binds
/// these shared fields. The fail-closed marker is a first-class variant, differing only in
/// the bound `Option` values — not a separate path.
#[derive(Clone, Debug)]
pub struct Provenance {
    /// `Extracted.model` for a scored row, or the router's model for the no-corpus marker.
    pub model_version: String,
    pub prompt_version: &'static str,
    /// `input_news_ids` / input component ids — the sources this derivation read.
    pub input_ids: Vec<i64>,
    /// `Some` → debounce: skip if unchanged (sigil). `None` → no debounce (vibe).
    pub input_hash: Option<String>,
}

/// EntityKey identifies the row a debounce check is scoped to. `season` is `Some` for
/// season-scoped products (sigil's `sigil_synthesis`) and `None` for entity-scoped ones
/// (vibe_scores, news_summaries).
#[derive(Clone, Debug)]
pub struct EntityKey {
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
    pub season: Option<i32>,
}

impl Harness {
    /// debounce_unchanged returns `true` when the entity's LATEST row in `table` already
    /// carries `input_hash == hash` (so the stage should skip — the sigil "did the inputs
    /// move?" gate). Mirrors `sigil.go::lastSynthesisHash` semantics: take the latest row
    /// regardless of nullability; a marker row's NULL `input_hash` compares unequal to any
    /// real hash, so a marker never wrongly causes a skip.
    ///
    /// `table` is a stage-controlled literal (never user input), so formatting it into the
    /// query carries no injection surface. vibe does not call this (it has no `input_hash`);
    /// it is shipped real for sigil, its first consumer (HORIZON).
    pub async fn debounce_unchanged(
        &self,
        table: &str,
        key: &EntityKey,
        hash: &str,
    ) -> Result<bool> {
        // `query_scalar` over a nullable column gives Option<Option<String>>:
        //   None        → no row for this entity      → don't skip
        //   Some(None)  → latest row has NULL hash    → don't skip (marker)
        //   Some(Some)  → compare to `hash`
        let latest: Option<Option<String>> = if key.season.is_some() {
            let q = format!(
                "SELECT input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4 \
                 ORDER BY generated_at DESC LIMIT 1"
            );
            sqlx::query_scalar(&q)
                .bind(&key.entity_type)
                .bind(key.entity_id)
                .bind(&key.sport)
                .bind(key.season)
                .fetch_optional(&self.pool)
                .await
        } else {
            let q = format!(
                "SELECT input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 \
                 ORDER BY generated_at DESC LIMIT 1"
            );
            sqlx::query_scalar(&q)
                .bind(&key.entity_type)
                .bind(key.entity_id)
                .bind(&key.sport)
                .fetch_optional(&self.pool)
                .await
        }
        .with_context(|| {
            format!(
                "debounce check {table} {}/{}",
                key.entity_type, key.entity_id
            )
        })?;

        Ok(latest.flatten().as_deref() == Some(hash))
    }
}

// ===========================================================================
// Resolve (Plan §1.3) — SHAPED STUB. Model now, embeddings later, same signature.
// ===========================================================================

/// EntityType discriminates the two resolvable kinds. (The work queue carries the type as a
/// string; Resolve and its candidates use this enum.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityType {
    Player,
    Team,
}

/// IdentityCard holds the disambiguators that break a same-name tie. `current_club` is the
/// strongest signal (see `news_scrub.go` / `transfer.go`). Sparse-tolerant — any field may
/// be absent.
#[derive(Clone, Debug, Default)]
pub struct IdentityCard {
    pub nationality: Option<String>,
    pub current_club: Option<String>,
    pub position: Option<String>,
}

/// Candidate is a known entity plus its identity-card disambiguators — one option Resolve
/// chooses among.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub entity_type: EntityType,
    pub entity_id: i32,
    pub name: String,
    pub identity: IdentityCard,
}

/// Resolved is the one candidate a `resolve_one` settled on, plus `subject` — an audit trail
/// of who the text was REALLY about (the transfer subject-resolver shape).
#[derive(Clone, Debug)]
pub struct Resolved {
    pub entity_id: i32,
    pub entity_type: EntityType,
    pub subject: String,
}

/// Resolution is the per-candidate kept/dropped verdict `resolve_set` returns (the news-scrub
/// gate shape — vet WHICH linked candidates the text is genuinely about).
#[derive(Clone, Debug)]
pub struct Resolution {
    pub entity_id: i32,
    pub entity_type: EntityType,
    pub kept: bool,
}

impl Harness {
    /// resolve_one: which ONE candidate (if any) the `raw_token` is, given its context.
    /// Fail-closed: ambiguous / contradicted / not-found ⇒ `None` (never a guess). The
    /// transfer subject-resolver shape. SHAPED STUB (HORIZON — see Plan §1.3).
    pub async fn resolve_one(
        &self,
        _role: Role,
        _raw_token: &str,
        _context: &str,
        _candidates: &[Candidate],
    ) -> Result<Option<Resolved>> {
        unimplemented!(
            "Resolve primitive (resolve_one) — shaped for transfers (HORIZON); Plan §1.3"
        )
    }

    /// resolve_set: vet WHICH of N linked candidates the text is genuinely about — a
    /// per-candidate kept/dropped verdict. Fail-closed to "drop the non-primary links" on
    /// parse failure. The news-scrub gate shape. SHAPED STUB (HORIZON — see Plan §1.3).
    pub async fn resolve_set(
        &self,
        _role: Role,
        _context: &str,
        _candidates: &[Candidate],
    ) -> Result<Vec<Resolution>> {
        unimplemented!(
            "Resolve primitive (resolve_set) — shaped for the scrub gate (HORIZON); Plan §1.3"
        )
    }
}

// ===========================================================================
// Embed + cluster (Plan §1.4) — SHAPED STUB. Rust's genuine CPU-bound win (candle).
// ===========================================================================

/// A dense embedding vector.
pub type Vector = Vec<f32>;

/// Embedder — the CPU-bound capability (candle). Placeholder until the `candle` dependency
/// lands with narratives (HORIZON); `Harness::embedder` is `None` until then, so this is
/// never constructed yet.
pub struct Embedder {}

/// Cluster groups input indices the model should treat as one storyline.
#[derive(Clone, Debug)]
pub struct Cluster {
    /// Indices into the `embed` input that fall in this cluster.
    pub members: Vec<usize>,
}

impl Harness {
    /// embed vectorizes texts (candle, batched). SHAPED STUB (HORIZON — see Plan §1.4).
    pub async fn embed(&self, _texts: &[String]) -> Result<Vec<Vector>> {
        unimplemented!("Embed primitive — candle-backed (HORIZON, narratives); Plan §1.4")
    }
}

/// cluster groups vectors by cosine similarity + threshold — DETERMINISTIC math, not a model
/// call. It stays in Rust (not Postgres) only because it is *transient compute feeding a
/// model* (storyline grouping for narratives), never a stored derived stat. SHAPED STUB
/// (HORIZON — see Plan §1.4).
pub fn cluster(_vectors: &[Vector], _threshold: f32) -> Vec<Cluster> {
    unimplemented!(
        "cluster — deterministic cosine/agglomerative merge (HORIZON, narratives); Plan §1.4"
    )
}

// ===========================================================================
// Normalize (Plan §1.5) — SHAPED STUB. multilang = normalize + (narratives).
// ===========================================================================

/// RawMention is an entity surface-form found in normalized text (to be Resolved downstream).
#[derive(Clone, Debug)]
pub struct RawMention {
    pub text: String,
}

/// NormalizedText is any-language text rendered to English + the entity mentions in it.
#[derive(Clone, Debug)]
pub struct NormalizedText {
    pub english: String,
    pub entities: Vec<RawMention>,
    pub source_lang: String,
}

impl Harness {
    /// normalize is `route(Multilang) + extract` — any-language text → English-normalized +
    /// entity-tagged. SHAPED STUB; the impl waits on the router's A/B eval choosing a
    /// multilang model on a measured win (HORIZON — see Plan §1.5).
    pub async fn normalize(&self, _text: &str) -> Result<NormalizedText> {
        unimplemented!("Normalize primitive — route(Multilang)+extract (HORIZON); Plan §1.5")
    }
}

//! The capability library — the Cognition Harness context plus its primitives.
//!
//! `Harness` is the one capability context handed to every stage composition (Plan §1.0): it
//! generalizes the `(pool, ollama)` pair the old `StageHandler` received — the pool stays, the
//! single `OllamaClient` is replaced by the `Router` (role → model). Built once at boot
//! (`main.rs`) and shared by every stage.
//!
//! The primitives are *methods on `Harness`*, not `dyn` traits — the primitives aren't swapped
//! at runtime, the *models* and *parsers* are. The only two real traits are the genuine swap
//! points: `Inference` (the model backend, in [`crate::route`]) and `Parser<T>` (the per-stage
//! output plug-in, here).
//!
//! The live primitives are **Route** (via the `Router`), **Extract** (`Harness::extract`), and
//! **Persist** (the `Provenance` envelope + `debounce_unchanged`). The Resolve/Embed/Normalize
//! shaped stubs (Plan §1.3–1.5) were deleted with the embed layer once the legacy rail's
//! relevance and novelty gates — their only consumers — were demolished (Phase 9).

use crate::ollama::{GenerateOptions, GenerateResult};
use crate::route::{Role, Router};
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::time::Duration;

/// Harness — the capability context handed to every stage composition. Built once at boot.
pub struct Harness {
    /// The Postgres pool (builds on `db::build_pool`). The queue host clones this for its own
    /// mechanics; the primitives read it for their corpus loads and provenance writes.
    pub pool: PgPool,
    /// Route primitive — owns the `Inference` backend(s) per role.
    pub router: Router,
    /// The worker's per-item ceiling (`COGNITION_HANDLER_TIMEOUT_SECONDS`), exposed so a handler
    /// that makes N *sequential* model calls can stop itself before the axe falls instead of being
    /// cancelled mid-loop. `Duration::ZERO` means unbounded, matching the worker's own reading of
    /// a zero timeout — the eval and one-shot binaries build the harness that way, so an
    /// inspection run always drives an entity to completion no matter how long it takes.
    ///
    /// Only `transfers` reads it today, and the asymmetry is the point: every other junction makes
    /// exactly one `extract` call per item, so its wall clock is one generation and cannot
    /// approach the ceiling. `transfers` makes one per candidate pair plus one per wire-wrap
    /// target, so its wall clock is a queue depth — it hit 1200s on 18 teams on 2026-07-27, and
    /// the half it lost was always the wrap, which runs last.
    pub handler_budget: Duration,
    /// The context window every voice on this host requests (`VOICE_NUM_CTX`, else the 4096
    /// packet envelope — [`crate::route::resolve_voice_num_ctx`]). Resolved once at boot so two
    /// items in one drain can never disagree about the window, and so every output reservation
    /// and context cap keys on the WINDOW — the arithmetic that must hold.
    pub voice_num_ctx: i32,
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
/// `Inference::generate` call that POSTed it), so it can never drift from what was POSTed.
#[derive(Debug)]
pub struct Extracted<T> {
    /// `None` = the fail-closed marker.
    pub value: Option<T>,
    /// The model's verbatim response text. When `value` is `None` this is the ONLY record of
    /// what the model actually said — persist it with the failure marker, or the fail-closed
    /// path is undiagnosable from the database (the Aug-17 adjudication failures stored "").
    pub raw_response: String,
    /// Which concrete model answered (echoed in the `GenerateResult`).
    pub model: String,
    /// The exact user prompt sent to the model.
    pub built_prompt: String,
    /// The exact `/api/generate` wire body for ledger/eval archive.
    pub request_body: serde_json::Value,
    /// Tokens the model evaluated (perf/telemetry; not all stages persist it).
    pub eval_count: i32,
    /// Wall-clock milliseconds of the model call (F-036: persisted per call via each
    /// stage's ledger `context_budget`, so throughput regressions are queryable, not felt).
    pub wall_ms: u64,
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
        let (gen, request_body): (GenerateResult, serde_json::Value) = backend
            .generate(prompt, opts)
            .await
            .context("model generate")?;
        let value = parser.parse(&gen.response)?;
        Ok(Extracted {
            value,
            raw_response: gen.response,
            model: gen.model,
            built_prompt: prompt.to_string(),
            request_body,
            eval_count: gen.eval_count,
            wall_ms: gen.total_duration.as_millis() as u64,
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
    /// Optional trigger payload captured for stages whose payload is caller-provided
    /// rather than a stage literal. Stored here so marker and scored rows bind it
    /// through the same envelope.
    pub trigger_payload: Option<serde_json::Value>,
}

impl Provenance {
    pub fn with_trigger_payload(mut self, payload: &serde_json::Value) -> Self {
        self.trigger_payload = Some(payload.clone());
        self
    }

    pub fn trigger_payload_json(&self, fallback: &str) -> String {
        self.trigger_payload
            .as_ref()
            .map(serde_json::Value::to_string)
            .unwrap_or_else(|| fallback.to_string())
    }
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

    /// latest_with_hash fetches the entity's LATEST synthesis row in ONE query, returning
    /// `(score, input_hash)` — the two facts the crown needs from that row: the previous-score
    /// baseline (delta display + persisted `previous_score`) AND the debounce hash. It folds the
    /// crown's former two round-trips (`debounce_unchanged` + `last_score`) into one — each was an
    /// identical `... ORDER BY generated_at DESC LIMIT 1` over the same row (plan A1), a consistent
    /// (non-torn) read of one prior synthesis. `debounce_unchanged` stays as the standalone bool
    /// gate for callers that only need the skip decision. (The prior BLURB rode along here in the
    /// panel era; it was dropped in the crown fold. The prior-READING memory that replaced it was
    /// itself retired at or9 — the crown is blind to memories now.)
    ///
    /// Both columns are read nullable and returned already flattened: a no-row entity and a marker
    /// row (NULL score / NULL hash) are semantically identical to the consumers — score `None` ⇒ 0
    /// baseline; hash `None` compares unequal to any real hash ⇒ never skips. `table` is a
    /// stage-controlled literal (no injection surface); it must expose `score`/`input_hash`
    /// columns (`sigil_synthesis` is the only caller today).
    pub async fn latest_with_hash(
        &self,
        table: &str,
        key: &EntityKey,
    ) -> Result<(Option<i16>, Option<String>)> {
        let row: Option<(Option<i16>, Option<String>)> = if key.season.is_some() {
            let q = format!(
                "SELECT score, input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 AND season = $4 \
                 ORDER BY generated_at DESC LIMIT 1"
            );
            sqlx::query_as(&q)
                .bind(&key.entity_type)
                .bind(key.entity_id)
                .bind(&key.sport)
                .bind(key.season)
                .fetch_optional(&self.pool)
                .await
        } else {
            let q = format!(
                "SELECT score, input_hash FROM {table} \
                 WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 \
                 ORDER BY generated_at DESC LIMIT 1"
            );
            sqlx::query_as(&q)
                .bind(&key.entity_type)
                .bind(key.entity_id)
                .bind(&key.sport)
                .fetch_optional(&self.pool)
                .await
        }
        .with_context(|| {
            format!(
                "latest_with_hash {table} {}/{}",
                key.entity_type, key.entity_id
            )
        })?;
        Ok(row.unwrap_or((None, None)))
    }

    /// latest_row fetches one column from the entity's LATEST row in a product table.
    ///
    /// Load-bearing details:
    /// - the SELECT casts `{column}::text` because sqlx will not decode every product
    ///   column type (for example `sigil_synthesis.score` smallint) as `String`;
    /// - `query_scalar` returns `Option<Option<String>>`, and this deliberately flattens
    ///   no-row and NULL-in-latest-row. That is fine for the latest-value helpers this
    ///   consolidates: both cases mean no skip / no baseline / no last hash. A future
    ///   caller that needs to distinguish those states should use a bespoke query.
    ///
    /// `table` and `column` are stage-controlled literals (never user input), so formatting
    /// them into the query carries no injection surface.
    pub async fn latest_row(
        &self,
        table: &str,
        key: &EntityKey,
        column: &str,
    ) -> Result<Option<String>> {
        let latest: Option<Option<String>> = if key.season.is_some() {
            let q = format!(
                "SELECT {column}::text FROM {table} \
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
                "SELECT {column}::text FROM {table} \
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
                "latest_row {table}.{column} {}/{}",
                key.entity_type, key.entity_id
            )
        })?;
        Ok(latest.flatten())
    }
}

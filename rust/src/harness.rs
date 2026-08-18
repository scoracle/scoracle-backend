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

use crate::embed::{cosine_similarity, Embedder};
use crate::ollama::{GenerateOptions, GenerateResult};
use crate::route::{Role, Router};
use anyhow::{anyhow, Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::Duration;

/// Harness — the capability context handed to every stage composition. Built once at boot.
pub struct Harness {
    /// The Postgres pool (builds on `db::build_pool`). The queue host clones this for its own
    /// mechanics; the primitives read it for their corpus loads and provenance writes.
    pub pool: PgPool,
    /// Route primitive — owns the `Inference` backend(s) per role.
    pub router: Router,
    /// Embed+cluster capability (candle, CPU — Plan §1.4). `Option` because the model is a
    /// heavy resource not every entry point needs: the service and embed-aware experiments
    /// construct it with `Some`; eval/input-build paths that never embed leave it `None`.
    /// Loading it is `Embedder::from_config`.
    ///
    /// SHRINKING: the relevance gate and the novelty gate no longer embed anything (teardown
    /// §2.1/§2.2). The last two consumers are `narratives`' pre-model corpus clustering and
    /// `threads`' centroid cosine — both retire in Phase 3, when The Journalist declares thread
    /// identity itself and the embedder can be deleted outright.
    pub embedder: Option<Embedder>,
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
    /// The context window every voice on this host requests (`VOICE_NUM_CTX`, else the rail's
    /// size — [`crate::route::resolve_voice_num_ctx`]). Carried beside the rail so the two can
    /// never disagree inside one drain, and so every output reservation and context cap can key
    /// on the WINDOW (the arithmetic that must hold) rather than on the rail (the corpus choice).
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

// ===========================================================================
// Resolve (Plan §1.3) — REAL. Embedding-hybrid impl lives in `crate::resolve` (it drops in
// BEHIND these types' signatures with no change to them — the §5 "library drawn right" test).
// The types (the primitive's vocabulary) stay here; the methods are `impl Harness` in resolve.rs.
// ===========================================================================

/// EntityType discriminates the two resolvable kinds. (The work queue carries the type as a
/// string; Resolve and its candidates use this enum.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityType {
    Player,
    Team,
}

impl EntityType {
    /// as_str is the DB/queue string form (`news_article_entities.entity_type`, `pipeline_work`).
    pub fn as_str(self) -> &'static str {
        match self {
            EntityType::Player => "player",
            EntityType::Team => "team",
        }
    }

    /// from_db_str maps the DB string to the enum; an unknown value (e.g. the article-keyed scrub
    /// stage's `'article'`, which is never a Resolve candidate) ⇒ `None`.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "player" => Some(EntityType::Player),
            "team" => Some(EntityType::Team),
            _ => None,
        }
    }
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

/// Resolution is the per-candidate kept/dropped verdict `resolve_set` returns (the news-scrub
/// gate shape — vet WHICH linked candidates the text is genuinely about).
#[derive(Clone, Debug)]
pub struct Resolution {
    pub entity_id: i32,
    pub entity_type: EntityType,
    pub kept: bool,
}

// `impl Harness { resolve_set }` is in `crate::resolve` — the embedding-hybrid recipe.

// ===========================================================================
// Embed + cluster (Plan §1.4) — both REAL: Embed (candle, CPU) + cluster (deterministic).
// ===========================================================================

/// A dense embedding vector.
pub type Vector = Vec<f32>;

/// Cluster groups input indices the model should treat as one storyline.
#[derive(Clone, Debug)]
pub struct Cluster {
    /// Indices into the `embed` input that fall in this cluster.
    pub members: Vec<usize>,
}

impl Harness {
    /// embed vectorizes texts on the CPU (candle, batched — Plan §1.4) via the loaded
    /// [`Embedder`]. CPU-bound work, so it is wrapped in `block_in_place` to keep it off the
    /// async reactor (it never contends with the generation GPU). Errors if no embedder is loaded —
    /// a programming error (only embed-using stages should call this, and they construct the
    /// `Harness` with `Some(embedder)`).
    pub async fn embed(&self, texts: &[String]) -> Result<Vec<Vector>> {
        let embedder = self.embedder.as_ref().ok_or_else(|| {
            anyhow!("embed called but no Embedder loaded (Harness.embedder is None)")
        })?;
        tokio::task::block_in_place(|| embedder.embed_batch(texts))
    }
}

/// uf_find is union-find with path halving — the cluster() merge helper.
fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// cluster groups vectors by cosine similarity — DETERMINISTIC math (single-link agglomerative
/// merge via union-find), NOT a model call. Two items join the same cluster when their cosine ≥
/// `threshold`; transitively-linked items merge into one storyline chain. It stays in Rust, not
/// Postgres, ONLY because it is *transient compute feeding a model* (storyline grouping + near-dup
/// dedup for narratives), never a stored derived stat — the one careful §1.4 boundary. O(n²)
/// pairwise, right for a per-entity corpus (tens–hundreds of articles). Deterministic output:
/// members sorted ascending, clusters ordered by smallest member — so a caller can diff runs.
pub fn cluster(vectors: &[Vector], threshold: f32) -> Vec<Cluster> {
    let n = vectors.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if cosine_similarity(&vectors[i], &vectors[j]) >= threshold {
                let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = uf_find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    let mut out: Vec<Cluster> = groups
        .into_values()
        .map(|mut members| {
            members.sort_unstable();
            Cluster { members }
        })
        .collect();
    // Every index lands in exactly one group, so members is non-empty; order by first member.
    out.sort_by_key(|c| c.members[0]);
    out
}

// ===========================================================================
// Normalize (Plan §1.5) — SHAPED STUB. multilang = normalize + (narratives).
// ===========================================================================

/// RawMention is an entity surface-form found in normalized text (to be resolved downstream).
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

#[cfg(test)]
mod tests {
    use super::*;

    // cluster() is pure deterministic math — fully offline-testable (no model).

    #[test]
    fn cluster_groups_similar_separates_dissimilar() {
        // Two tight pairs along different axes + one singleton.
        let v = vec![
            vec![1.0, 0.0, 0.0],   // 0 ─┐ axis x
            vec![0.99, 0.01, 0.0], // 1 ─┘
            vec![0.0, 1.0, 0.0],   // 2 ─┐ axis y
            vec![0.0, 0.98, 0.02], // 3 ─┘
            vec![0.0, 0.0, 1.0],   // 4    axis z (alone)
        ];
        let clusters = cluster(&v, 0.9);
        assert_eq!(clusters.len(), 3);
        assert_eq!(clusters[0].members, vec![0, 1]);
        assert_eq!(clusters[1].members, vec![2, 3]);
        assert_eq!(clusters[2].members, vec![4]);
    }

    #[test]
    fn cluster_single_link_is_transitive() {
        // a–b cosine 0.8, b–c cosine 0.6, a–c cosine 0.0. At threshold 0.55 the chain merges
        // all three even though a and c are not directly similar — the single-link property.
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.8_f32, 0.6];
        let c = vec![0.0_f32, 1.0];
        let clusters = cluster(&[a, b, c], 0.55);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members, vec![0, 1, 2]);
    }

    #[test]
    fn cluster_threshold_separates_the_chain() {
        // Same three, but threshold 0.7 breaks the b–c link (0.6 < 0.7): {a,b} and {c}.
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.8_f32, 0.6];
        let c = vec![0.0_f32, 1.0];
        let clusters = cluster(&[a, b, c], 0.7);
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].members, vec![0, 1]);
        assert_eq!(clusters[1].members, vec![2]);
    }

    #[test]
    fn cluster_empty_is_empty() {
        assert!(cluster(&[], 0.5).is_empty());
    }
}

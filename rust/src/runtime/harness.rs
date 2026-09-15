//! Shared model routing, extraction, parsing, provenance, and debounce primitives.

use crate::runtime::providers::ollama::GenerateOptions;
use crate::runtime::route::{Role, Router};
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::ops::{Deref, DerefMut};
use std::time::Duration;

/// Harness — the capability context handed to every stage composition. Built once at boot.
pub struct Harness {
    /// The Postgres pool (builds on `db::build_pool`). The queue host clones this for its own
    /// mechanics; the primitives read it for their corpus loads and provenance writes.
    pub pool: PgPool,
    /// Route primitive — owns the `Inference` backend(s) per role.
    pub router: Router,
    /// Per-item ceiling. Multi-call handlers use it to stop cleanly before cancellation.
    /// `Duration::ZERO` means unbounded.
    pub handler_budget: Duration,
    /// Context window shared by every voice on this host.
    pub voice_num_ctx: i32,
}

// ===========================================================================
// Extract and validate.
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

/// Parsed value plus the exact generation provenance.
#[derive(Debug)]
pub struct Extracted<T> {
    /// `None` = the fail-closed marker.
    pub value: Option<T>,
    /// Verbatim response, retained so fail-closed results remain diagnosable.
    pub raw_response: String,
    /// Which concrete model answered (echoed in the `GenerateResult`).
    pub model: String,
    /// The exact user prompt sent to the model.
    pub built_prompt: String,
    /// The exact `/api/generate` wire body for ledger/eval archive.
    pub request_body: serde_json::Value,
    /// Tokens the model evaluated (perf/telemetry; not all stages persist it).
    pub eval_count: i32,
    /// Wall-clock milliseconds of the model call.
    pub wall_ms: u64,
}

/// Model-call diagnostics shared by every generated product.
#[derive(Clone, Debug)]
pub struct GenerationCall {
    pub built_prompt: String,
    pub request_body: serde_json::Value,
    pub eval_count: Option<i32>,
    pub wall_ms: Option<u64>,
}

impl<T> From<&Extracted<T>> for GenerationCall {
    fn from(extracted: &Extracted<T>) -> Self {
        Self {
            built_prompt: extracted.built_prompt.clone(),
            request_body: extracted.request_body.clone(),
            eval_count: Some(extracted.eval_count),
            wall_ms: Some(extracted.wall_ms),
        }
    }
}

/// A seat-specific product wrapped in the provenance and call diagnostics common to every seat.
#[derive(Clone, Debug)]
pub struct Generation<T> {
    pub product: T,
    pub provenance: Provenance,
    pub call: Option<GenerationCall>,
}

impl<T> Generation<T> {
    pub fn called(
        product: T,
        model_version: String,
        prompt_version: &'static str,
        input_ids: Vec<i64>,
        input_hash: Option<String>,
        call: GenerationCall,
    ) -> Self {
        Self {
            product,
            provenance: Provenance {
                model_version,
                prompt_version,
                input_ids,
                input_hash,
            },
            call: Some(call),
        }
    }

    pub fn uncalled(
        product: T,
        model_version: String,
        prompt_version: &'static str,
        input_ids: Vec<i64>,
        input_hash: Option<String>,
    ) -> Self {
        Self {
            product,
            provenance: Provenance {
                model_version,
                prompt_version,
                input_ids,
                input_hash,
            },
            call: None,
        }
    }

    pub fn was_called(&self) -> bool {
        self.call.is_some()
    }

    pub fn request_body(&self) -> Option<&serde_json::Value> {
        self.call.as_ref().map(|call| &call.request_body)
    }

    /// Add the call telemetry shared by every cognition-ledger context budget.
    pub fn context_budget(&self, mut budget: serde_json::Value) -> serde_json::Value {
        if let serde_json::Value::Object(fields) = &mut budget {
            fields.insert(
                "eval_count".to_string(),
                serde_json::json!(self.call.as_ref().and_then(|call| call.eval_count)),
            );
            fields.insert(
                "wall_ms".to_string(),
                serde_json::json!(self.call.as_ref().and_then(|call| call.wall_ms)),
            );
        }
        budget
    }
}

impl<T> Deref for Generation<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.product
    }
}

impl<T> DerefMut for Generation<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.product
    }
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
        extract_with_backend(backend.as_ref(), prompt, opts, parser).await
    }
}

async fn extract_with_backend<T, P: Parser<T>>(
    backend: &dyn crate::runtime::route::Inference,
    prompt: &str,
    opts: &GenerateOptions,
    parser: &P,
) -> Result<Extracted<T>> {
    let mut built_prompt = prompt.to_string();
    for attempt in 0..3 {
        let result = async {
            let (gen, request_body) = backend
                .generate(&built_prompt, opts)
                .await
                .context("model generate")?;
            let value = parser.parse(&gen.response)?;
            Ok::<_, anyhow::Error>((gen, request_body, value))
        }
        .await;
        let (gen, request_body, value) = match result {
            Ok(result) => result,
            Err(error) if attempt < 2 && error.is::<crate::composition::form::SurfaceError>() => {
                tracing::warn!(%error, "card surface rewrite");
                built_prompt.push_str(&format!(
                    "\nOutput correction: {error} Rewrite from scratch as one compact paragraph. Keep only the main finding and one supporting detail. Target at most 500 body characters so the complete JSON fits. Do not enumerate every input."
                ));
                continue;
            }
            Err(error)
                if attempt < 2
                    && error.is::<crate::runtime::providers::ollama::IncompleteOutput>() =>
            {
                tracing::warn!(%error, "incomplete output rewrite");
                built_prompt.push_str("\nOutput correction: the response ran out of space. Rewrite from scratch as one compact paragraph. Keep only the main finding and one supporting detail. Target at most 500 body characters so the complete JSON fits. Do not enumerate every input.");
                continue;
            }
            Err(error) => return Err(error),
        };
        return Ok(Extracted {
            value,
            raw_response: gen.response,
            model: gen.model,
            built_prompt,
            request_body,
            eval_count: gen.eval_count,
            wall_ms: gen.total_duration.as_millis() as u64,
        });
    }
    unreachable!("bounded rewrite returns on its last attempt")
}

#[cfg(test)]
mod surface_tests {
    use super::*;
    use crate::runtime::providers::ollama::GenerateResult;
    use std::sync::Mutex;

    struct Backend(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl crate::runtime::route::Inference for Backend {
        async fn generate(
            &self,
            prompt: &str,
            opts: &GenerateOptions,
        ) -> Result<(GenerateResult, serde_json::Value)> {
            let reply = self.0.lock().unwrap().remove(0);
            if reply == "length" {
                crate::runtime::providers::ollama::validate_completion(Some("length"), Some(true))?;
            }
            Ok((
                GenerateResult {
                    response: reply,
                    thinking: String::new(),
                    model: "test".into(),
                    total_duration: Duration::ZERO,
                    prompt_eval_count: 1,
                    eval_count: 1,
                    completion_reason: Some("stop".into()),
                    raw_response_body: String::new(),
                },
                self.request_body(prompt, opts),
            ))
        }
        fn model(&self) -> &str {
            "test"
        }
        fn request_body(&self, prompt: &str, opts: &GenerateOptions) -> serde_json::Value {
            serde_json::json!({"prompt":prompt,"num_ctx":opts.num_ctx,"num_predict":opts.num_predict})
        }
    }
    struct BodyParser;
    impl Parser<String> for BodyParser {
        fn parse(&self, raw: &str) -> Result<Option<String>> {
            crate::composition::form::validate_body(raw)?;
            Ok(Some(raw.to_string()))
        }
    }

    #[tokio::test]
    async fn rewrite_is_bounded_and_preserves_evidence_and_capacity() {
        let opts = GenerateOptions {
            num_ctx: 4096,
            num_predict: 700,
            ..Default::default()
        };
        for first in ["x".repeat(1201), "length".into()] {
            let backend = Backend(Mutex::new(vec![
                first,
                "The measured creation is strong.".into(),
            ]));
            let result = extract_with_backend(&backend, "Original evidence", &opts, &BodyParser)
                .await
                .unwrap();
            assert!(backend.0.lock().unwrap().is_empty());
            assert!(result
                .built_prompt
                .starts_with("Original evidence\nOutput correction:"));
            assert!(result
                .built_prompt
                .contains("Target at most 500 body characters"));
            assert_eq!(result.request_body["num_ctx"], 4096);
            assert_eq!(result.request_body["num_predict"], 700);
        }
        let backend = Backend(Mutex::new(vec![
            "x".repeat(1201),
            "x".repeat(1201),
            "x".repeat(1201),
            "unused".into(),
        ]));
        assert!(
            extract_with_backend(&backend, "Evidence", &opts, &BodyParser)
                .await
                .is_err()
        );
        assert_eq!(backend.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_second_surface_failure_gets_one_final_bounded_rewrite() {
        let opts = GenerateOptions {
            num_ctx: 4096,
            num_predict: 700,
            ..Default::default()
        };
        let backend = Backend(Mutex::new(vec![
            "x".repeat(1201),
            "x".repeat(1201),
            "The measured creation is strong.".into(),
        ]));
        let result = extract_with_backend(&backend, "Original evidence", &opts, &BodyParser)
            .await
            .unwrap();
        assert!(backend.0.lock().unwrap().is_empty());
        assert_eq!(result.built_prompt.matches("Output correction:").count(), 2);
        assert_eq!(result.raw_response, "The measured creation is strong.");
    }
}

// ===========================================================================
// Persist with provenance.
// ===========================================================================

/// Shared provenance fields bound by each stage's typed insert.
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
    /// Returns true when the entity's latest row already carries this input hash.
    ///
    /// `table` is a stage-controlled literal (never user input), so formatting it into the
    /// query carries no injection surface. vibe does not call this (it has no `input_hash`);
    /// callers control `table`; it is never user input.
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

    /// Loads the latest score and input hash in one consistent read.
    /// Missing rows and NULL columns both flatten to `None`.
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

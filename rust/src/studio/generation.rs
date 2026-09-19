//! Validated products and creation provenance.

use anyhow::Result;
use std::ops::{Deref, DerefMut};

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
    /// The exact transport request body for ledger/eval archive.
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

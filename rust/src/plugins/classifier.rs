//! Archived RSS headlines and their source-bound relevance spectra.
pub mod adapter;
pub mod decision;
pub mod delivery;
mod headline;
pub(crate) mod identity;
pub mod manifest;
pub mod memories;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Prefix of the `input_version` on character work dispatched from a spectrum obligation.
/// Character work is dispatched from admitted headline spectra.
pub const CONTRACT: &str = "classifier-spectrum-v1";

/// Immutable RSS metadata and candidates; historical rows may also retain article text.
/// A query identity is a candidate, never a positive relevance label.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub article_id: i64,
    pub body: String,
    pub source: String,
    pub published_at: Option<String>,
    pub query_entities: Vec<Value>,
    #[serde(flatten)]
    pub provenance: BTreeMap<String, Value>,
}

fn hash(body: &str) -> String {
    hex::encode(Sha256::digest(body.as_bytes()))
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
pub(crate) mod plumbing_tests;

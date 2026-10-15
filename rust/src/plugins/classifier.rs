//! Native Classifier: the retained source contract and the decision-model spectrum.
pub mod adapter;
pub mod decision;
pub mod delivery;
pub(crate) mod identity;
pub mod manifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Prefix of the `input_version` on character work dispatched from a spectrum obligation.
/// Insider/Scout use it to tell classifier-derived work from the retired periodic pipeline.
pub const CONTRACT: &str = "classifier-spectrum-v1";

/// Retained publisher source and candidate query identities. The native spectrum binds to these
/// exact bytes; a query identity is a candidate, never a positive relevance label.
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
mod tests;

#[cfg(test)]
pub(crate) mod plumbing_tests;

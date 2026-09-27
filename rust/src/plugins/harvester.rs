//! Harvester: cascade relevance checks and deliver attributed, verbatim context.
//!
//! The application supplies a DecisionModel. The regular worker lives in `adapter`
//! and publishes `context::HarvestContext` under a queue claim. The older
//! `Harvester::harvest` JSON shape is retained for historical offline replays;
//! the worker and character handoffs do not publish or consume those packets.

pub mod adapter;
pub mod cognition;
pub mod context;
pub mod delivery;
pub mod maintenance;
pub mod policy;

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::studio::decision::DecisionModel;
use crate::studio::decision::{DecisionRequest, DecisionResponse};
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use anyhow::Result;
pub use cognition::Article;
use cognition::PreparedText;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub mod manifest {
    use super::*;
    use crate::studio::tools::DomainClass;

    pub const TASK: TaskKey = TaskKey::new("harvester");
    const TOOLS: [ToolGrant; 2] = [
        ToolGrant::WebFetch(&[DomainClass::CuratedArticles]),
        ToolGrant::Classification,
    ];
    pub const MANIFEST: PluginManifest = PluginManifest {
        id: PluginId::new("scoracle.internal.harvester"),
        task: TASK,
        claim_policy: ClaimPolicy::RANKED_ARTICLES,
        inference_routes: &[],
        // Publisher fetches can overlap while the local Laya service serializes
        // inference. They do not consume the Mac generative-model slot group.
        resources: ResourceProfile::unbounded_batch(4),
        tools: &TOOLS,
    };
}

/// An explicitly constructed plugin instance. No implicit model choice, fallback,
/// network fetch, queue admission, or database write occurs here.
pub struct Harvester {
    model: Arc<dyn DecisionModel>,
}

/// The cheap classification result over a retained publisher opening.
#[derive(Clone, Debug, Serialize)]
pub struct Classification {
    source_binding: String,
    pub prepared: PreparedText,
    pub relevance_request: DecisionRequest,
    pub relevance_response: DecisionResponse,
    pub character_request: Option<DecisionRequest>,
    pub character_response: Option<DecisionResponse>,
}

fn source_binding(article: &Article) -> Result<String> {
    let mut source = serde_json::to_value(article)?;
    // Historical teacher diagnostics are never part of intake evidence.
    source.as_object_mut().unwrap().remove("baseline");
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&source)?)))
}

impl Classification {
    pub fn relevant(&self) -> Result<bool> {
        cognition::passed_relevance(&self.relevance_request, &self.relevance_response)
    }
}

impl Harvester {
    pub fn new(model: Arc<dyn DecisionModel>) -> Self {
        Self { model }
    }

    pub fn manifest(&self) -> &'static PluginManifest {
        &manifest::MANIFEST
    }

    /// Offline compatibility path over an already retained publisher body.
    pub async fn classify(&self, article: &Article) -> Result<Classification> {
        anyhow::ensure!(!article.body.trim().is_empty(), "missing publisher body");
        let (excerpt, relevance_request) = cognition::prepare_relevance(article)?;
        let relevance_response = self.model.evaluate(&relevance_request).await?;
        let character_stage =
            if cognition::passed_relevance(&relevance_request, &relevance_response)? {
                let request = cognition::prepare_character_routing(article, &excerpt);
                let response = self.model.evaluate(&request).await?;
                Some((request, response))
            } else {
                None
            };
        let (character_request, character_response) = match character_stage {
            Some((request, response)) => (Some(request), Some(response)),
            None => (None, None),
        };
        Ok(Classification {
            source_binding: source_binding(article)?,
            prepared: excerpt,
            relevance_request,
            relevance_response,
            character_request,
            character_response,
        })
    }

    /// Compile against the same publisher evidence and prompts used for classification.
    /// This historical JSON shape retains publisher evidence for offline audit.
    pub fn compile(&self, article: &Article, mut result: Classification) -> Result<Value> {
        anyhow::ensure!(
            result.source_binding == source_binding(article)?,
            "article evidence changed after classification"
        );
        let character_stage = result
            .character_request
            .as_ref()
            .zip(result.character_response.take());
        let mut packet = cognition::compile(
            article,
            result.prepared,
            &result.relevance_request,
            result.relevance_response,
            character_stage,
        )?;
        packet["plugin_id"] = self.manifest().id.as_str().into();
        Ok(packet)
    }

    /// Compatibility path for already-materialized corpora.
    pub async fn harvest(&self, article: &Article) -> Result<Value> {
        let result = self.classify(article).await?;
        self.compile(article, result)
    }
}

#[cfg(test)]
mod tests;

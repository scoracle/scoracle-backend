//! Harvester owns headline screening; Editor owns article acquisition and routing.
//! Worker and replay callers share `context`; Laya supplies probability signals only.

pub mod adapter;
pub mod cognition;
pub mod context;
pub mod decision;
pub mod delivery;
pub mod maintenance;
pub mod policy;
pub mod prompt;

use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};
pub use cognition::Article;

pub mod manifest {
    use super::*;

    pub const TASK: TaskKey = TaskKey::new("harvester");
    pub const MANIFEST: PluginManifest = PluginManifest {
        id: PluginId::new("scoracle.internal.harvester"),
        task: TASK,
        claim_policy: ClaimPolicy::RANKED_ARTICLES,
        inference_routes: &[],
        // Headline calls can overlap while the local Laya service serializes
        // inference. They do not consume the Mac generative-model slot group.
        resources: ResourceProfile::unbounded_batch(4),
        tools: &[ToolGrant::Classification],
    };
}

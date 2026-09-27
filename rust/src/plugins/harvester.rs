//! Harvester owns source selection, bounded predicates, admission and routing intent.
//! Worker and replay callers share `context`; Laya supplies probability signals only.

pub mod adapter;
pub mod cognition;
pub mod context;
pub mod delivery;
pub mod maintenance;
pub mod policy;

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
pub use cognition::Article;

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

//! Headline scoring.
use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};

pub const TASK: TaskKey = TaskKey::new("classifier");
pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.classifier"),
    task: TASK,
    claim_policy: ClaimPolicy::FRESH_ARTICLES,
    inference_routes: &[],
    resources: ResourceProfile::grouped(4, crate::harness::fleet::ARCHBOX_SLOTS).batched(4),
    tools: &[],
};

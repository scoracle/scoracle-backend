//! Registration policy owned by this plugin.

use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};

pub const TASK: TaskKey = TaskKey::new("transfers");
pub use super::prompt::MODEL as ROUTE;

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.transfers"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::unbounded_batch(1),
    tools: &[ToolGrant::Inference],
};

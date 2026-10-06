//! Registration policy owned by this plugin.

use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};
use crate::harness::route::RouteKey;

pub const TASK: TaskKey = TaskKey::new("transfers");
pub const ROUTE: RouteKey = RouteKey::new("transfer-logic", "TRANSFER_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.transfers"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::unbounded_batch(1),
    tools: &[ToolGrant::Inference],
};

//! Registration policy owned by this plugin.

use crate::harness::fleet::ARCHBOX_SLOTS;
use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};
use crate::harness::route::RouteKey;

pub const TASK: TaskKey = TaskKey::new("rating");
pub const ROUTE: RouteKey = RouteKey::new("stats-logic", "STATS_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.rating"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::grouped(2, ARCHBOX_SLOTS),
    tools: &[ToolGrant::Inference],
};

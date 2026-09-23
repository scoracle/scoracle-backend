//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::ARCHBOX_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};

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

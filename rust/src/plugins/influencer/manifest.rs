//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("vibe");
pub const ROUTE: RouteKey = RouteKey::new("vibe-logic", "VIBE_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.vibe"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    // One slot leaves room in the shared voice group for the terminal Oracle.
    resources: ResourceProfile::grouped(1, MAC_SLOTS),
    tools: &[ToolGrant::Inference],
};

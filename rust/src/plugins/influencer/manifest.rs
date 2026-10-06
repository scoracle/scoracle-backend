//! Registration policy owned by this plugin.

use crate::harness::fleet::MAC_SLOTS;
use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};
use crate::harness::route::RouteKey;

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

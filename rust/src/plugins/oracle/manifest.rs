//! Registration policy owned by this plugin.

use crate::harness::fleet::MAC_SLOTS;
use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};
use crate::harness::route::RouteKey;

pub const TASK: TaskKey = TaskKey::new("sigil");
pub const ROUTE: RouteKey = RouteKey::new("oracle-logic", "ORACLE_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.sigil"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST.gated_by(
        &["narratives", "rating", "vibe", "momentum", "transfers"],
        &[
            "rating_completed",
            "rating_debounced",
            "vibe_completed",
            "momentum_completed",
            "narratives_completed",
            "transfer_published",
        ],
    ),
    inference_routes: &[ROUTE],
    resources: ResourceProfile::grouped(2, MAC_SLOTS),
    tools: &[ToolGrant::Inference],
};

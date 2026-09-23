//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};

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

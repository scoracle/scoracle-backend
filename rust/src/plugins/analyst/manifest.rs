//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("momentum");
pub const ROUTE: RouteKey = RouteKey::new("momentum-logic", "MOMENTUM_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.momentum"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST
        .gated_by(&["rating", "vibe"], &["rating_completed", "vibe_completed"]),
    inference_routes: &[ROUTE],
    resources: ResourceProfile::unbounded_batch(1),
    tools: &[ToolGrant::Inference],
};

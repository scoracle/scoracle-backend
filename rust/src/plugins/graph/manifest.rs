//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::ARCHBOX_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("graph");
pub const ROUTE: RouteKey = RouteKey::new("emotional-news", "EMOTIONAL_NEWS");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.graph"),
    task: TASK,
    claim_policy: ClaimPolicy::FIFO,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::grouped(ARCHBOX_SLOTS.1, ARCHBOX_SLOTS).batched(8),
    tools: &[ToolGrant::Inference],
};

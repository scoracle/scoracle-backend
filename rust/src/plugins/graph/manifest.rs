//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::ARCHBOX_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("graph");
pub const ROUTE: RouteKey = RouteKey::new("emotional-news", "EMOTIONAL_NEWS");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.graph"),
    contract_version: "g5",
    task: TASK,
    claim_policy: ClaimPolicy::FIFO,
    inference_routes: &[ROUTE],
    context_requirements: &[],
    consumes: &[ProductKind::EDITOR_READ],
    produces: &[ProductKind::RELATIONS],
    resources: ResourceProfile::grouped(ARCHBOX_SLOTS.1, ARCHBOX_SLOTS).batched(8),
    tools: &[
        ToolGrant::WorldRead,
        ToolGrant::Commit,
        ToolGrant::Inference,
    ],
};

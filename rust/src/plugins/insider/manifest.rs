//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::runtime::route::RouteKey;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("transfers");
pub const ROUTE: RouteKey = RouteKey::new("transfer-logic", "TRANSFER_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.transfers"),
    contract_version: "transfer-verdict-v1",
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE, crate::plugins::graph::manifest::ROUTE],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::EDITOR_PACKETS,
        ProviderId::SOURCED_MEMORY,
    ],
    consumes: &[ProductKind::EDITOR_READ, ProductKind::IDENTITY],
    produces: &[ProductKind::TRANSFERS],
    resources: ResourceProfile::unbounded_batch(1),
    tools: &[
        ToolGrant::WorldRead,
        ToolGrant::Commit,
        ToolGrant::Inference,
    ],
};

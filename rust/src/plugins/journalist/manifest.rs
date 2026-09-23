//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("narratives");
pub const ROUTE: RouteKey = RouteKey::new("narrative-logic", "NARRATIVE_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.narrative"),
    contract_version: "narratives-v3-schema",
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::EDITOR_PACKETS,
        ProviderId::SOURCED_MEMORY,
    ],
    consumes: &[ProductKind::EDITOR_READ],
    produces: &[ProductKind::NARRATIVES],
    resources: ResourceProfile::grouped(2, MAC_SLOTS),
    tools: &[
        ToolGrant::WorldRead,
        ToolGrant::Commit,
        ToolGrant::Inference,
    ],
};

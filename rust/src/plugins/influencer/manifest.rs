//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("vibe");
pub const ROUTE: RouteKey = RouteKey::new("vibe-logic", "VIBE_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.vibe"),
    contract_version: "vibe-v36",
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::EDITOR_PACKETS,
        ProviderId::SOURCED_MEMORY,
        ProviderId::LATEST_SELF_CARD,
    ],
    consumes: &[ProductKind::EDITOR_READ, ProductKind::NARRATIVES],
    produces: &[ProductKind::VIBE],
    // One slot leaves room in the shared voice group for the terminal Oracle.
    resources: ResourceProfile::grouped(1, MAC_SLOTS),
    tools: &[
        ToolGrant::WorldRead,
        ToolGrant::Commit,
        ToolGrant::Inference,
    ],
};

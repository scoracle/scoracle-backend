//! Registration policy owned by this plugin.

use crate::application::queue::work::Stage;
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::Role;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.narrative"),
    contract_version: "narratives-v3-schema",
    task: Stage::Narratives,
    model_roles: &[Role::NarrativeLogic],
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

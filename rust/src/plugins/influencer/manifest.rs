//! Registration policy owned by this plugin.

use crate::application::queue::work::Stage;
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::Role;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.vibe"),
    contract_version: "vibe-v36",
    task: Stage::Vibe,
    model_roles: &[Role::VibeLogic],
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

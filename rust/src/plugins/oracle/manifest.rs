//! Registration policy owned by this plugin.

use crate::application::queue::work::Stage;
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::Role;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.sigil"),
    contract_version: "oracle-reading-v2",
    task: Stage::Sigil,
    model_roles: &[Role::OracleLogic],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::CURRENT_SEASON,
        ProviderId::PILLAR_CARDS,
    ],
    consumes: &[
        ProductKind::NARRATIVES,
        ProductKind::RATING,
        ProductKind::VIBE,
        ProductKind::MOMENTUM,
        ProductKind::TRANSFERS,
    ],
    produces: &[ProductKind::SIGIL],
    resources: ResourceProfile::grouped(2, MAC_SLOTS),
    tools: &[
        ToolGrant::WorldRead,
        ToolGrant::Commit,
        ToolGrant::Inference,
    ],
};

//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, Stage};
use crate::runtime::route::Role;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: Stage = Stage::new("momentum");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.momentum"),
    contract_version: "momentum-summary-v1",
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST
        .gated_by(&["rating", "vibe"], &["rating_completed", "vibe_completed"]),
    model_roles: &[Role::MomentumLogic],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::CURRENT_SEASON,
        ProviderId::PILLAR_CARDS,
        ProviderId::MOMENTUM_SNAPSHOT,
        ProviderId::SOURCED_MEMORY,
    ],
    consumes: &[ProductKind::RATING, ProductKind::VIBE],
    produces: &[ProductKind::MOMENTUM],
    resources: ResourceProfile::unbounded_batch(1),
    tools: &[
        ToolGrant::WorldRead,
        ToolGrant::Commit,
        ToolGrant::Inference,
    ],
};

//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, Stage};
use crate::plugins::support::resources::ARCHBOX_SLOTS;
use crate::runtime::route::Role;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: Stage = Stage::new("rating");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.rating"),
    contract_version: "rating-commentary-v6",
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    model_roles: &[Role::StatsLogic],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::ANALYTICS_SNAPSHOT,
        ProviderId::SOURCED_MEMORY,
    ],
    consumes: &[ProductKind::BOX_SCORE, ProductKind::IDENTITY],
    produces: &[ProductKind::RATING],
    resources: ResourceProfile::grouped(2, ARCHBOX_SLOTS),
    tools: &[
        ToolGrant::WorldRead,
        ToolGrant::Commit,
        ToolGrant::Inference,
    ],
};

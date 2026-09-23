//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::ARCHBOX_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("rating");
pub const ROUTE: RouteKey = RouteKey::new("stats-logic", "STATS_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.rating"),
    contract_version: "rating-commentary-v6",
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
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

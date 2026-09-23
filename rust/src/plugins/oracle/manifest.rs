//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::MAC_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::ProviderId;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("sigil");
pub const ROUTE: RouteKey = RouteKey::new("oracle-logic", "ORACLE_LOGIC");

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.sigil"),
    contract_version: "oracle-reading-v2",
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST.gated_by(
        &["narratives", "rating", "vibe", "momentum", "transfers"],
        &[
            "rating_completed",
            "rating_debounced",
            "vibe_completed",
            "momentum_completed",
            "narratives_completed",
            "transfer_published",
        ],
    ),
    inference_routes: &[ROUTE],
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

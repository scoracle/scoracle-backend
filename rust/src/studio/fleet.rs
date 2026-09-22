//! The plugin fleet's manifests: one descriptive record per cognitive unit.
//!
//! Manifests are policy, not behavior. Resource values here are the single source the
//! worker's scheduler reads. Product relationships record what each reading consumes
//! and produces — dependency edges between peers, never hierarchy — and will drive the
//! Studio's dependency-invalidation engine after its shadow-compare proof.
//!
//! Grants are deny-by-default. The web grants name the exact domain classes a plugin
//! may reach; the tool broker refuses everything else. Context requirements declare the
//! prepared materials a character's cognition needs — the "who is this / what does she
//! know" steps of the data flow, made explicit instead of adapter-implicit.

use super::plugin::{
    ModelRole, PluginId, PluginManifest, ProductKind, ProviderId, ResourceProfile, TaskKind,
    ToolGrant,
};
use super::tools::DomainClass;

/// Shared slots for models hosted on Archbox. Keep this aligned with the host's
/// `OLLAMA_NUM_PARALLEL` and configured backend concurrency.
pub const ARCHBOX_SLOTS: (&str, usize) = ("archbox-3b", 4);
/// Shared slots for models hosted on the Mac. Slot-group membership must follow routing.
pub const MAC_SLOTS: (&str, usize) = ("mac-3b", 4);

const CHARACTER_TOOLS: [ToolGrant; 3] = [
    ToolGrant::WorldRead,
    ToolGrant::Commit,
    ToolGrant::Inference,
];

const EDITOR_WEB_DOMAINS: [DomainClass; 2] = [DomainClass::NewsRss, DomainClass::CuratedArticles];
const EDITOR_TOOLS: [ToolGrant; 3] = [
    ToolGrant::WorldRead,
    ToolGrant::Commit,
    ToolGrant::WebFetch(&EDITOR_WEB_DOMAINS),
];

const INVESTIGATOR_WEB_DOMAINS: [DomainClass; 1] = [DomainClass::Wikimedia];
const INVESTIGATOR_TOOLS: [ToolGrant; 3] = [
    ToolGrant::WorldRead,
    ToolGrant::Commit,
    ToolGrant::WebFetch(&INVESTIGATOR_WEB_DOMAINS),
];

const BOXSCORE_WEB_DOMAINS: [DomainClass; 1] = [DomainClass::BoxscoreSources];
const BOXSCORE_TOOLS: [ToolGrant; 3] = [
    ToolGrant::WorldRead,
    ToolGrant::Commit,
    ToolGrant::WebFetch(&BOXSCORE_WEB_DOMAINS),
];

pub const JOURNALIST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.narrative"),
    contract_version: "narratives-v3-schema",
    tasks: &[TaskKind::NARRATIVES],
    model_roles: &[ModelRole("narrative-logic")],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::EDITOR_PACKETS,
        ProviderId::SOURCED_MEMORY,
    ],
    consumes: &[ProductKind::EDITOR_READ],
    produces: &[ProductKind::NARRATIVES],
    resources: ResourceProfile::grouped(2, MAC_SLOTS),
    tools: &CHARACTER_TOOLS,
};

pub const INFLUENCER: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.vibe"),
    contract_version: "vibe-v36",
    tasks: &[TaskKind::VIBE],
    model_roles: &[ModelRole("vibe-logic")],
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
    tools: &CHARACTER_TOOLS,
};

pub const SCOUT: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.rating"),
    contract_version: "rating-commentary-v6",
    tasks: &[TaskKind::RATING],
    model_roles: &[ModelRole("stats-logic")],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::ANALYTICS_SNAPSHOT,
        ProviderId::SOURCED_MEMORY,
    ],
    consumes: &[ProductKind::BOX_SCORE, ProductKind::IDENTITY],
    produces: &[ProductKind::RATING],
    resources: ResourceProfile::grouped(2, ARCHBOX_SLOTS),
    tools: &CHARACTER_TOOLS,
};

pub const INSIDER: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.transfers"),
    contract_version: "transfer-verdict-v1",
    tasks: &[TaskKind::TRANSFERS],
    model_roles: &[ModelRole("transfer-logic")],
    context_requirements: &[
        ProviderId::ENTITY_IDENTITY,
        ProviderId::EDITOR_PACKETS,
        ProviderId::SOURCED_MEMORY,
    ],
    consumes: &[ProductKind::EDITOR_READ, ProductKind::IDENTITY],
    produces: &[ProductKind::TRANSFERS],
    resources: ResourceProfile::unbounded_batch(1),
    tools: &CHARACTER_TOOLS,
};

pub const ANALYST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.momentum"),
    contract_version: "momentum-summary-v1",
    tasks: &[TaskKind::MOMENTUM],
    model_roles: &[ModelRole("momentum-logic")],
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
    tools: &CHARACTER_TOOLS,
};

pub const ORACLE: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.character.sigil"),
    contract_version: "oracle-reading-v2",
    tasks: &[TaskKind::SIGIL],
    model_roles: &[ModelRole("oracle-logic")],
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
    tools: &CHARACTER_TOOLS,
};

pub const EDITOR: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.editor"),
    contract_version: "ep8",
    tasks: &[TaskKind::EDITOR],
    model_roles: &[ModelRole("editor")],
    context_requirements: &[],
    consumes: &[],
    produces: &[ProductKind::EDITOR_READ],
    resources: ResourceProfile::grouped(ARCHBOX_SLOTS.1, ARCHBOX_SLOTS).batched(8),
    tools: &EDITOR_TOOLS,
};

pub const INVESTIGATOR: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.investigator"),
    contract_version: "investigate-entity-wikidata-v1",
    tasks: &[TaskKind::INVESTIGATE_ENTITY],
    model_roles: &[ModelRole("investigator")],
    context_requirements: &[],
    consumes: &[ProductKind::EDITOR_READ],
    produces: &[ProductKind::IDENTITY],
    // Wikimedia's per-domain spacing is the binding concurrency limit.
    resources: ResourceProfile::unbounded_batch(1),
    tools: &INVESTIGATOR_TOOLS,
};

pub const FIXTURE_BOXSCORE: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.fixture_boxscore"),
    contract_version: "fixture-boxscore-v1",
    tasks: &[TaskKind::FIXTURE_BOXSCORE],
    model_roles: &[],
    context_requirements: &[],
    consumes: &[],
    produces: &[ProductKind::BOX_SCORE],
    // Deterministic retrieval: no model slot, one at a time against the fetcher's
    // per-domain floor.
    resources: ResourceProfile::unbounded_batch(1),
    tools: &BOXSCORE_TOOLS,
};

pub const GRAPH: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.graph"),
    contract_version: "g5",
    tasks: &[TaskKind::GRAPH],
    model_roles: &[ModelRole("emotional-news")],
    context_requirements: &[],
    consumes: &[ProductKind::EDITOR_READ],
    produces: &[ProductKind::RELATIONS],
    resources: ResourceProfile::grouped(ARCHBOX_SLOTS.1, ARCHBOX_SLOTS).batched(8),
    tools: &CHARACTER_TOOLS,
};

/// The full first-party fleet in canonical order: the six reader-facing characters,
/// then the internal seats. Registration order in `main.rs` follows the queue's
/// dependency order instead; this list is the identity roster.
pub const ALL: [&PluginManifest; 10] = [
    &JOURNALIST,
    &INFLUENCER,
    &SCOUT,
    &INSIDER,
    &ANALYST,
    &ORACLE,
    &EDITOR,
    &INVESTIGATOR,
    &FIXTURE_BOXSCORE,
    &GRAPH,
];

#[cfg(test)]
mod tests;

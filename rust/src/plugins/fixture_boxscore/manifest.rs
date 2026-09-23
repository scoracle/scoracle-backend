//! Registration policy owned by this plugin.

use crate::application::queue::work::Stage;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};
use crate::studio::tools::DomainClass;

const BOXSCORE_WEB_DOMAINS: [DomainClass; 1] = [DomainClass::BoxscoreSources];
const BOXSCORE_TOOLS: [ToolGrant; 3] = [
    ToolGrant::WorldRead,
    ToolGrant::Commit,
    ToolGrant::WebFetch(&BOXSCORE_WEB_DOMAINS),
];

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.fixture_boxscore"),
    contract_version: "fixture-boxscore-v1",
    task: Stage::FixtureBoxscore,
    model_roles: &[],
    context_requirements: &[],
    consumes: &[],
    produces: &[ProductKind::BOX_SCORE],
    // Deterministic retrieval: no model slot, one at a time against the fetcher's
    // per-domain floor.
    resources: ResourceProfile::unbounded_batch(1),
    tools: &BOXSCORE_TOOLS,
};

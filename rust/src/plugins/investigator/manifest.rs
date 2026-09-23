//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};
use crate::studio::tools::DomainClass;

const INVESTIGATOR_WEB_DOMAINS: [DomainClass; 1] = [DomainClass::Wikimedia];
pub const TASK: TaskKey = TaskKey::new("investigate_entity");
pub const ROUTE: RouteKey = RouteKey::new("investigator", "INVESTIGATOR");
const INVESTIGATOR_TOOLS: [ToolGrant; 4] = [
    ToolGrant::WorldRead,
    ToolGrant::Commit,
    ToolGrant::WebFetch(&INVESTIGATOR_WEB_DOMAINS),
    ToolGrant::Inference,
];

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.investigator"),
    contract_version: "investigate-entity-wikidata-v1",
    task: TASK,
    claim_policy: ClaimPolicy::FIFO,
    inference_routes: &[ROUTE],
    context_requirements: &[],
    consumes: &[ProductKind::EDITOR_READ],
    produces: &[ProductKind::IDENTITY],
    // Wikimedia's per-domain spacing is the binding concurrency limit.
    resources: ResourceProfile::unbounded_batch(1),
    tools: &INVESTIGATOR_TOOLS,
};

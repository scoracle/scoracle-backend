//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::runtime::route::Role;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};
use crate::studio::tools::DomainClass;

const INVESTIGATOR_WEB_DOMAINS: [DomainClass; 1] = [DomainClass::Wikimedia];
pub const TASK: TaskKey = TaskKey::new("investigate_entity");
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
    model_roles: &[Role::Investigator],
    context_requirements: &[],
    consumes: &[ProductKind::EDITOR_READ],
    produces: &[ProductKind::IDENTITY],
    // Wikimedia's per-domain spacing is the binding concurrency limit.
    resources: ResourceProfile::unbounded_batch(1),
    tools: &INVESTIGATOR_TOOLS,
};

//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::ARCHBOX_SLOTS;
use crate::runtime::route::Role;
use crate::studio::plugin::{PluginId, PluginManifest, ProductKind, ResourceProfile, ToolGrant};
use crate::studio::tools::DomainClass;

const EDITOR_WEB_DOMAINS: [DomainClass; 2] = [DomainClass::NewsRss, DomainClass::CuratedArticles];
pub const TASK: TaskKey = TaskKey::new("editor");
const EDITOR_TOOLS: [ToolGrant; 4] = [
    ToolGrant::WorldRead,
    ToolGrant::Commit,
    ToolGrant::WebFetch(&EDITOR_WEB_DOMAINS),
    ToolGrant::Inference,
];

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.editor"),
    contract_version: "ep8",
    task: TASK,
    claim_policy: ClaimPolicy::RANKED_ARTICLES,
    model_roles: &[Role::Editor],
    context_requirements: &[],
    consumes: &[],
    produces: &[ProductKind::EDITOR_READ],
    resources: ResourceProfile::grouped(ARCHBOX_SLOTS.1, ARCHBOX_SLOTS).batched(8),
    tools: &EDITOR_TOOLS,
};

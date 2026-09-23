//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::plugins::support::resources::ARCHBOX_SLOTS;
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::studio::tools::DomainClass;

const EDITOR_WEB_DOMAINS: [DomainClass; 2] = [DomainClass::NewsRss, DomainClass::CuratedArticles];
pub const TASK: TaskKey = TaskKey::new("editor");
pub const ROUTE: RouteKey = RouteKey::new("editor", "EDITOR");
const EDITOR_TOOLS: [ToolGrant; 2] = [
    ToolGrant::WebFetch(&EDITOR_WEB_DOMAINS),
    ToolGrant::Inference,
];

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.editor"),
    task: TASK,
    claim_policy: ClaimPolicy::RANKED_ARTICLES,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::grouped(ARCHBOX_SLOTS.1, ARCHBOX_SLOTS).batched(8),
    tools: &EDITOR_TOOLS,
};

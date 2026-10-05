//! Registration policy owned by this plugin.

use crate::application::queue::work::{ClaimPolicy, TaskKey};
use crate::runtime::route::RouteKey;
use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::studio::tools::DomainClass;

const INVESTIGATOR_WEB_DOMAINS: [DomainClass; 1] = [DomainClass::Wikimedia];
pub const TASK: TaskKey = TaskKey::new("investigate_entity");
/// Retained only for historical prose evaluation; the runtime has no inference grant.
pub const ROUTE: RouteKey = RouteKey::new("investigator", "INVESTIGATOR");
const INVESTIGATOR_TOOLS: [ToolGrant; 1] = [ToolGrant::WebFetch(&INVESTIGATOR_WEB_DOMAINS)];

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.investigator"),
    task: TASK,
    claim_policy: ClaimPolicy::FIFO,
    inference_routes: &[],
    // Wikimedia's per-domain spacing is the binding concurrency limit.
    resources: ResourceProfile::unbounded_batch(1),
    tools: &INVESTIGATOR_TOOLS,
};

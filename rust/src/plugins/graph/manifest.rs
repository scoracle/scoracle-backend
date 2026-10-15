//! Registration policy owned by this plugin.

use crate::harness::fleet::MAC_SLOTS;
use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};

pub const TASK: TaskKey = TaskKey::new("graph");
pub use super::prompt::MODEL as ROUTE;

pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.graph"),
    task: TASK,
    claim_policy: ClaimPolicy::FIFO,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::grouped(MAC_SLOTS.1, MAC_SLOTS).batched(8),
    tools: &[ToolGrant::Inference],
};

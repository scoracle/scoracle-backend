//! Registration policy owned by this plugin.

use crate::harness::queue::work::{ClaimPolicy, TaskKey};
use crate::harness::skill::{ResourceProfile, SkillId, SkillManifest, ToolGrant};

pub const TASK: TaskKey = TaskKey::new("transfers");
pub use super::request::MODEL as ROUTE;

pub const MANIFEST: SkillManifest = SkillManifest {
    plugin_id: crate::harness::registration::characters::ID,
    id: SkillId::new("scoracle.character.transfers"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::grouped(1, crate::harness::fleet::MAC_SLOTS),
    tools: &[ToolGrant::Inference],
};

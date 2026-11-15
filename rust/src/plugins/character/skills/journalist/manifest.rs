//! Registration policy owned by this plugin.

use crate::harness::fleet::MAC_SLOTS;
use crate::harness::plugin::{ResourceProfile, SkillId, SkillManifest, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};

pub const TASK: TaskKey = TaskKey::new("narratives");
pub use super::request::MODEL as ROUTE;

pub const MANIFEST: SkillManifest = SkillManifest {
    plugin_id: crate::plugins::character::ID,
    id: SkillId::new("scoracle.character.narrative"),
    task: TASK,
    claim_policy: ClaimPolicy::TEAMS_FIRST,
    inference_routes: &[ROUTE],
    resources: ResourceProfile::grouped(2, MAC_SLOTS),
    tools: &[ToolGrant::Inference],
};

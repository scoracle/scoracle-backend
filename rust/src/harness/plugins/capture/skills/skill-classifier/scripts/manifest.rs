//! Headline scoring.
use crate::harness::queue::work::{ClaimPolicy, TaskKey};
use crate::harness::skill::{ResourceProfile, SkillId, SkillManifest};

pub const TASK: TaskKey = TaskKey::new("classifier");
pub const MANIFEST: SkillManifest = SkillManifest {
    plugin_id: crate::harness::plugins::capture::ID,
    id: SkillId::new("scoracle.internal.classifier"),
    task: TASK,
    claim_policy: ClaimPolicy::FRESH_ARTICLES,
    inference_routes: &[],
    resources: ResourceProfile::grouped(4, crate::harness::fleet::ARCHBOX_SLOTS).batched(4),
    tools: &[],
};

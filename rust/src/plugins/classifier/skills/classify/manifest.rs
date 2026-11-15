//! Headline scoring.
use crate::harness::plugin::{ResourceProfile, SkillId, SkillManifest};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};

pub const TASK: TaskKey = TaskKey::new("classifier");
pub const MANIFEST: SkillManifest = SkillManifest {
    plugin_id: crate::plugins::classifier::ID,
    id: SkillId::new("scoracle.internal.classifier"),
    task: TASK,
    claim_policy: ClaimPolicy::FRESH_ARTICLES,
    inference_routes: &[],
    resources: ResourceProfile::grouped(4, crate::harness::fleet::ARCHBOX_SLOTS).batched(4),
    tools: &[],
};

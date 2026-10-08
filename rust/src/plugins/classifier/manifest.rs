//! Classifier registration; acquisition and inference remain separately runnable.
use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
use crate::harness::queue::work::{ClaimPolicy, TaskKey};
use crate::harness::tools::DomainClass;

pub const TASK: TaskKey = TaskKey::new("classifier");
pub const ACQUIRE_TASK: TaskKey = TaskKey::new("classifier_acquire");
pub const MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.classifier"),
    task: TASK,
    claim_policy: ClaimPolicy::RANKED_ARTICLES,
    inference_routes: &[super::prompt::MODEL],
    resources: ResourceProfile::unbounded_batch(1),
    tools: &[ToolGrant::Inference],
};
pub const ACQUIRE_MANIFEST: PluginManifest = PluginManifest {
    id: PluginId::new("scoracle.internal.classifier.source"),
    task: ACQUIRE_TASK,
    claim_policy: ClaimPolicy::RANKED_ARTICLES,
    inference_routes: &[],
    resources: ResourceProfile::unbounded_batch(4),
    tools: &[ToolGrant::WebFetch(&[DomainClass::CuratedArticles])],
};

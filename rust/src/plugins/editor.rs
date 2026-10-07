//! System 1 article reader and downstream router. No generated prose.
pub mod prompt;

use crate::harness::plugin::{PluginManifest, PluginOutcome, StudioPlugin};
use crate::harness::queue::work::Item;
use crate::harness::tools::WebBroker;
use crate::plugins::system_one::DecisionModel;
use anyhow::Result;
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;

pub mod manifest {
    use crate::harness::plugin::{PluginId, PluginManifest, ResourceProfile, ToolGrant};
    use crate::harness::queue::work::{ClaimPolicy, TaskKey};
    use crate::harness::tools::DomainClass;
    pub const TASK: TaskKey = TaskKey::new("editor");
    pub const MANIFEST: PluginManifest = PluginManifest {
        id: PluginId::new("scoracle.internal.editor"),
        task: TASK,
        claim_policy: ClaimPolicy::RANKED_ARTICLES,
        inference_routes: &[],
        resources: ResourceProfile::unbounded_batch(4),
        tools: &[
            ToolGrant::Classification,
            ToolGrant::WebFetch(&[DomainClass::CuratedArticles]),
        ],
    };
}

pub struct EditorHandler {
    pool: PgPool,
    model: Arc<dyn DecisionModel>,
    web: Arc<WebBroker>,
}
impl EditorHandler {
    pub fn new(pool: PgPool, model: Arc<dyn DecisionModel>, web: Arc<WebBroker>) -> Self {
        Self { pool, model, web }
    }
}
#[async_trait]
impl StudioPlugin for EditorHandler {
    fn manifest(&self) -> &'static PluginManifest {
        &manifest::MANIFEST
    }
    async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
        crate::plugins::harvester::adapter::execute_editor(
            &self.pool,
            self.model.as_ref(),
            &self.web,
            self.manifest(),
            item,
        )
        .await
    }
}

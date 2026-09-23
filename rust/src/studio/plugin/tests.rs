//! Registry contract tests. Service-free: manifests and resolution only.

use super::*;
use crate::application::queue::work::{ClaimPolicy, Item, TaskKey};
use crate::studio::plugin::PluginOutcome;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

const TASK_A: TaskKey = TaskKey::new("test_task_a");
const TASK_B: TaskKey = TaskKey::new("test_task_b");
const TASK_MISSING: TaskKey = TaskKey::new("test_task_missing");

const fn manifest(id: &'static str, task: TaskKey) -> PluginManifest {
    PluginManifest {
        id: PluginId::new(id),
        task,
        claim_policy: ClaimPolicy::FIFO,
        inference_routes: &[],
        resources: ResourceProfile::unbounded_batch(1),
        tools: &[],
    }
}

fn plugin(m: &'static PluginManifest) -> Arc<dyn StudioPlugin> {
    // The dummy reads the static manifest from its own impl; the registry test only
    // needs the passed manifest to be the one served.
    Arc::new(TestPlugin(m))
}

struct TestPlugin(&'static PluginManifest);

#[async_trait]
impl StudioPlugin for TestPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        self.0
    }
    async fn execute(&self, _item: &Item) -> Result<PluginOutcome> {
        Ok(PluginOutcome::Committed)
    }
}

#[tokio::test]
async fn unrelated_task_key_registers_resolves_and_executes_without_kernel_changes() {
    const TASK: TaskKey = TaskKey::new("test_unrelated_capability");
    static MANIFEST: PluginManifest = manifest("test.unrelated", TASK);
    let registry = PluginRegistry::new(vec![plugin(&MANIFEST)]).unwrap();
    let registered = registry.resolve(TASK).expect("open task key registered");
    let outcome = registered
        .execute(&Item {
            stage: TASK,
            entity_type: "team".to_string(),
            entity_id: 1,
            sport: "TEST".to_string(),
            input_version: Some("v1".to_string()),
            attempts: 0,
            claim_token: Some("test-lease".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(outcome, PluginOutcome::Committed);
}

#[test]
fn registry_rejects_duplicate_task_ownership() {
    static A: PluginManifest = manifest("test.a", TASK_A);
    static B: PluginManifest = manifest("test.b", TASK_A);
    let err = match PluginRegistry::new(vec![plugin(&A), plugin(&B)]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate task ownership must not register"),
    };
    assert!(err.contains(TASK_A.as_str()), "{err}");
    assert!(err.contains("test.a") && err.contains("test.b"), "{err}");
}

#[test]
fn registry_rejects_duplicate_plugin_ids() {
    static A: PluginManifest = manifest("test.same", TASK_A);
    static B: PluginManifest = manifest("test.same", TASK_B);
    let err = match PluginRegistry::new(vec![plugin(&A), plugin(&B)]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate ids must not register"),
    };
    assert!(err.contains("duplicate plugin id"), "{err}");
}

#[test]
fn registry_rejects_an_empty_open_task_key() {
    static EMPTY: PluginManifest = manifest("test.empty-task", TaskKey::new(""));
    let error = PluginRegistry::new(vec![plugin(&EMPTY)])
        .err()
        .expect("empty task keys must fail registration");
    assert!(error.to_string().contains("empty task key"));
}

#[test]
fn registry_accepts_an_empty_fleet_as_the_idle_scaffold() {
    let reg = PluginRegistry::new(vec![]).unwrap();
    assert!(reg.plugins().is_empty());
}

#[test]
fn registry_resolves_every_registered_task_and_leaves_missing_tasks_unowned() {
    static A: PluginManifest = manifest("test.a", TASK_A);
    static B: PluginManifest = manifest("test.b", TASK_B);
    let reg = PluginRegistry::new(vec![plugin(&A), plugin(&B)]).unwrap();
    for m in [&A, &B] {
        assert_eq!(reg.resolve(m.task).unwrap().manifest().id, m.id);
    }
    assert!(reg.resolve(TASK_MISSING).is_none());
}

#[test]
fn registry_rejects_inconsistent_inference_declarations() {
    static ROLE_ONLY: PluginManifest = PluginManifest {
        inference_routes: &[crate::plugins::scout::manifest::ROUTE],
        ..manifest("test.role-only", TASK_A)
    };
    static GRANT_ONLY: PluginManifest = PluginManifest {
        tools: &[ToolGrant::Inference],
        ..manifest("test.grant-only", TASK_A)
    };
    for m in [&ROLE_ONLY, &GRANT_ONLY] {
        let err = PluginRegistry::new(vec![plugin(m)])
            .err()
            .expect("invalid declarations");
        assert!(err.to_string().contains(m.id.as_str()));
        assert!(err
            .to_string()
            .contains("inference roles and the inference grant together"));
    }
}

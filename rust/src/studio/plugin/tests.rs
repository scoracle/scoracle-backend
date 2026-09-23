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

const fn manifest(
    id: &'static str,
    task: TaskKey,
    produces: &'static [ProductKind],
    consumes: &'static [ProductKind],
) -> PluginManifest {
    PluginManifest {
        id: PluginId::new(id),
        contract_version: "test-v1",
        task,
        claim_policy: ClaimPolicy::FIFO,
        model_roles: &[],
        context_requirements: &[],
        consumes,
        produces,
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

#[test]
fn registry_resolves_stage_to_its_owner() {
    static M: PluginManifest = manifest("test.owner", TASK_A, &[], &[]);
    let reg = PluginRegistry::new(vec![plugin(&M)]).unwrap();
    assert!(reg.resolve(TASK_A).is_some());
    assert!(reg.resolve(TASK_B).is_none());
    assert_eq!(reg.resolve(TASK_A).unwrap().manifest().id, M.id);
}

#[tokio::test]
async fn unrelated_task_key_registers_resolves_and_executes_without_kernel_changes() {
    const TASK: TaskKey = TaskKey::new("test_unrelated_capability");
    static MANIFEST: PluginManifest = manifest("test.unrelated", TASK, &[], &[]);
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
    static A: PluginManifest = manifest("test.a", TASK_A, &[], &[]);
    static B: PluginManifest = manifest("test.b", TASK_A, &[], &[]);
    let err = match PluginRegistry::new(vec![plugin(&A), plugin(&B)]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate task ownership must not register"),
    };
    assert!(err.contains(TASK_A.as_str()), "{err}");
    assert!(err.contains("test.a") && err.contains("test.b"), "{err}");
}

#[test]
fn registry_rejects_duplicate_plugin_ids() {
    static A: PluginManifest = manifest("test.same", TASK_A, &[], &[]);
    static B: PluginManifest = manifest("test.same", TASK_B, &[], &[]);
    let err = match PluginRegistry::new(vec![plugin(&A), plugin(&B)]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate ids must not register"),
    };
    assert!(err.contains("duplicate plugin id"), "{err}");
}

#[test]
fn registry_rejects_an_empty_open_task_key() {
    static EMPTY: PluginManifest = manifest("test.empty-task", TaskKey::new(""), &[], &[]);
    let error = PluginRegistry::new(vec![plugin(&EMPTY)])
        .err()
        .expect("empty task keys must fail registration");
    assert!(error.to_string().contains("empty task key"));
}

#[test]
fn registry_accepts_an_empty_fleet_as_the_idle_scaffold() {
    let reg = PluginRegistry::new(vec![]).unwrap();
    assert!(reg.plugins().is_empty());
    assert!(reg.tasks().is_empty());
}

#[test]
fn registry_resolves_every_registered_task_and_leaves_missing_tasks_unowned() {
    static A: PluginManifest = manifest("test.a", TASK_A, &[], &[]);
    static B: PluginManifest = manifest("test.b", TASK_B, &[], &[]);
    let reg = PluginRegistry::new(vec![plugin(&A), plugin(&B)]).unwrap();
    for m in [&A, &B] {
        assert_eq!(reg.resolve(m.task).unwrap().manifest().id, m.id);
        assert_eq!(reg.manifest_for_task(m.task.as_str()).unwrap().id, m.id);
    }
    assert!(reg.resolve(TASK_MISSING).is_none());
    assert!(reg.manifest_for_task("unregistered").is_none());
}

#[test]
fn registry_rejects_inconsistent_inference_declarations() {
    static ROLE_ONLY: PluginManifest = PluginManifest {
        model_roles: &[crate::runtime::route::Role::StatsLogic],
        ..manifest("test.role-only", TASK_A, &[], &[])
    };
    static GRANT_ONLY: PluginManifest = PluginManifest {
        tools: &[ToolGrant::Inference],
        ..manifest("test.grant-only", TASK_A, &[], &[])
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

#[test]
fn manifest_owns_stage_matches_declared_tasks_only() {
    static M: PluginManifest = manifest("test.sigil", TASK_B, &[], &[]);
    assert!(M.owns_stage(TASK_B));
    assert!(!M.owns_stage(TASK_A));
}

#[test]
fn resource_profile_builders_preserve_current_cap_shapes() {
    let archbox = ("test-archbox", 4);
    let mac = ("test-mac", 4);

    let grouped = ResourceProfile::grouped(2, mac);
    assert_eq!(grouped.max_in_flight, 2);
    assert_eq!(grouped.slot_group, Some(mac));
    assert_eq!(grouped.rotation_batch, 1);

    let batched = ResourceProfile::grouped(archbox.1, archbox).batched(8);
    assert_eq!(batched.rotation_batch, 8);
    assert_eq!(batched.max_in_flight, archbox.1);

    let solo = ResourceProfile::unbounded_batch(1);
    assert_eq!(solo.slot_group, None);
    assert_eq!(solo.rotation_batch, 1);
}

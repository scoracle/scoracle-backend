//! Registry contract tests. Service-free: manifests and resolution only.

use super::*;
use crate::application::queue::work::{Item, Stage};
use crate::studio::plugin::PluginOutcome;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

const fn manifest(
    id: &'static str,
    task: Stage,
    produces: &'static [ProductKind],
    consumes: &'static [ProductKind],
) -> PluginManifest {
    PluginManifest {
        id: PluginId::new(id),
        contract_version: "test-v1",
        task,
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
    static M: PluginManifest = manifest("scoracle.character.momentum", Stage::Momentum, &[], &[]);
    let reg = PluginRegistry::new(vec![plugin(&M)]).unwrap();
    assert!(reg.resolve(Stage::Momentum).is_some());
    assert!(reg.resolve(Stage::Sigil).is_none());
    assert_eq!(reg.resolve(Stage::Momentum).unwrap().manifest().id, M.id);
}

#[test]
fn registry_rejects_duplicate_task_ownership() {
    static A: PluginManifest = manifest("test.a", Stage::Momentum, &[], &[]);
    static B: PluginManifest = manifest("test.b", Stage::Momentum, &[], &[]);
    let err = match PluginRegistry::new(vec![plugin(&A), plugin(&B)]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate task ownership must not register"),
    };
    assert!(err.contains("momentum"), "{err}");
    assert!(err.contains("test.a") && err.contains("test.b"), "{err}");
}

#[test]
fn registry_rejects_duplicate_plugin_ids() {
    static A: PluginManifest = manifest("test.same", Stage::Momentum, &[], &[]);
    static B: PluginManifest = manifest("test.same", Stage::Sigil, &[], &[]);
    let err = match PluginRegistry::new(vec![plugin(&A), plugin(&B)]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate ids must not register"),
    };
    assert!(err.contains("duplicate plugin id"), "{err}");
}

#[test]
fn registry_accepts_an_empty_fleet_as_the_idle_scaffold() {
    let reg = PluginRegistry::new(vec![]).unwrap();
    assert!(reg.plugins().is_empty());
    assert!(reg.tasks().is_empty());
}

#[test]
fn registry_resolves_every_registered_task_and_leaves_missing_tasks_unowned() {
    static A: PluginManifest = manifest("test.rating", Stage::Rating, &[], &[]);
    static B: PluginManifest = manifest("test.momentum", Stage::Momentum, &[], &[]);
    let reg = PluginRegistry::new(vec![plugin(&A), plugin(&B)]).unwrap();
    for m in [&A, &B] {
        assert_eq!(reg.resolve(m.task).unwrap().manifest().id, m.id);
        assert_eq!(reg.manifest_for_task(m.task.as_str()).unwrap().id, m.id);
    }
    assert!(reg.resolve(Stage::Sigil).is_none());
    assert!(reg.manifest_for_task("unregistered").is_none());
}

#[test]
fn registry_rejects_inconsistent_inference_declarations() {
    static ROLE_ONLY: PluginManifest = PluginManifest {
        model_roles: &[crate::runtime::route::Role::StatsLogic],
        ..manifest("test.role-only", Stage::Rating, &[], &[])
    };
    static GRANT_ONLY: PluginManifest = PluginManifest {
        tools: &[ToolGrant::Inference],
        ..manifest("test.grant-only", Stage::Rating, &[], &[])
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
    static M: PluginManifest = manifest("test.sigil", Stage::Sigil, &[], &[]);
    assert!(M.owns_stage(Stage::Sigil));
    assert!(!M.owns_stage(Stage::Momentum));
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

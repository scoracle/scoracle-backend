//! Registry contract tests. Service-free: manifests and resolution only.

use super::*;
use crate::application::queue::work::{Item, Stage};
use crate::studio::plugin::PluginOutcome;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

fn stage_of(t: TaskKind) -> Stage {
    t.stage()
}

const fn manifest(
    id: &'static str,
    tasks: &'static [TaskKind],
    produces: &'static [ProductKind],
    consumes: &'static [ProductKind],
) -> PluginManifest {
    PluginManifest {
        id: PluginId::new(id),
        contract_version: "test-v1",
        tasks,
        model_roles: &[],
        context_requirements: &[],
        consumes,
        produces,
        resources: ResourceProfile::unbounded_batch(1),
        tools: &[ToolGrant::Inference],
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

const MOM_TASKS: &[TaskKind] = &[TaskKind::MOMENTUM];
const SIGIL_TASKS: &[TaskKind] = &[TaskKind::SIGIL];

#[test]
fn task_kind_stage_roundtrip_covers_the_whole_fleet() {
    let fleet = [
        TaskKind::GRAPH,
        TaskKind::EDITOR,
        TaskKind::INVESTIGATE_ENTITY,
        TaskKind::FIXTURE_BOXSCORE,
        TaskKind::RATING,
        TaskKind::MOMENTUM,
        TaskKind::TRANSFERS,
        TaskKind::NARRATIVES,
        TaskKind::VIBE,
        TaskKind::SIGIL,
    ];
    for t in fleet {
        assert_eq!(stage_of(t).as_str(), t.as_str(), "{t} stage mismatch");
    }
}

#[test]
fn registry_resolves_stage_to_its_owner() {
    static M: PluginManifest = manifest("scoracle.character.momentum", MOM_TASKS, &[], &[]);
    let reg = PluginRegistry::new(vec![plugin(&M)]).unwrap();
    assert!(reg.resolve(Stage::Momentum).is_some());
    assert!(reg.resolve(Stage::Sigil).is_none());
    assert_eq!(reg.resolve(Stage::Momentum).unwrap().manifest().id, M.id);
}

#[test]
fn registry_rejects_duplicate_task_ownership() {
    static A: PluginManifest = manifest("test.a", MOM_TASKS, &[], &[]);
    static B: PluginManifest = manifest("test.b", MOM_TASKS, &[], &[]);
    let err = match PluginRegistry::new(vec![plugin(&A), plugin(&B)]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate task ownership must not register"),
    };
    assert!(err.contains("momentum"), "{err}");
    assert!(err.contains("test.a") && err.contains("test.b"), "{err}");
}

#[test]
fn registry_rejects_duplicate_plugin_ids() {
    static A: PluginManifest = manifest("test.same", MOM_TASKS, &[], &[]);
    static B: PluginManifest = manifest("test.same", SIGIL_TASKS, &[], &[]);
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
fn registry_accepts_a_compound_plugin_with_two_tasks() {
    static TASKS: &[TaskKind] = &[TaskKind::RATING, TaskKind::MOMENTUM];
    static M: PluginManifest = manifest("test.compound", TASKS, &[], &[]);
    let reg = PluginRegistry::new(vec![plugin(&M)]).unwrap();
    assert!(reg.resolve(Stage::Rating).is_some());
    assert!(reg.resolve(Stage::Momentum).is_some());
    assert_eq!(reg.tasks().len(), 2);
}

#[test]
fn manifest_owns_stage_matches_declared_tasks_only() {
    static M: PluginManifest = manifest("test.sigil", SIGIL_TASKS, &[], &[]);
    assert!(M.owns_stage(Stage::Sigil));
    assert!(!M.owns_stage(Stage::Momentum));
}

#[test]
fn resource_profile_builders_preserve_current_cap_shapes() {
    let archbox = crate::studio::fleet::ARCHBOX_SLOTS;
    let mac = crate::studio::fleet::MAC_SLOTS;

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

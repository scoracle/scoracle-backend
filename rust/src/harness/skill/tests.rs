//! Registry contract tests. Service-free: manifests and resolution only.

use super::*;
use crate::harness::queue::work::{ClaimPolicy, Item, TaskKey};
use crate::harness::skill::SkillOutcome;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

const TASK_A: TaskKey = TaskKey::new("test_task_a");
const TASK_B: TaskKey = TaskKey::new("test_task_b");

const fn manifest(id: &'static str, task: TaskKey) -> SkillManifest {
    SkillManifest {
        plugin_id: "test",
        id: SkillId::new(id),
        task,
        claim_policy: ClaimPolicy::FIFO,
        inference_routes: &[],
        resources: ResourceProfile::unbounded_batch(1),
        tools: &[],
    }
}

fn plugin(m: &'static SkillManifest) -> Arc<dyn Skill> {
    // The dummy reads the static manifest from its own impl; the registry test only
    // needs the passed manifest to be the one served.
    Arc::new(TestPlugin(m))
}

struct TestPlugin(&'static SkillManifest);

#[async_trait]
impl Skill for TestPlugin {
    fn manifest(&self) -> &'static SkillManifest {
        self.0
    }
    async fn execute(&self, _item: &Item) -> Result<SkillOutcome> {
        Ok(SkillOutcome::Committed)
    }
}

#[tokio::test]
async fn unrelated_task_key_registers_and_executes_without_kernel_changes() {
    const TASK: TaskKey = TaskKey::new("test_unrelated_capability");
    static MANIFEST: SkillManifest = manifest("test.unrelated", TASK);
    let registry = PluginRegistry::new(vec![test_plugin(vec![plugin(&MANIFEST)])]).unwrap();
    let registered = registry.skills()[0].clone();
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
    assert_eq!(outcome, SkillOutcome::Committed);
}

#[test]
fn registry_rejects_duplicate_task_ownership() {
    static A: SkillManifest = manifest("test.a", TASK_A);
    static B: SkillManifest = manifest("test.b", TASK_A);
    let err = match PluginRegistry::new(vec![test_plugin(vec![plugin(&A), plugin(&B)])]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate task ownership must not register"),
    };
    assert!(err.contains(TASK_A.as_str()), "{err}");
    assert!(err.contains("test.a") && err.contains("test.b"), "{err}");
}

#[test]
fn registry_rejects_duplicate_skill_ids() {
    static A: SkillManifest = manifest("test.same", TASK_A);
    static B: SkillManifest = manifest("test.same", TASK_B);
    let err = match PluginRegistry::new(vec![test_plugin(vec![plugin(&A), plugin(&B)])]) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("duplicate ids must not register"),
    };
    assert!(err.contains("duplicate skill id"), "{err}");
}

#[test]
fn registry_rejects_an_empty_open_task_key() {
    static EMPTY: SkillManifest = manifest("test.empty-task", TaskKey::new(""));
    let error = PluginRegistry::new(vec![test_plugin(vec![plugin(&EMPTY)])])
        .err()
        .expect("empty task keys must fail registration");
    assert!(error.to_string().contains("empty task key"));
}

#[test]
fn registry_accepts_an_empty_fleet_as_the_idle_scaffold() {
    let reg = PluginRegistry::new(vec![]).unwrap();
    assert!(reg.skills().is_empty());
}

#[test]
fn registry_keeps_registration_order() {
    static A: SkillManifest = manifest("test.a", TASK_A);
    static B: SkillManifest = manifest("test.b", TASK_B);
    let reg = PluginRegistry::new(vec![test_plugin(vec![plugin(&A), plugin(&B)])]).unwrap();
    let ids: Vec<_> = reg.skills().iter().map(|p| p.manifest().id).collect();
    assert_eq!(ids, [A.id, B.id]);
}

#[test]
fn registry_rejects_inconsistent_inference_declarations() {
    static ROLE_ONLY: SkillManifest = SkillManifest {
        inference_routes: &[crate::harness::plugins::characters::skills::scout::manifest::ROUTE],
        ..manifest("test.role-only", TASK_A)
    };
    static GRANT_ONLY: SkillManifest = SkillManifest {
        tools: &[ToolGrant::Inference],
        ..manifest("test.grant-only", TASK_A)
    };
    for m in [&ROLE_ONLY, &GRANT_ONLY] {
        let err = PluginRegistry::new(vec![test_plugin(vec![plugin(m)])])
            .err()
            .expect("invalid declarations");
        assert!(err.to_string().contains(m.id.as_str()));
        assert!(err
            .to_string()
            .contains("inference roles and the inference grant together"));
    }
}

/// Bind test handlers under one owner without adding a production compatibility path.
pub(crate) fn test_plugin(skills: Vec<Arc<dyn Skill>>) -> Plugin {
    let manifests = Box::leak(
        skills
            .iter()
            .map(|s| s.manifest())
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let manifest = Box::leak(Box::new(PluginManifest {
        id: "test",
        skills: manifests,
    }));
    Plugin { manifest, skills }
}

#[test]
fn registry_rejects_duplicate_plugin_owners() {
    let error = PluginRegistry::new(vec![test_plugin(vec![]), test_plugin(vec![])])
        .err()
        .unwrap();
    assert!(error.to_string().contains("duplicate plugin id"));
}

#[test]
fn registry_rejects_wrong_owner_and_empty_skill_identity() {
    static WRONG: SkillManifest = SkillManifest {
        plugin_id: "another",
        ..manifest("test.wrong", TASK_A)
    };
    static EMPTY: SkillManifest = manifest("", TASK_A);
    for (m, message) in [(&WRONG, "wrong plugin owner"), (&EMPTY, "empty skill id")] {
        let error = PluginRegistry::new(vec![test_plugin(vec![plugin(m)])])
            .err()
            .unwrap();
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn enabled_skill_cannot_change_its_declared_permissions_or_register_twice() {
    static DECLARED: SkillManifest = manifest("test.a", TASK_A);
    static ALTERED: SkillManifest = SkillManifest {
        tools: &[ToolGrant::WebFetch(&[
            crate::harness::tools::DomainClass::Wikimedia,
        ])],
        ..manifest("test.a", TASK_A)
    };
    let mut owner = test_plugin(vec![plugin(&DECLARED)]);
    owner.skills = vec![plugin(&ALTERED)];
    let error = PluginRegistry::new(vec![owner]).err().unwrap();
    assert!(error.to_string().contains("undeclared or altered"));
    let mut owner = test_plugin(vec![plugin(&DECLARED)]);
    owner.skills.push(plugin(&DECLARED));
    let error = PluginRegistry::new(vec![owner]).err().unwrap();
    assert!(error.to_string().contains("duplicate enabled skill"));
}

#[test]
fn registry_rejects_conflicting_shared_resource_budgets() {
    static A: SkillManifest = SkillManifest {
        resources: ResourceProfile::grouped(1, ("shared", 2)),
        ..manifest("test.a", TASK_A)
    };
    static B: SkillManifest = SkillManifest {
        resources: ResourceProfile::grouped(1, ("shared", 4)),
        ..manifest("test.b", TASK_B)
    };
    let error = PluginRegistry::new(vec![test_plugin(vec![plugin(&A), plugin(&B)])])
        .err()
        .unwrap();
    assert!(error.to_string().contains("inconsistent budget"));
}

#[test]
fn capture_declares_three_skills_and_schedules_only_classifier() {
    use crate::harness::plugins::capture;
    assert_eq!(capture::MANIFEST.skills.len(), 3);
    let active: Vec<_> = crate::harness::fleet::skills()
        .filter(|s| s.plugin_id == capture::ID)
        .collect();
    assert_eq!(active, [&capture::skills::classifier::manifest::MANIFEST]);
}

#[test]
fn registry_rejects_a_slot_group_that_cannot_admit_work() {
    static ZERO: SkillManifest = SkillManifest {
        resources: ResourceProfile::grouped(1, ("blocked", 0)),
        ..manifest("test.zero", TASK_A)
    };
    let error = PluginRegistry::new(vec![test_plugin(vec![plugin(&ZERO)])])
        .err()
        .unwrap();
    assert!(error
        .to_string()
        .contains("slot group blocked must have a positive budget"));
}

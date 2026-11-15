//! Production fleet contracts: identifiers, resources, grants and registration.

use crate::plugins::caretaker::skills::fixture_boxscore::manifest::MANIFEST as FIXTURE_BOXSCORE;
use crate::plugins::caretaker::skills::investigator::manifest::MANIFEST as INVESTIGATOR;
use crate::plugins::character::skills::analyst::manifest::MANIFEST as ANALYST;
use crate::plugins::character::skills::influencer::manifest::MANIFEST as INFLUENCER;
use crate::plugins::character::skills::insider::manifest::MANIFEST as INSIDER;
use crate::plugins::character::skills::journalist::manifest::MANIFEST as JOURNALIST;
use crate::plugins::character::skills::oracle::manifest::MANIFEST as ORACLE;
use crate::plugins::character::skills::scout::manifest::MANIFEST as SCOUT;
use crate::plugins::classifier::skills::classify::manifest::MANIFEST as CLASSIFIER;

use super::*;
use crate::harness::plugin::{PluginRegistry, Skill, SkillManifest, SkillOutcome, ToolGrant};
use crate::harness::tools::DomainClass;

#[test]
fn production_resource_policy_matches_the_deployment_contract() {
    let archbox = crate::harness::fleet::ARCHBOX_SLOTS;
    let mac = crate::harness::fleet::MAC_SLOTS;

    assert_eq!(
        (
            JOURNALIST.resources.max_in_flight,
            JOURNALIST.resources.slot_group
        ),
        (2, Some(mac))
    );
    assert_eq!(
        (
            INFLUENCER.resources.max_in_flight,
            INFLUENCER.resources.slot_group
        ),
        (1, Some(mac))
    );
    assert_eq!(
        (SCOUT.resources.max_in_flight, SCOUT.resources.slot_group),
        (2, Some(mac))
    );
    assert_eq!(
        (ORACLE.resources.max_in_flight, ORACLE.resources.slot_group),
        (2, Some(mac))
    );
    assert_eq!(
        (
            INSIDER.resources.max_in_flight,
            INSIDER.resources.slot_group
        ),
        (1, Some(mac))
    );
    assert_eq!(
        (
            ANALYST.resources.max_in_flight,
            ANALYST.resources.slot_group
        ),
        (1, Some(mac))
    );
    assert_eq!(
        (
            CLASSIFIER.resources.max_in_flight,
            CLASSIFIER.resources.rotation_batch,
            CLASSIFIER.resources.slot_group
        ),
        (4, 4, Some(archbox))
    );
    assert_eq!(
        (
            INVESTIGATOR.resources.max_in_flight,
            INVESTIGATOR.resources.slot_group
        ),
        (1, None)
    );
    assert_eq!(
        (
            FIXTURE_BOXSCORE.resources.max_in_flight,
            FIXTURE_BOXSCORE.resources.slot_group
        ),
        (1, None)
    );
}

#[test]
fn durable_task_identifiers_remain_compatible() {
    assert_eq!(JOURNALIST.task.as_str(), "narratives");
    assert_eq!(INFLUENCER.task.as_str(), "vibe");
    assert_eq!(SCOUT.task.as_str(), "rating");
    assert_eq!(INSIDER.task.as_str(), "transfers");
    assert_eq!(ANALYST.task.as_str(), "momentum");
    assert_eq!(ORACLE.task.as_str(), "sigil");
    assert_eq!(INVESTIGATOR.task.as_str(), "investigate_entity");
    assert_eq!(FIXTURE_BOXSCORE.task.as_str(), "fixture_boxscore");
}

#[test]
fn acquisition_web_grants_are_scoped_to_each_plugins_sources() {
    assert!(!CLASSIFIER
        .tools
        .iter()
        .any(|g| matches!(g, ToolGrant::WebFetch(_))));

    assert!(INVESTIGATOR.inference_routes.is_empty());
    assert!(!INVESTIGATOR.tools.contains(&ToolGrant::Inference));
    assert!(INVESTIGATOR.grants_web(DomainClass::Wikimedia));
    assert!(!INVESTIGATOR.grants_web(DomainClass::NewsRss));
    assert!(!INVESTIGATOR.grants_web(DomainClass::BoxscoreSources));

    assert!(FIXTURE_BOXSCORE.grants_web(DomainClass::BoxscoreSources));
    assert!(!FIXTURE_BOXSCORE.grants_web(DomainClass::Wikimedia));
    assert!(!FIXTURE_BOXSCORE.grants_web(DomainClass::NewsRss));
    assert!(!FIXTURE_BOXSCORE.tools.contains(&ToolGrant::Inference));
}

#[test]
fn full_and_partial_production_fleets_register() {
    let plugins = ALL
        .iter()
        .map(|manifest| crate::harness::plugin::Plugin {
            manifest,
            skills: if manifest.id == crate::plugins::caretaker::MANIFEST.id {
                vec![]
            } else {
                manifest.skills.iter().map(|m| plugin(m)).collect()
            },
        })
        .collect();
    let registry = PluginRegistry::new(plugins).unwrap();
    assert_eq!(registry.plugins().len(), 3);
    assert_eq!(registry.skills().len(), 7);
    assert!(registry.plugins()[2].skills.is_empty());
    for manifest in crate::harness::fleet::skills() {
        let owner = ALL.iter().find(|p| p.id == manifest.plugin_id).unwrap();
        let partial = PluginRegistry::new(vec![crate::harness::plugin::Plugin {
            manifest: owner,
            skills: vec![plugin(manifest)],
        }])
        .unwrap();
        assert_eq!(partial.skills()[0].manifest().id, manifest.id);
    }
}

struct ManifestPlugin(&'static SkillManifest);

#[async_trait::async_trait]
impl Skill for ManifestPlugin {
    fn manifest(&self) -> &'static SkillManifest {
        self.0
    }

    async fn execute(&self, _: &crate::harness::queue::work::Item) -> anyhow::Result<SkillOutcome> {
        unreachable!("registration-only test")
    }
}

fn plugin(manifest: &'static SkillManifest) -> std::sync::Arc<dyn Skill> {
    std::sync::Arc::new(ManifestPlugin(manifest))
}

#[test]
fn classifier_replaces_retired_intake_in_the_deployable_fleet() {
    assert_eq!(CLASSIFIER.task.as_str(), "classifier");
    assert!(!skills().any(|m| matches!(
        m.task.as_str(),
        "harvester" | "editor" | "graph" | "classifier_acquire"
    )));
    assert!(inference_routes()
        .iter()
        .all(|r| r.env_suffix() != "EMOTIONAL_NEWS"));
    assert!(skills().any(|m| m.id == CLASSIFIER.id));
    assert!(crate::harness::registration::enabled_from_config(Some("harvester,editor")).is_err());
    assert!(!crate::harness::registration::enabled_from_config(None)
        .unwrap()
        .contains("classifier"));
}

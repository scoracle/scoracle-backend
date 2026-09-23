//! Production fleet contract tests. These protect durable identifiers, operational
//! resource policy, scoped web reach, and complete boot registration.

use super::*;
use crate::studio::plugin::{
    PluginManifest, PluginOutcome, PluginRegistry, StudioPlugin, ToolGrant,
};
use crate::studio::tools::DomainClass;

#[test]
fn production_resource_policy_matches_the_deployment_contract() {
    let archbox = crate::plugins::support::resources::ARCHBOX_SLOTS;
    let mac = crate::plugins::support::resources::MAC_SLOTS;

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
        (2, Some(archbox))
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
        (1, None)
    );
    assert_eq!(
        (
            ANALYST.resources.max_in_flight,
            ANALYST.resources.slot_group
        ),
        (1, None)
    );
    assert_eq!(
        (EDITOR.resources.rotation_batch, EDITOR.resources.slot_group),
        (8, Some(archbox))
    );
    assert_eq!(
        (GRAPH.resources.rotation_batch, GRAPH.resources.slot_group),
        (8, Some(archbox))
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
    assert_eq!(EDITOR.task.as_str(), "editor");
    assert_eq!(INVESTIGATOR.task.as_str(), "investigate_entity");
    assert_eq!(FIXTURE_BOXSCORE.task.as_str(), "fixture_boxscore");
    assert_eq!(GRAPH.task.as_str(), "graph");
}

#[test]
fn acquisition_web_grants_are_scoped_to_each_plugins_sources() {
    assert!(EDITOR.grants_web(DomainClass::NewsRss));
    assert!(EDITOR.grants_web(DomainClass::CuratedArticles));
    assert!(!EDITOR.grants_web(DomainClass::Wikimedia));
    assert!(!EDITOR.grants_web(DomainClass::BoxscoreSources));

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
    let fleet = ALL;
    let registry =
        PluginRegistry::new(fleet.iter().map(|manifest| plugin(manifest)).collect()).unwrap();
    assert_eq!(registry.plugins().len(), fleet.len());
    for manifest in fleet {
        assert_eq!(
            registry.resolve(manifest.task).unwrap().manifest().id,
            manifest.id
        );
        let partial = PluginRegistry::new(vec![plugin(manifest)]).unwrap();
        assert_eq!(
            partial.resolve(manifest.task).unwrap().manifest().id,
            manifest.id
        );
    }
}

struct ManifestPlugin(&'static PluginManifest);

#[async_trait::async_trait]
impl StudioPlugin for ManifestPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        self.0
    }

    async fn execute(
        &self,
        _: &crate::application::queue::work::Item,
    ) -> anyhow::Result<PluginOutcome> {
        unreachable!("registration-only test")
    }
}

fn plugin(manifest: &'static PluginManifest) -> std::sync::Arc<dyn StudioPlugin> {
    std::sync::Arc::new(ManifestPlugin(manifest))
}

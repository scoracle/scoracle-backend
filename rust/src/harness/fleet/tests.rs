//! Production fleet contract tests. These protect durable identifiers, operational
//! resource policy, scoped web reach, and complete boot registration.

use super::*;
use crate::harness::plugin::{
    PluginManifest, PluginOutcome, PluginRegistry, StudioPlugin, ToolGrant,
};
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
    assert_eq!(INVESTIGATOR.task.as_str(), "investigate_entity");
    assert_eq!(FIXTURE_BOXSCORE.task.as_str(), "fixture_boxscore");
    assert_eq!(GRAPH.task.as_str(), "graph");
}

#[test]
fn acquisition_web_grants_are_scoped_to_each_plugins_sources() {
    assert!(HARVESTER.grants_web(DomainClass::CuratedArticles));
    assert!(!HARVESTER.grants_web(DomainClass::NewsRss));
    assert!(!HARVESTER.grants_web(DomainClass::Wikimedia));
    assert!(!HARVESTER.grants_web(DomainClass::BoxscoreSources));

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
    let fleet = ALL;
    let registry =
        PluginRegistry::new(fleet.iter().map(|manifest| plugin(manifest)).collect()).unwrap();
    assert_eq!(registry.plugins().len(), fleet.len());
    let ids: Vec<_> = registry.plugins().iter().map(|p| p.manifest().id).collect();
    assert_eq!(
        ids,
        fleet.iter().map(|m| m.id).collect::<Vec<_>>(),
        "registration order must be preserved"
    );
    // Every fleet member also registers on its own (a partial deployment).
    for manifest in fleet {
        let partial = PluginRegistry::new(vec![plugin(manifest)]).unwrap();
        assert_eq!(partial.plugins()[0].manifest().id, manifest.id);
    }
}

#[test]
fn harvester_has_independent_identity_and_requires_explicit_enablement() {
    assert_eq!(HARVESTER.task.as_str(), "harvester");
    assert_ne!(HARVESTER.id.as_str(), "scoracle.internal.editor");
    assert!(!ALL.iter().any(|m| m.task.as_str() == "editor"));
    assert!(!inference_routes()
        .iter()
        .any(|route| route.as_str() == "editor"));
    assert!(HARVESTER.tools.contains(&ToolGrant::Classification));
    assert!(HARVESTER.grants_web(DomainClass::CuratedArticles));
    assert!(HARVESTER.inference_routes.is_empty());
    assert_eq!(HARVESTER.resources.max_in_flight, 4);
    assert_eq!(HARVESTER.resources.slot_group, None);
    assert!(ALL.iter().any(|m| m.id == HARVESTER.id));
    assert!(!crate::harness::registration::enabled_from_config(None)
        .unwrap()
        .contains("harvester"));
    assert!(crate::harness::registration::enabled_from_config(Some("harvester")).is_ok());
    PluginRegistry::new(vec![plugin(&HARVESTER)]).unwrap();
}

struct ManifestPlugin(&'static PluginManifest);

#[async_trait::async_trait]
impl StudioPlugin for ManifestPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        self.0
    }

    async fn execute(
        &self,
        _: &crate::harness::queue::work::Item,
    ) -> anyhow::Result<PluginOutcome> {
        unreachable!("registration-only test")
    }
}

fn plugin(manifest: &'static PluginManifest) -> std::sync::Arc<dyn StudioPlugin> {
    std::sync::Arc::new(ManifestPlugin(manifest))
}

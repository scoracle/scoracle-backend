//! Fleet manifest contract tests: caps mirror production, products form the reading
//! graph, and every role name matches its router key.

use super::*;
use crate::studio::plugin::{ProductKind, ProviderId, ToolGrant};
use crate::studio::tools::DomainClass;

#[test]
fn character_resources_mirror_the_caps_the_adapters_enforce() {
    let archbox = crate::plugins::support::resources::ARCHBOX_SLOTS;
    let mac = crate::plugins::support::resources::MAC_SLOTS;

    assert_eq!(JOURNALIST.resources.max_in_flight, 2);
    assert_eq!(JOURNALIST.resources.slot_group, Some(mac));
    assert_eq!(INFLUENCER.resources.max_in_flight, 1);
    assert_eq!(INFLUENCER.resources.slot_group, Some(mac));
    assert_eq!(SCOUT.resources.max_in_flight, 2);
    assert_eq!(SCOUT.resources.slot_group, Some(archbox));
    assert_eq!(ORACLE.resources.max_in_flight, 2);
    assert_eq!(ORACLE.resources.slot_group, Some(mac));

    // Insider and Analyst run single-slot and ungrouped (their own host governor binds).
    assert_eq!(INSIDER.resources.slot_group, None);
    assert_eq!(INSIDER.resources.max_in_flight, 1);
    assert_eq!(ANALYST.resources.slot_group, None);
    assert_eq!(ANALYST.resources.max_in_flight, 1);

    // Internal seats keep their batching and slot shapes.
    assert_eq!(EDITOR.resources.rotation_batch, 8);
    assert_eq!(EDITOR.resources.slot_group, Some(archbox));
    assert_eq!(GRAPH.resources.rotation_batch, 8);
    assert_eq!(GRAPH.resources.slot_group, Some(archbox));
    assert_eq!(INVESTIGATOR.resources.slot_group, None);
    assert_eq!(INVESTIGATOR.resources.max_in_flight, 1);
    assert_eq!(FIXTURE_BOXSCORE.resources.slot_group, None);
    assert_eq!(FIXTURE_BOXSCORE.resources.max_in_flight, 1);
    assert_eq!(FIXTURE_BOXSCORE.resources.rotation_batch, 1);
}

#[test]
fn every_character_consumes_the_cards_it_reads() {
    // The Analyst reads finished Rating and Vibe cards, not raw rails.
    assert_eq!(
        ANALYST.consumes,
        &[ProductKind::RATING, ProductKind::VIBE][..]
    );
    // The Oracle reads the five finished cards only.
    assert_eq!(
        ORACLE.consumes,
        &[
            ProductKind::NARRATIVES,
            ProductKind::RATING,
            ProductKind::VIBE,
            ProductKind::MOMENTUM,
            ProductKind::TRANSFERS,
        ][..]
    );
    // Each produces exactly its own reading.
    assert_eq!(ANALYST.produces, &[ProductKind::MOMENTUM][..]);
    assert_eq!(ORACLE.produces, &[ProductKind::SIGIL][..]);
    assert_eq!(JOURNALIST.produces, &[ProductKind::NARRATIVES][..]);
    assert_eq!(SCOUT.produces, &[ProductKind::RATING][..]);
    assert_eq!(INFLUENCER.produces, &[ProductKind::VIBE][..]);
    assert_eq!(INSIDER.produces, &[ProductKind::TRANSFERS][..]);
}

#[test]
fn task_kinds_map_onto_their_durable_stages() {
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
fn contract_versions_match_the_studio_owners() {
    assert_eq!(
        JOURNALIST.contract_version,
        crate::plugins::journalist::cognition::NARRATIVES_OUTPUT_CONTRACT_VERSION
    );
    assert_eq!(
        SCOUT.contract_version,
        crate::plugins::scout::cognition::RATING_OUTPUT_CONTRACT_VERSION
    );
    assert_eq!(
        ANALYST.contract_version,
        crate::plugins::analyst::cognition::MOMENTUM_OUTPUT_CONTRACT_VERSION
    );
    assert_eq!(
        ORACLE.contract_version,
        crate::plugins::oracle::cognition::ORACLE_OUTPUT_CONTRACT_VERSION
    );
    assert_eq!(
        INSIDER.contract_version,
        crate::plugins::insider::cognition::TRANSFER_OUTPUT_CONTRACT_VERSION
    );
    assert_eq!(
        EDITOR.contract_version,
        crate::plugins::editor::cognition::EDITOR_CONTRACT_VERSION
    );
    assert_eq!(
        GRAPH.contract_version,
        crate::plugins::graph::cognition::prompt::GRAPH_PROMPT_VERSION
    );
}

#[test]
fn fixture_boxscore_declares_no_inference_grant() {
    assert!(!FIXTURE_BOXSCORE.tools.contains(&ToolGrant::Inference));
    assert!(FIXTURE_BOXSCORE
        .tools
        .iter()
        .any(|g| matches!(g, ToolGrant::WebFetch(_))));
}

#[test]
fn only_acquisition_seats_carry_a_web_grant() {
    for (m, web) in [
        (JOURNALIST, false),
        (INFLUENCER, false),
        (SCOUT, false),
        (INSIDER, false),
        (ANALYST, false),
        (ORACLE, false),
        (GRAPH, false),
        (EDITOR, true),
        (INVESTIGATOR, true),
        (FIXTURE_BOXSCORE, true),
    ] {
        assert_eq!(
            m.tools.iter().any(|g| matches!(g, ToolGrant::WebFetch(_))),
            web,
            "{}",
            m.id
        );
    }
}

#[test]
fn investigator_web_grants_include_wikimedia() {
    // The Investigator is the internet-search exemplar: its discovery calls reach
    // wikidata/wikipedia, and the broker refuses everything outside its declared set.
    let domains: Vec<DomainClass> = INVESTIGATOR
        .tools
        .iter()
        .filter_map(|g| match g {
            ToolGrant::WebFetch(domains) => Some(*domains),
            _ => None,
        })
        .flatten()
        .copied()
        .collect();
    assert!(domains.contains(&DomainClass::Wikimedia));
    assert!(INVESTIGATOR.grants_web(DomainClass::Wikimedia));
}

#[test]
fn acquisition_web_grants_are_scoped_to_the_seats_actual_skill() {
    // The room brokers web access. Seats declare only the source class their work
    // needs; a broad shared acquisition grant would let one seat quietly become a
    // second harness with unrelated reach.
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
}

#[test]
fn characters_declare_the_context_their_flow_needs() {
    // The Analyst reads identity, season, the finished Rating and Vibe cards, its
    // deterministic snapshot, and sourced memory — never raw articles.
    assert_eq!(
        ANALYST.context_requirements,
        &[
            ProviderId::ENTITY_IDENTITY,
            ProviderId::CURRENT_SEASON,
            ProviderId::PILLAR_CARDS,
            ProviderId::MOMENTUM_SNAPSHOT,
            ProviderId::SOURCED_MEMORY,
        ][..]
    );
    assert_eq!(
        ORACLE.context_requirements,
        &[
            ProviderId::ENTITY_IDENTITY,
            ProviderId::CURRENT_SEASON,
            ProviderId::PILLAR_CARDS,
        ][..]
    );
    // The Oracle never expands articles: she reads finished cards only.
    assert!(!ORACLE
        .context_requirements
        .contains(&ProviderId::ARTICLE_EXPANSION));
}

#[test]
fn internal_seats_never_produce_reader_facing_cards() {
    for m in [EDITOR, INVESTIGATOR, FIXTURE_BOXSCORE, GRAPH] {
        for product in m.produces {
            assert_ne!(*product, ProductKind::NARRATIVES);
            assert_ne!(*product, ProductKind::RATING);
            assert_ne!(*product, ProductKind::VIBE);
            assert_ne!(*product, ProductKind::TRANSFERS);
            assert_ne!(*product, ProductKind::MOMENTUM);
            assert_ne!(*product, ProductKind::SIGIL);
        }
    }
}

#[test]
fn full_and_partial_production_fleets_register() {
    let fleet = ALL;
    let registry = PluginRegistry::new(fleet.iter().map(|m| plugin(m)).collect()).unwrap();
    assert_eq!(registry.plugins().len(), fleet.len());
    for m in fleet {
        assert_eq!(registry.resolve(m.task).unwrap().manifest().id, m.id);
        let partial = PluginRegistry::new(vec![plugin(m)]).unwrap();
        assert_eq!(partial.resolve(m.task).unwrap().manifest().id, m.id);
    }
}

// Exercise the real roster through registration without constructing live dependencies.
use crate::studio::plugin::{PluginManifest, PluginOutcome, PluginRegistry, StudioPlugin};
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
fn plugin(m: &'static PluginManifest) -> std::sync::Arc<dyn StudioPlugin> {
    std::sync::Arc::new(ManifestPlugin(m))
}

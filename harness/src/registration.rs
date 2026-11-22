//! Concrete application assembly for the Studio plugin fleet.
//!
//! Studio owns the plugin contract; each plugin owns its manifest and adapters.
//! The application binds plugins to Postgres and model routing. Keeping this assembly
//! here leaves `main.rs` as process boot and makes the adapter boundary explicit.

pub mod capture;
pub mod characters;

use crate::harness::models::Models;
use crate::harness::skills::analyst;
use crate::harness::skills::oracle;
use crate::harness::skills::scout;
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;

/// Resolve `COGNITION_STAGES` against the registered Studio fleet.
///
/// An unset value enables the character skills; Classifier remains opt-in. An
/// explicitly empty value remains an idle fleet, which is useful for a process that
/// only runs its outbox or recovery loops.
pub fn enabled_from_config(raw: Option<&str>) -> Result<HashSet<String>> {
    let known = known_stages();
    let Some(raw) = raw else {
        // New schema, model endpoint, and character delivery are deployed in
        // stages. Intake replacements require explicit enablement while cognition is paused.
        return Ok(known
            .into_iter()
            .filter(|s| *s != "classifier")
            .map(str::to_owned)
            .collect());
    };

    let mut stages = HashSet::new();
    let mut unknown = Vec::new();
    for stage in raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
    {
        if known.contains(&stage.as_str()) {
            stages.insert(stage);
        } else {
            unknown.push(stage);
        }
    }
    if !unknown.is_empty() {
        return Err(anyhow!(
            "unknown COGNITION_STAGES value(s): {}; allowed: {}",
            unknown.join(","),
            known.join(",")
        ));
    }
    Ok(stages)
}

/// Task names are durable queue vocabulary, derived from the manifest roster rather
/// than duplicated in service configuration.
fn known_stages() -> Vec<&'static str> {
    let mut stages: Vec<&'static str> = crate::harness::fleet::skills()
        .map(|manifest| manifest.task.as_str())
        .collect();
    stages.sort_unstable();
    stages.dedup();
    stages
}

/// Construct the enabled first-party fleet with application dependencies bound.
///
/// Registration order is deliberate: intake precedes the voice stages, which
/// follow their product dependency order. Scheduling caps and task ownership
/// still come from each skill's manifest once the fleet reaches the worker.
pub fn build(
    pool: PgPool,
    models: Arc<Models>,
    enabled: &HashSet<String>,
) -> Result<Vec<crate::harness::skill::Plugin>> {
    Ok(vec![
        crate::harness::registration::capture::build(pool.clone(), enabled)?,
        crate::harness::registration::characters::build(pool, &models, enabled)?,
    ])
}

/// Assemble complete durable reaction chains from the statically linked fleet.
/// Reactions are independent of this process's enabled task handlers: any host may
/// durably enqueue work for a stage that another worker process executes.
pub fn build_reactions(pool: PgPool) -> Result<crate::harness::queue::outbox::ReactionRegistry> {
    use crate::harness::queue::outbox::EventReaction;

    let reactions: Vec<Arc<dyn EventReaction>> = vec![
        Arc::new(analyst::MomentumReaction::new(pool.clone())),
        Arc::new(oracle::OracleBarrierReaction::new(pool.clone())),
        Arc::new(scout::RatingIdentityReaction::new(pool)),
    ];
    crate::harness::queue::outbox::ReactionRegistry::new(reactions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::skill::PluginRegistry;

    #[test]
    fn unset_configuration_keeps_classifier_opt_in() {
        let stages = enabled_from_config(None).unwrap();
        assert_eq!(stages.len() + 1, known_stages().len());
        for stage in known_stages() {
            assert_eq!(stages.contains(stage), stage != "classifier");
        }
        assert!(enabled_from_config(Some("harvester,editor")).is_err());
        assert!(enabled_from_config(Some("classifier_acquire")).is_err());
    }

    #[test]
    fn configured_stages_normalize_and_dedupe() {
        let stages =
            enabled_from_config(Some(" narratives, rating, momentum, vibe, VIBE ,,sigil "))
                .unwrap();
        assert_eq!(stages.len(), 5);
        assert!(!stages.contains("editor"));
        assert!(stages.contains("narratives"));
        assert!(enabled_from_config(Some("fixture_boxscore,investigate_entity")).is_err());
        assert!(stages.contains("rating"));
        assert!(stages.contains("momentum"));
        assert!(stages.contains("vibe"));
        assert!(stages.contains("sigil"));
    }

    #[test]
    fn configured_stages_reject_unknown_values() {
        let err = enabled_from_config(Some("graph,headlinez,scrub,oracle,editor"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("graph"));
        assert!(err.contains("headlinez"));
        assert!(!err.contains("unknown COGNITION_STAGES value(s): editor"));
        assert!(err.contains("scrub"));
        assert!(err.contains("oracle"));
        assert!(err.contains("narratives"));
    }

    #[test]
    fn explicit_empty_configuration_is_an_idle_fleet() {
        assert!(enabled_from_config(Some("  ")).unwrap().is_empty());
    }

    #[test]
    fn composition_registers_consumers_after_their_producers() {
        let position = |stage| {
            crate::harness::registration::characters::MANIFEST
                .skills
                .iter()
                .position(|candidate| candidate.task == stage)
                .unwrap()
        };
        assert!(
            position(crate::harness::skills::analyst::manifest::TASK)
                > position(crate::harness::skills::scout::manifest::TASK)
        );
        assert!(
            position(crate::harness::skills::analyst::manifest::TASK)
                > position(crate::harness::skills::influencer::manifest::TASK)
        );
        assert_eq!(
            position(crate::harness::skills::oracle::manifest::TASK),
            crate::harness::registration::characters::MANIFEST
                .skills
                .len()
                - 1
        );
    }

    #[tokio::test]
    async fn reactions_register_complete_chains_independently_of_task_handlers() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://localhost/unused")
            .unwrap();
        let complete = build_reactions(pool).unwrap();
        let oracle_only = ["oracle.completion-barrier"];
        for kind in [
            crate::harness::skills::analyst::MOMENTUM_COMPLETED,
            crate::harness::skills::scout::RATING_DEBOUNCED,
            crate::harness::skills::journalist::NARRATIVES_COMPLETED,
            crate::harness::skills::insider::TRANSFER_PUBLISHED,
        ] {
            assert_eq!(complete.reaction_names(kind), oracle_only, "{kind}");
        }
        let momentum_then_oracle = ["analyst.enqueue-momentum", "oracle.completion-barrier"];
        for kind in [
            crate::harness::skills::influencer::VIBE_COMPLETED,
            crate::harness::skills::scout::RATING_COMPLETED,
        ] {
            assert_eq!(
                complete.reaction_names(kind),
                momentum_then_oracle,
                "{kind}"
            );
        }
        assert_eq!(
            complete.reaction_names(crate::harness::skills::scout::TRANSFER_IDENTITY_APPLIED),
            ["scout.rate-applied-identity"]
        );
    }

    #[tokio::test]
    async fn real_character_assembly_preserves_partial_and_idle_fleets() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://localhost/unused")
            .unwrap();
        let models = Arc::new(crate::harness::models::tests::models());
        for (config, expected) in [
            (Some(""), vec![]),
            (Some("momentum,narratives"), vec!["narratives", "momentum"]),
            (
                None,
                vec![
                    "narratives",
                    "vibe",
                    "rating",
                    "transfers",
                    "momentum",
                    "sigil",
                ],
            ),
        ] {
            let enabled = enabled_from_config(config).unwrap();
            let registry =
                PluginRegistry::new(build(pool.clone(), models.clone(), &enabled).unwrap())
                    .unwrap();
            assert_eq!(registry.plugins().len(), 2);
            assert_eq!(
                registry
                    .skills()
                    .iter()
                    .map(|s| s.manifest().task.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(registry.plugins()[0].skills.is_empty());
            assert!(registry
                .skills()
                .iter()
                .all(|s| s.manifest().plugin_id == crate::harness::registration::characters::ID));
        }
    }
}

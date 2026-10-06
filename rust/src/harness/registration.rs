//! Concrete application assembly for the Studio plugin fleet.
//!
//! Studio owns the plugin contract; each plugin owns its manifest and adapters.
//! The application binds plugins to Postgres and model routing. Keeping this assembly
//! here leaves `main.rs` as process boot and makes the adapter boundary explicit.

use crate::harness::models::Models;
use crate::harness::plugin::StudioPlugin;
use crate::harness::queue::work;
use crate::harness::tools::WebBroker;
use crate::plugins::analyst;
use crate::plugins::fixture_boxscore::adapter as boxscore;
use crate::plugins::graph::adapter as graph;
use crate::plugins::harvester::adapter as harvester;
use crate::plugins::influencer;
use crate::plugins::insider;
use crate::plugins::journalist;
use crate::plugins::oracle;
use crate::plugins::scout;
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;

/// First-party registration order. This is composition policy, not a durable
/// queue invariant; claim-time database gates remain authoritative across workers.
const VOICE_ORDER: [work::TaskKey; 6] = [
    crate::plugins::journalist::manifest::TASK,
    crate::plugins::influencer::manifest::TASK,
    crate::plugins::scout::manifest::TASK,
    crate::plugins::insider::manifest::TASK,
    crate::plugins::analyst::manifest::TASK,
    crate::plugins::oracle::manifest::TASK,
];

/// Resolve `COGNITION_STAGES` against the registered Studio fleet.
///
/// An unset value means every task currently declared by a first-party manifest. An
/// explicitly empty value remains an idle fleet, which is useful for a process that
/// only runs its outbox or recovery loops.
pub fn enabled_from_config(raw: Option<&str>) -> Result<HashSet<String>> {
    let known = known_stages();
    let Some(raw) = raw else {
        // New schema, model endpoint, and character delivery are deployed in
        // stages. A host must opt in to the Harvester worker explicitly.
        return Ok(known
            .into_iter()
            .filter(|s| *s != "harvester")
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
    let mut stages: Vec<&'static str> = crate::harness::fleet::ALL
        .iter()
        .map(|manifest| manifest.task.as_str())
        .collect();
    stages.sort_unstable();
    stages.dedup();
    stages
}

/// Construct the enabled first-party fleet with application dependencies bound.
///
/// Registration order is deliberate: Graph uses the acquisition rail, and the
/// voice stages follow their product dependency order. Scheduling caps and task ownership
/// still come from each plugin's Studio manifest once the fleet reaches the worker.
pub fn build(
    pool: PgPool,
    models: Arc<Models>,
    enabled: &HashSet<String>,
) -> Result<Vec<Arc<dyn StudioPlugin>>> {
    let mut handlers: Vec<Arc<dyn StudioPlugin>> = Vec::new();
    let mut web_workspace: Option<Arc<WebBroker>> = None;

    // Graph reviews publisher text without Editor-derived evidence.
    if enabled.contains("graph") {
        handlers.push(Arc::new(graph::GraphHandler::new(
            pool.clone(),
            models.capabilities(&crate::plugins::graph::manifest::MANIFEST)?,
        )));
    }
    if enabled.contains("harvester") {
        let endpoint = std::env::var("HARVESTER_MODEL_ENDPOINT").map_err(|_| {
            anyhow!("HARVESTER_MODEL_ENDPOINT is required when harvester is enabled")
        })?;
        let model = crate::harness::providers::system_one::SystemOneClient::new(endpoint)?;
        let web = shared_web_workspace(&mut web_workspace)?;
        handlers.push(Arc::new(harvester::HarvesterHandler::new(
            pool.clone(),
            Arc::new(model),
            web,
        )));
    }
    // Structured discovery uses only the scoped web broker.
    if enabled.contains("investigate_entity") {
        let web = shared_web_workspace(&mut web_workspace)?;
        handlers.push(Arc::new(
            crate::plugins::investigator::adapter::InvestigateEntityHandler::new(pool.clone(), web),
        ));
    }
    if enabled.contains("fixture_boxscore") {
        let web = shared_web_workspace(&mut web_workspace)?;
        handlers.push(Arc::new(boxscore::FixtureBoxscoreHandler::new(
            pool.clone(),
            web,
        )));
    }

    // Voice registration order is the tested dependency order.
    for stage in VOICE_ORDER {
        if !enabled.contains(stage.as_str()) {
            continue;
        }
        handlers.push(match stage {
            crate::plugins::journalist::manifest::TASK => {
                Arc::new(journalist::NarrativesHandler::new(
                    pool.clone(),
                    models.capabilities(&crate::plugins::journalist::manifest::MANIFEST)?,
                )) as Arc<dyn StudioPlugin>
            }
            crate::plugins::influencer::manifest::TASK => Arc::new(influencer::VibeHandler::new(
                pool.clone(),
                models.capabilities(&crate::plugins::influencer::manifest::MANIFEST)?,
            )),
            // The rating stage feeds Momentum/Sigil but not the news rail, so it sits behind the
            // two news-product voices: a nightly stat backlog must not delay The Journalist.
            crate::plugins::scout::manifest::TASK => Arc::new(scout::RatingHandler::new(
                pool.clone(),
                models.capabilities(&crate::plugins::scout::manifest::MANIFEST)?,
            )),
            crate::plugins::insider::manifest::TASK => Arc::new(insider::TransferHandler::new(
                pool.clone(),
                models.capabilities(&crate::plugins::insider::manifest::MANIFEST)?,
            )),
            // Momentum consumes the rating card + vibe, so a vibe hand-off drains in the same
            // tick pass instead of waiting for the next NOTIFY/safety-net wake.
            crate::plugins::analyst::manifest::TASK => Arc::new(analyst::MomentumHandler::new(
                pool.clone(),
                models.capabilities(&crate::plugins::analyst::manifest::MANIFEST)?,
            )),
            // Sigil is terminal because it reads all five pillars.
            crate::plugins::oracle::manifest::TASK => Arc::new(oracle::SigilHandler::new(
                pool.clone(),
                models.capabilities(&crate::plugins::oracle::manifest::MANIFEST)?,
            )),
            other => unreachable!("{other} is not a voice; VOICE_ORDER holds the six voices"),
        });
    }

    Ok(handlers)
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

/// Allocate the process's shared workspace only when an enabled plugin needs it. This keeps an
/// intentionally idle or voice-only worker from initializing an unused provider capability.
fn shared_web_workspace(workspace: &mut Option<Arc<WebBroker>>) -> Result<Arc<WebBroker>> {
    if workspace.is_none() {
        *workspace = Some(Arc::new(WebBroker::new(0)?));
    }
    Ok(workspace
        .as_ref()
        .expect("workspace was initialized")
        .clone())
}

#[cfg(test)]
mod tests {
    use super::{
        build_reactions, enabled_from_config, known_stages, shared_web_workspace, VOICE_ORDER,
    };

    #[test]
    fn unset_configuration_keeps_harvester_opt_in() {
        let stages = enabled_from_config(None).unwrap();
        assert_eq!(stages.len() + 1, known_stages().len());
        for stage in known_stages() {
            assert_eq!(stages.contains(stage), stage != "harvester");
        }
        assert!(enabled_from_config(Some("harvester"))
            .unwrap()
            .contains("harvester"));
    }

    #[test]
    fn configured_stages_normalize_and_dedupe() {
        let stages = enabled_from_config(Some(
            " Graph, fixture_boxscore, rating, momentum, vibe, VIBE ,,sigil ",
        ))
        .unwrap();
        assert_eq!(stages.len(), 6);
        assert!(stages.contains("graph"));
        assert!(!stages.contains("editor"));
        assert!(stages.contains("fixture_boxscore"));
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
        assert!(err.contains("headlinez"));
        assert!(err.contains("editor"));
        assert!(err.contains("scrub"));
        assert!(err.contains("oracle"));
        assert!(err.contains("narratives"));
    }

    #[test]
    fn explicit_empty_configuration_is_an_idle_fleet() {
        assert!(enabled_from_config(Some("  ")).unwrap().is_empty());
    }

    #[test]
    fn acquisition_plugins_receive_one_shared_web_workspace() {
        let mut workspace = None;
        let investigator = shared_web_workspace(&mut workspace).unwrap();
        let boxscore = shared_web_workspace(&mut workspace).unwrap();
        assert!(std::sync::Arc::ptr_eq(&investigator, &boxscore));
    }

    #[test]
    fn composition_registers_consumers_after_their_producers() {
        let position = |stage| {
            VOICE_ORDER
                .iter()
                .position(|candidate| *candidate == stage)
                .unwrap()
        };
        assert!(
            position(crate::plugins::analyst::manifest::TASK)
                > position(crate::plugins::scout::manifest::TASK)
        );
        assert!(
            position(crate::plugins::analyst::manifest::TASK)
                > position(crate::plugins::influencer::manifest::TASK)
        );
        assert_eq!(
            position(crate::plugins::oracle::manifest::TASK),
            VOICE_ORDER.len() - 1
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
            crate::plugins::analyst::MOMENTUM_COMPLETED,
            crate::plugins::scout::RATING_DEBOUNCED,
            crate::plugins::journalist::NARRATIVES_COMPLETED,
            crate::plugins::insider::TRANSFER_PUBLISHED,
        ] {
            assert_eq!(complete.reaction_names(kind), oracle_only, "{kind}");
        }
        let momentum_then_oracle = ["analyst.enqueue-momentum", "oracle.completion-barrier"];
        for kind in [
            crate::plugins::influencer::VIBE_COMPLETED,
            crate::plugins::scout::RATING_COMPLETED,
        ] {
            assert_eq!(
                complete.reaction_names(kind),
                momentum_then_oracle,
                "{kind}"
            );
        }
        assert_eq!(
            complete.reaction_names(crate::plugins::scout::TRANSFER_IDENTITY_APPLIED),
            ["scout.rate-applied-identity"]
        );
    }
}

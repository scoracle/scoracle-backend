//! Concrete application assembly for the Studio plugin fleet.
//!
//! Studio owns the plugin contract; each plugin owns its manifest and adapters.
//! The application binds plugins to Postgres and model routing. Keeping this assembly
//! here leaves `main.rs` as process boot and makes the adapter boundary explicit.

use crate::application::models::Models;
use crate::application::queue::work;
use crate::application::tools::WebBroker;
use crate::plugins::analyst::adapter as analyst;
use crate::plugins::editor::adapter as editor;
use crate::plugins::fixture_boxscore::adapter as boxscore;
use crate::plugins::graph::adapter as graph;
use crate::plugins::influencer::adapter as influencer;
use crate::plugins::insider::adapter as insider;
use crate::plugins::journalist::adapter as journalist;
use crate::plugins::oracle::adapter as oracle;
use crate::plugins::scout::adapter as scout;
use crate::studio::plugin::StudioPlugin;
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;

/// First-party registration order. This is composition policy, not a durable
/// queue invariant; claim-time database gates remain authoritative across workers.
const VOICE_ORDER: [work::Stage; 6] = [
    work::Stage::Narratives,
    work::Stage::Vibe,
    work::Stage::Rating,
    work::Stage::Transfers,
    work::Stage::Momentum,
    work::Stage::Sigil,
];

/// Resolve `COGNITION_STAGES` against the registered Studio fleet.
///
/// An unset value means every task currently declared by a first-party manifest. An
/// explicitly empty value remains an idle fleet, which is useful for a process that
/// only runs its outbox or recovery loops.
pub fn enabled_from_config(raw: Option<&str>) -> Result<HashSet<String>> {
    let known = known_stages();
    let Some(raw) = raw else {
        return Ok(known.into_iter().map(str::to_owned).collect());
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
    let mut stages: Vec<&'static str> = crate::application::fleet::ALL
        .iter()
        .map(|manifest| manifest.task.as_str())
        .collect();
    stages.sort_unstable();
    stages.dedup();
    stages
}

/// Construct the enabled first-party fleet with application dependencies bound.
///
/// Registration order is deliberate: graph/editor share the acquisition rail, and the
/// voice stages follow their product dependency order. Scheduling caps and task ownership
/// still come from each plugin's Studio manifest once the fleet reaches the worker.
pub fn build(
    pool: PgPool,
    models: Arc<Models>,
    enabled: &HashSet<String>,
    packet_compile: bool,
) -> Result<Vec<Arc<dyn StudioPlugin>>> {
    let mut handlers: Vec<Arc<dyn StudioPlugin>> = Vec::new();
    let mut web_workspace: Option<Arc<WebBroker>> = None;

    // Graph is article-keyed and downstream of the Editor.
    if enabled.contains("graph") {
        handlers.push(Arc::new(graph::GraphHandler::new(
            pool.clone(),
            models.clone(),
        )));
    }
    // Graph registers first so it reclaims shared slots promptly.
    if enabled.contains("editor") {
        handlers.push(Arc::new(editor::EditorHandler::new(
            pool.clone(),
            models.clone(),
            packet_compile,
        )));
    }
    // Discovery uses the Editor's idle shared capacity.
    if enabled.contains("investigate_entity") {
        let web = shared_web_workspace(&mut web_workspace)?;
        handlers.push(Arc::new(
            crate::plugins::investigator::adapter::InvestigateEntityHandler::new(
                pool.clone(),
                models.clone(),
                web,
            ),
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
            work::Stage::Narratives => Arc::new(journalist::NarrativesHandler::new(
                pool.clone(),
                models.clone(),
            )) as Arc<dyn StudioPlugin>,
            work::Stage::Vibe => {
                Arc::new(influencer::VibeHandler::new(pool.clone(), models.clone()))
            }
            // The rating stage feeds Momentum/Sigil but not the news rail, so it sits behind the
            // two news-product voices: a nightly stat backlog must not delay The Journalist.
            work::Stage::Rating => {
                Arc::new(scout::RatingHandler::new(pool.clone(), models.clone()))
            }
            work::Stage::Transfers => {
                Arc::new(insider::TransferHandler::new(pool.clone(), models.clone()))
            }
            // Momentum consumes the rating card + vibe, so a vibe hand-off drains in the same
            // tick pass instead of waiting for the next NOTIFY/safety-net wake.
            work::Stage::Momentum => {
                Arc::new(analyst::MomentumHandler::new(pool.clone(), models.clone()))
            }
            // Sigil is terminal because it reads all five pillars.
            work::Stage::Sigil => Arc::new(oracle::SigilHandler::new(pool.clone(), models.clone())),
            other => unreachable!("{other} is not a voice; VOICE_ORDER holds the six voices"),
        });
    }

    Ok(handlers)
}

/// Assemble complete durable reaction chains from the statically linked fleet.
/// Reactions are independent of this process's enabled task handlers: any host may
/// durably enqueue work for a stage that another worker process executes.
pub fn build_reactions(
    pool: PgPool,
) -> Result<crate::application::queue::outbox::ReactionRegistry> {
    use crate::application::queue::outbox::EventReaction;

    let reactions: Vec<Arc<dyn EventReaction>> = vec![
        Arc::new(analyst::MomentumReaction::new(pool.clone())),
        Arc::new(oracle::OracleBarrierReaction::new(pool.clone())),
        Arc::new(scout::RatingIdentityReaction::new(pool)),
    ];
    crate::application::queue::outbox::ReactionRegistry::new(reactions)
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
    use crate::application::queue::work::Stage;

    #[test]
    fn unset_configuration_enables_every_manifest_task() {
        let stages = enabled_from_config(None).unwrap();
        assert_eq!(stages.len(), known_stages().len());
        for stage in known_stages() {
            assert!(stages.contains(stage));
        }
    }

    #[test]
    fn configured_stages_normalize_and_dedupe() {
        let stages = enabled_from_config(Some(
            " Graph, editor, fixture_boxscore, rating, momentum, vibe, VIBE ,,sigil ",
        ))
        .unwrap();
        assert_eq!(stages.len(), 7);
        assert!(stages.contains("graph"));
        assert!(stages.contains("editor"));
        assert!(stages.contains("fixture_boxscore"));
        assert!(stages.contains("rating"));
        assert!(stages.contains("momentum"));
        assert!(stages.contains("vibe"));
        assert!(stages.contains("sigil"));
    }

    #[test]
    fn configured_stages_reject_unknown_values() {
        let err = enabled_from_config(Some("graph,headlinez,scrub,oracle"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("headlinez"));
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
        assert!(position(Stage::Momentum) > position(Stage::Rating));
        assert!(position(Stage::Momentum) > position(Stage::Vibe));
        assert_eq!(position(Stage::Sigil), VOICE_ORDER.len() - 1);
    }

    #[test]
    fn composition_roster_contains_every_voice_once() {
        assert_eq!(
            VOICE_ORDER,
            [
                Stage::Narratives,
                Stage::Vibe,
                Stage::Rating,
                Stage::Transfers,
                Stage::Momentum,
                Stage::Sigil,
            ]
        );
    }

    #[tokio::test]
    async fn reactions_register_complete_chains_independently_of_task_handlers() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://localhost/unused")
            .unwrap();
        let complete = build_reactions(pool).unwrap();
        assert_eq!(
            complete.reaction_names("rating_completed"),
            ["analyst.enqueue-momentum", "oracle.completion-barrier"]
        );
        assert_eq!(
            complete.reaction_names("transfer_identity_applied"),
            ["scout.rate-applied-identity"]
        );
    }
}

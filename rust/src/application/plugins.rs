//! Concrete application assembly for the Studio plugin fleet.
//!
//! Studio owns the plugin contract and manifests; the application owns binding those
//! plugins to Postgres, model routing, and queue-facing adapters. Keeping this assembly
//! here leaves `main.rs` as process boot and makes the adapter boundary explicit.

use crate::application::editor;
use crate::application::graph;
use crate::application::insider;
use crate::application::investigator::boxscore;
use crate::application::models::Models;
use crate::application::queue::work;
use crate::application::{analyst, influencer, journalist, oracle, scout};
use crate::studio::plugin::StudioPlugin;
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;

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
    let mut stages: Vec<&'static str> = crate::studio::fleet::ALL
        .iter()
        .flat_map(|manifest| manifest.tasks.iter().map(|task| task.as_str()))
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
) -> Result<Vec<Arc<dyn StudioPlugin>>> {
    let mut handlers: Vec<Arc<dyn StudioPlugin>> = Vec::new();

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
        )));
    }
    // Discovery uses the Editor's idle shared capacity.
    if enabled.contains("investigate_entity") {
        handlers.push(Arc::new(
            crate::application::investigator::InvestigateEntityHandler::new(
                pool.clone(),
                models.clone(),
            )?,
        ));
    }
    if enabled.contains("fixture_boxscore") {
        handlers.push(Arc::new(boxscore::FixtureBoxscoreHandler::new(
            pool.clone(),
        )?));
    }

    // Voice registration order is the tested dependency order.
    for stage in work::VOICE_ORDER {
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

#[cfg(test)]
mod tests {
    use super::{enabled_from_config, known_stages};

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
}

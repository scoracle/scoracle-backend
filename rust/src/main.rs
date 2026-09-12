//! scoracle-cognition — the Rust Cognition Harness service binary.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop. On boot it connects, verifies Ollama, recovers stale
//! leases, and drains each REGISTERED stage to empty; with no handlers it idles.
//!
//! Handlers register from `COGNITION_STAGES` (default: every live stage).

use anyhow::{anyhow, Result};
use scoracle_cognition::buildinfo;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::junctions::investigator::boxscore;
use scoracle_cognition::junctions::{
    analyst, editor, graph, influencer, insider, journalist, oracle, scout,
};
use scoracle_cognition::route::Router;
use scoracle_cognition::{config, db, ollama, openai, stage, work, worker};
use std::collections::HashSet;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = config::Config::from_env()?;
    info!(
        model = %cfg.ollama_model,
        commit = buildinfo::COMMIT,
        built = buildinfo::BUILD_TIME,
        "scoracle-cognition starting",
    );

    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    info!("connected to postgres");

    // Boot reachability check against EVERY host the route table names, not just the default
    // base. Under the topology split a role can live on another machine, and the most likely
    // failure by far is that machine being asleep — which should be one obvious WARN at boot
    // rather than a slow trickle of failed items an hour later. Still non-fatal: a host that
    // comes back mid-run heals on the next claim.
    // Ping each host with ITS backend's protocol: an oMLX host 404s an ollama-style ping and
    // the boot line then cries wolf about a healthy server (observed at the D-T47 cutover).
    let mut hosts: Vec<(&str, config::Backend)> = cfg
        .route
        .roles
        .values()
        .map(|s| (s.base_url.as_str(), s.backend))
        .collect();
    hosts.sort_unstable();
    hosts.dedup();
    for (host, backend) in &hosts {
        // A host's concurrency budget is its own; log it beside the ping so the resolved
        // topology is legible from the boot lines alone.
        let permits = cfg
            .route
            .backend_concurrency
            .get(*host)
            .copied()
            .unwrap_or(cfg.ollama_max_concurrent);
        let (kind, pinged) = match backend {
            config::Backend::Ollama => (
                "ollama",
                ollama::OllamaClient::new(*host, &cfg.ollama_model, cfg.ollama_timeout)?
                    .ping()
                    .await,
            ),
            config::Backend::OpenAi => (
                "openai",
                openai::OpenAiClient::new(*host, &cfg.ollama_model, cfg.ollama_timeout)?
                    .ping()
                    .await,
            ),
        };
        match pinged {
            Ok(()) => {
                info!(base_url = %host, backend = kind, max_concurrent = permits, "model host reachable")
            }
            Err(e) => {
                warn!(error = %e, base_url = %host, backend = kind, max_concurrent = permits, "model host NOT reachable (continuing; roles on this host will fail until it is)")
            }
        }
    }

    // The resolved role → model@host table. With one host this is the familiar single-model
    // deploy; with two it is the only place the split is visible at a glance, so a misrouted
    // character is caught at boot instead of in a week-old sigil card.
    let mut routes: Vec<String> = cfg
        .route
        .roles
        .iter()
        .map(|(role, spec)| format!("{}={}@{}", role.as_str(), spec.model, spec.base_url))
        .collect();
    routes.sort();
    info!(hosts = hosts.len(), routes = %routes.join(" "), "resolved model topology");

    // Env-driven stage registration; the default owns every live cognition stage.
    let enabled = parse_enabled_stages(&std::env::var("COGNITION_STAGES").unwrap_or_else(|_| {
        "graph,editor,investigate_entity,fixture_boxscore,rating,momentum,transfers,narratives,vibe,sigil"
            .to_string()
    }))?;

    // Shared database, routing, budget, and context-window capabilities.
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?,
        // The same ceiling the worker enforces, handed to the handlers so a multi-call stage can
        // land inside it under its own power rather than being cancelled at it.
        handler_budget: cfg.handler_timeout,
        voice_num_ctx: cfg.voice_num_ctx,
    };

    // Each handler owns exactly one enabled queue stage.
    let mut handlers: Vec<Box<dyn stage::StageHandler>> = Vec::new();
    // Graph is article-keyed and downstream of the Editor.
    if enabled.contains("graph") {
        handlers.push(Box::new(graph::GraphHandler::new()));
    }
    // Graph registers first so it reclaims shared slots promptly.
    if enabled.contains("editor") {
        handlers.push(Box::new(editor::EditorHandler::new()));
    }
    // Discovery uses the Editor's idle shared capacity.
    if enabled.contains("investigate_entity") {
        handlers.push(Box::new(
            scoracle_cognition::junctions::investigator::entity::InvestigateEntityHandler::new()?,
        ));
    }
    if enabled.contains("fixture_boxscore") {
        handlers.push(Box::new(boxscore::FixtureBoxscoreHandler::new()?));
    }
    // Voice registration order is the tested dependency order.
    for stage in work::VOICE_ORDER {
        if !enabled.contains(stage.as_str()) {
            continue;
        }
        handlers.push(match stage {
            work::Stage::Narratives => {
                Box::new(journalist::NarrativesHandler::new()) as Box<dyn stage::StageHandler>
            }
            work::Stage::Vibe => Box::new(influencer::VibeHandler::new()),
            // The rating stage feeds Momentum/Sigil but not the news rail, so it sits behind the
            // two news-product voices: a nightly stat backlog must not delay The Journalist.
            work::Stage::Rating => Box::new(scout::RatingHandler::new()),
            work::Stage::Transfers => Box::new(insider::TransferHandler::new()),
            // momentum consumes the rating card + vibe, so a vibe hand-off
            // (enqueue_momentum_if_needed) drains in the same tick pass instead of waiting for
            // the next NOTIFY/safety-net wake.
            work::Stage::Momentum => Box::new(analyst::MomentumHandler::new()),
            // Sigil is terminal because it reads all five pillars.
            work::Stage::Sigil => Box::new(oracle::SigilHandler::new()),
            other => unreachable!("{other} is not a voice; VOICE_ORDER holds the six voices"),
        });
    }
    info!(stages = ?enabled, handlers = handlers.len(), "registered stage handlers");
    // Log switches that change what the deploy writes.
    info!(
        packet_compile = cfg.packet_compile,
        "desk: storyline assembly always on; packet compile gated by COGNITION_PACKET_COMPILE"
    );
    // Every voice derives its prompt budget from this shared window.
    info!(
        voice_num_ctx = cfg.voice_num_ctx,
        pinned = std::env::var("VOICE_NUM_CTX").is_ok(),
        envelope = if scoracle_cognition::route::small_voice_window(cfg.voice_num_ctx) {
            "small: reservations ≤700, crown cards capped, journalist corpus 8"
        } else {
            "wide: larger reservations, no card caps, journalist corpus 40"
        },
        "VOICE WINDOW: every voice on this host requests num_ctx {}",
        cfg.voice_num_ctx
    );

    let worker = worker::Worker::new(
        harness,
        handlers,
        cfg.safety_net,
        cfg.stale_lease,
        cfg.handler_timeout,
        cfg.watchdog,
        cfg.drain_concurrency,
        cfg.packet_compile,
    );
    worker.run().await
}

fn parse_enabled_stages(raw: &str) -> Result<HashSet<String>> {
    const KNOWN: &[&str] = &[
        "graph",
        "editor",
        "investigate_entity",
        "fixture_boxscore",
        "rating",
        "momentum",
        "transfers",
        "narratives",
        "vibe",
        "sigil",
    ];
    let mut stages = HashSet::new();
    let mut unknown = Vec::new();
    for stage in raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
    {
        if KNOWN.contains(&stage.as_str()) {
            stages.insert(stage);
        } else {
            unknown.push(stage);
        }
    }
    if !unknown.is_empty() {
        return Err(anyhow!(
            "unknown COGNITION_STAGES value(s): {}; allowed: {}",
            unknown.join(","),
            KNOWN.join(",")
        ));
    }
    Ok(stages)
}

#[cfg(test)]
mod tests {
    use super::parse_enabled_stages;

    #[test]
    fn parse_enabled_stages_normalizes_and_dedupes() {
        let stages = parse_enabled_stages(
            " Graph, editor, fixture_boxscore, rating, momentum, vibe, VIBE ,,sigil ",
        )
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
    fn parse_enabled_stages_rejects_unknown_values() {
        let err = parse_enabled_stages("graph,headlinez,scrub,oracle")
            .unwrap_err()
            .to_string();
        assert!(err.contains("headlinez"));
        assert!(err.contains("scrub"));
        assert!(err.contains("oracle"));
        assert!(err.contains("narratives"));
    }
}

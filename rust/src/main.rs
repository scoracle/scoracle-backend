//! scoracle-cognition — the Rust Cognition Harness service binary.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop. On boot it connects, verifies Ollama, recovers stale
//! leases, and drains each REGISTERED stage to empty; with no handlers it idles.
//!
//! Handlers register from `COGNITION_STAGES` (comma-separated; default `vibe,sigil`). IMPORTANT:
//! only run this binary live for stages the Go Drainer no longer owns, or both claim the same items
//! and burn the GPU twice. The L6 scrub cutover runs scrub-only (`COGNITION_STAGES=scrub`): the Go
//! Drainer has no scrub handler, so there is no collision, while vibe/sigil stay with Go until their
//! own cutover. The offline harnesses (`src/bin/*`) never claim the live queue.
//!
//! See `rust/README.md` and the canonical architecture doc
//! `scoracleWiki/wiki/Architecture/Rust Cognition Harness.md` (the older phased plan
//! `scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md` is superseded on sequencing).

use anyhow::Result;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::route::Router;
use scoracle_cognition::{config, db, embed, ollama, scrub, sigil, stage, vibe, worker};
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
    info!(model = %cfg.ollama_model, "scoracle-cognition starting");

    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    info!("connected to postgres");

    // Boot reachability check against the shared Ollama base (every role's backend today).
    // The router builds its own per-role clients from config below; this throwaway only pings.
    let ping_client =
        ollama::OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
    match ping_client.ping().await {
        Ok(()) => info!(base_url = %cfg.ollama_base_url, "ollama reachable"),
        Err(e) => {
            warn!(error = %e, base_url = %cfg.ollama_base_url, "ollama not reachable (continuing; claimed items will fail until it is)")
        }
    }

    // Env-driven stage registration (COGNITION_STAGES, comma-separated; default "vibe,sigil").
    // For the L6 scrub cutover run scrub-only (COGNITION_STAGES=scrub) so the service never
    // double-claims vibe/sigil — those stay with the Go Drainer until their own cutover.
    let enabled: HashSet<String> = std::env::var("COGNITION_STAGES")
        .unwrap_or_else(|_| "vibe,sigil".to_string())
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    // The scrub gate needs the CPU embedder (the asymmetric resolve_set pre-filter); the per-entity
    // stages don't, so it loads ONLY when scrub is enabled (a heavy resource — Plan §1.4).
    let embedder = if enabled.contains("scrub") {
        info!(model = %cfg.embed.model_repo, "loading embedder for the scrub gate (CPU)");
        Some(embed::Embedder::from_config(&cfg.embed)?)
    } else {
        None
    };

    // The capability context handed to every stage: the config-driven router (role → model from
    // COGNITION_ROUTE_*, all-Gemma by default), the embedder (Some only for the scrub path), the pool.
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout)?,
        embedder,
        resolve: cfg.resolve.clone(),
    };

    // Each handler owns exactly one queue stage. NB: registration ≠ cutover — only run live for
    // stages the Go Drainer no longer owns (it would otherwise double-claim). scrub is safe to run
    // live now (the Go Drainer has no scrub handler); vibe/sigil await their own cutover.
    let mut handlers: Vec<Box<dyn stage::StageHandler>> = Vec::new();
    if enabled.contains("scrub") {
        handlers.push(Box::new(scrub::ScrubHandler::new()));
    }
    if enabled.contains("vibe") {
        handlers.push(Box::new(vibe::VibeHandler::new()));
    }
    if enabled.contains("sigil") {
        handlers.push(Box::new(sigil::SigilHandler::new()));
    }
    info!(stages = ?enabled, handlers = handlers.len(), "registered stage handlers");

    let worker = worker::Worker::new(harness, handlers, cfg.safety_net, cfg.stale_lease);
    worker.run().await
}

//! scoracle-cognition — the Rust Cognition Harness service binary.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop. On boot it connects, verifies Ollama, recovers stale
//! leases, and drains each REGISTERED stage to empty; with no handlers it idles.
//!
//! Phase 1 registers the first handler — `vibe`. IMPORTANT: do NOT run this binary
//! against a database whose Go drainer still owns the vibe stage, or both will claim
//! real `vibe` items and burn the GPU twice (the per-stage cutover is Phase 2). The
//! offline parity proof runs through `src/bin/parity.rs`, which writes only the shadow
//! table and never claims the live queue.
//!
//! See `rust/README.md` and the canonical architecture doc
//! `scoracleWiki/wiki/Architecture/Rust Cognition Harness.md` (the older phased plan
//! `scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md` is superseded on sequencing).

use anyhow::Result;
use scoracle_cognition::{config, db, ollama, stage, vibe, worker};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cfg = config::Config::from_env()?;
    info!(model = %cfg.ollama_model, "scoracle-cognition starting");

    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    info!("connected to postgres");

    let ollama = ollama::OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
    match ollama.ping().await {
        Ok(()) => info!(base_url = %cfg.ollama_base_url, "ollama reachable"),
        Err(e) => warn!(error = %e, base_url = %cfg.ollama_base_url, "ollama not reachable (continuing; claimed items will fail until it is)"),
    }

    // Registered stages. Each handler owns exactly one queue stage; register in
    // dependency order (transfers → narratives → vibe → sigil) as stages are ported.
    let handlers: Vec<Box<dyn stage::StageHandler>> = vec![Box::new(vibe::VibeHandler::new())];

    let worker = worker::Worker::new(pool, ollama, handlers, cfg.safety_net, cfg.stale_lease);
    worker.run().await
}

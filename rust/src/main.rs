//! scoracle-scrubber — the Rust scrubbing layer (Phase 0 scaffold).
//!
//! What this is: a durable `pipeline_work` queue consumer plus an Ollama client,
//! wired to a LISTEN/NOTIFY drain loop. It is the foundation the per-stage
//! derivation handlers (vibe, narratives, transfers, sigil) plug into in later
//! phases. NO stage logic ships here — with zero handlers registered the worker
//! connects, verifies Ollama, listens, and idles without mutating the queue.
//!
//! See `rust/README.md` and the vault plan
//! `scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md`.

// Phase 0: the queue/Ollama client surface is intentionally complete but not
// all called yet — stage handlers (Phase 1+) exercise it. Remove once the first
// handler lands.
#![allow(dead_code)]

mod config;
mod db;
mod ollama;
mod stage;
mod util;
mod work;
mod worker;

use anyhow::Result;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cfg = config::Config::from_env()?;
    info!(model = %cfg.ollama_model, "scoracle-scrubber starting (Phase 0 scaffold)");

    let pool = db::build_pool(&cfg.database_url, cfg.db_max_conns).await?;
    info!("connected to postgres");

    let ollama = ollama::OllamaClient::new(&cfg.ollama_base_url, &cfg.ollama_model, cfg.ollama_timeout)?;
    match ollama.ping().await {
        Ok(()) => info!(base_url = %cfg.ollama_base_url, "ollama reachable"),
        Err(e) => warn!(error = %e, base_url = %cfg.ollama_base_url, "ollama not reachable (continuing; no stages registered)"),
    }

    // Phase 1 registers the first handler here, e.g.:
    //   handlers.push(Box::new(vibe::VibeHandler::new()));
    let handlers: Vec<Box<dyn stage::StageHandler>> = Vec::new();

    let worker = worker::Worker::new(pool, ollama, handlers, cfg.safety_net, cfg.stale_lease);
    worker.run().await
}

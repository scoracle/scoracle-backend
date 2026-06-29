//! scoracle-cognition — the Rust Cognition Harness service binary.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop. On boot it connects, verifies Ollama, recovers stale
//! leases, and drains each REGISTERED stage to empty; with no handlers it idles.
//!
//! Handlers register from `COGNITION_STAGES` (comma-separated; default = every stage).
//! Post Step-3 cutover (2026-06-28) the Rust daemon owns ALL five LLM queue stages —
//! scrub, transfers, narratives, vibe, sigil — and the Go API's derive worker is retired
//! (`DERIVE_WORKER_ENABLED=false` keeps it off). The committed systemd unit
//! (`scripts/systemd/scoracle-cognition.service`) hardcodes the production set, so this
//! default only fires when the unit isn't the one starting the process (a fresh-box boot
//! without systemd, etc.) — picking the full set means a misconfigure still runs cleanly.
//! The offline harnesses (`src/bin/*`) never claim the live queue.
//!
//! See `rust/README.md` and the canonical architecture doc
//! `scoracleWiki/wiki/Architecture/Rust Cognition Harness.md` (the older phased plan
//! `scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md` is superseded on sequencing).

use anyhow::Result;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::route::Router;
use scoracle_cognition::{
    config, db, embed, narratives, ollama, scrub, sigil, stage, transfer, vibe, worker,
};
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

    // Env-driven stage registration (COGNITION_STAGES, comma-separated; default = every stage).
    // Post Step-3 cutover the daemon owns all five stages; the Go derive worker is retired. To
    // revert Step 3 in an emergency, set DERIVE_WORKER_ENABLED=true (re-arm Go) and stop this
    // service — see RUNBOOK.md §3 "Step-3 cognition rollback".
    let enabled: HashSet<String> = std::env::var("COGNITION_STAGES")
        .unwrap_or_else(|_| "scrub,transfers,narratives,vibe,sigil".to_string())
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    // The CPU embedder (candle, Plan §1.4) powers the scrub gate's asymmetric resolve_set pre-filter
    // AND the narratives near-duplicate dedup. It is a heavy resource, so it loads ONLY when one of
    // those stages is enabled; the other per-entity stages never embed. With it None, narratives'
    // dedup is the identity (the byte-parity path) — which is exactly what the offline bins rely on.
    let embedder = if enabled.contains("scrub") || enabled.contains("narratives") {
        info!(model = %cfg.embed.model_repo, "loading embedder (CPU) for scrub/narratives");
        Some(embed::Embedder::from_config(&cfg.embed)?)
    } else {
        None
    };

    // The capability context handed to every stage: the config-driven router (role → model from
    // COGNITION_ROUTE_*, all-Gemma by default), the embedder (Some only for the scrub path), the pool.
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?,
        embedder,
        resolve: cfg.resolve.clone(),
    };

    // Each handler owns exactly one queue stage. Post Step-3 the daemon owns all five; the
    // Go derive path is off (DERIVE_WORKER_ENABLED=false). The COGNITION_STAGES env can
    // still be narrowed (e.g. a debug run that wants only `vibe`), but the systemd unit on
    // the prod box hardcodes the full set.
    let mut handlers: Vec<Box<dyn stage::StageHandler>> = Vec::new();
    if enabled.contains("scrub") {
        handlers.push(Box::new(scrub::ScrubHandler::new()));
    }
    if enabled.contains("transfers") {
        handlers.push(Box::new(transfer::TransferHandler::new()));
    }
    // narratives needs the CPU embedder loaded above for its near-duplicate dedup step.
    if enabled.contains("narratives") {
        handlers.push(Box::new(narratives::NarrativesHandler::new()));
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

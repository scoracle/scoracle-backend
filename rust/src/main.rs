//! scoracle-cognition — the Rust Cognition Harness service binary.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop. On boot it connects, verifies Ollama, recovers stale
//! leases, and drains each REGISTERED stage to empty; with no handlers it idles.
//!
//! Handlers register from `COGNITION_STAGES` (comma-separated; default = every stage).
//! Post Step-3 cutover (2026-06-28) the Rust daemon owns all LLM queue stages —
//! scrub, peak, momentum, transfers, narratives, vibe, sigil — and the Go API's derive worker is retired
//! (`DERIVE_WORKER_ENABLED=false` keeps it off). The committed systemd unit
//! (`scripts/systemd/scoracle-cognition.service`) hardcodes the production set, so this
//! default only fires when the unit isn't the one starting the process (a fresh-box boot
//! without systemd, etc.) — picking the full set means a misconfigure still runs cleanly.
//! The offline harnesses (`src/bin/*`) never claim the live queue.
//!
//! See `rust/README.md` and the canonical architecture doc
//! `scoracle-wiki/wiki/Architecture/Rust Cognition Harness.md` (the older phased plan
//! `scoracle-wiki/raw/scoracle-rust-scrubber-implementation-plan.md` is superseded on sequencing).

use anyhow::{anyhow, Result};
use scoracle_cognition::buildinfo;
use scoracle_cognition::harness::Harness;
use scoracle_cognition::route::Router;
use scoracle_cognition::{
    config, db, embed, graph, momentum, narratives, ollama, rating, scrub, sigil, stage, transfer,
    vibe, worker,
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
    info!(
        model = %cfg.ollama_model,
        commit = buildinfo::COMMIT,
        built = buildinfo::BUILD_TIME,
        "scoracle-cognition starting",
    );

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
    // Post Step-3 cutover the daemon owns the live cognition stages. Headlines has been folded into
    // narratives, so the news rail is scrub -> transfers -> narratives -> vibe -> momentum -> sigil. The Go derive worker is
    // retired. To revert Step 3 in an emergency, set
    // DERIVE_WORKER_ENABLED=true (re-arm Go) and stop this service — see RUNBOOK.md §3 rollback.
    let enabled =
        parse_enabled_stages(&std::env::var("COGNITION_STAGES").unwrap_or_else(|_| {
            "scrub,graph,peak,momentum,transfers,narratives,vibe,sigil".to_string()
        }))?;

    // The CPU embedder (candle, Plan §1.4) powers the scrub resolve pre-filter + bucket fallback,
    // narratives near-duplicate dedup, topic heat-rank, and vibe narrative relevance weighting. It
    // is a heavy resource, so it loads only when one of those stages is enabled.
    let embedder = if enabled.contains("scrub")
        || enabled.contains("narratives")
        || enabled.contains("vibe")
    {
        info!(model = %cfg.embed.model_repo, "loading embedder (CPU) for scrub/narratives/vibe");
        Some(embed::Embedder::from_config(&cfg.embed)?)
    } else {
        None
    };

    // The capability context handed to every stage: the config-driven router (role → local model
    // from COGNITION_ROUTE_*), the embedder (Some only for scrub/narratives), the pool.
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?,
        embedder,
        resolve: cfg.resolve.clone(),
        scrub: cfg.scrub.clone(),
    };

    // Each handler owns exactly one queue stage. Post Step-3 the daemon owns the live set; the
    // Go derive path is off (DERIVE_WORKER_ENABLED=false). The COGNITION_STAGES env can
    // still be narrowed (e.g. a debug run that wants only `vibe`), but the systemd unit on
    // the prod box hardcodes the full set.
    let mut handlers: Vec<Box<dyn stage::StageHandler>> = Vec::new();
    if enabled.contains("scrub") {
        handlers.push(Box::new(scrub::ScrubHandler::new()));
    }
    // graph is article-keyed directly downstream of scrub (the mig-165 vetted-trigger
    // enqueue): typed extraction into narrative_events + person-candidate evidence.
    // Wired 2026-07-19 after the fixture gate measured 12/12 at g2.
    if enabled.contains("graph") {
        handlers.push(Box::new(graph::GraphHandler::new()));
    }
    if enabled.contains("peak") {
        handlers.push(Box::new(rating::PeakHandler::new()));
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
    // momentum consumes PEAK + vibe, so it registers after vibe: a vibe hand-off
    // (enqueue_momentum_if_needed) drains in the same tick pass instead of waiting
    // for the next NOTIFY/safety-net wake.
    if enabled.contains("momentum") {
        handlers.push(Box::new(momentum::MomentumHandler::new()));
    }
    // sigil is the terminal stage: decide → voice as two internal steps of one work item
    // (the oracle stage folded in, Session B 2026-07-16).
    if enabled.contains("sigil") {
        handlers.push(Box::new(sigil::SigilHandler::new()));
    }
    info!(stages = ?enabled, handlers = handlers.len(), "registered stage handlers");

    let worker = worker::Worker::new(
        harness,
        handlers,
        cfg.safety_net,
        cfg.stale_lease,
        cfg.handler_timeout,
        cfg.watchdog,
    );
    worker.run().await
}

fn parse_enabled_stages(raw: &str) -> Result<HashSet<String>> {
    const KNOWN: &[&str] = &[
        "scrub",
        "graph",
        "peak",
        "momentum",
        "transfers",
        "narratives",
        "vibe",
        "sigil",
    ];
    // Retired stage names are tolerated with a warning (never a boot failure): a stale
    // COGNITION_STAGES in a unit override or .env must not take prod down at a cutover.
    // `oracle` folded into the sigil stage 2026-07-16 (Session B).
    const RETIRED: &[&str] = &["oracle"];

    let mut stages = HashSet::new();
    let mut unknown = Vec::new();
    for stage in raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
    {
        if KNOWN.contains(&stage.as_str()) {
            stages.insert(stage);
        } else if RETIRED.contains(&stage.as_str()) {
            warn!(stage = %stage, "COGNITION_STAGES names a retired stage; ignoring");
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
        let stages = parse_enabled_stages(" Scrub, peak, momentum, vibe, VIBE ,,sigil ").unwrap();
        assert_eq!(stages.len(), 5);
        assert!(stages.contains("scrub"));
        assert!(stages.contains("peak"));
        assert!(stages.contains("momentum"));
        assert!(stages.contains("vibe"));
        assert!(stages.contains("sigil"));
    }

    #[test]
    fn parse_enabled_stages_rejects_unknown_values() {
        let err = parse_enabled_stages("scrub,headlinez")
            .unwrap_err()
            .to_string();
        assert!(err.contains("headlinez"));
        assert!(err.contains("narratives"));
    }

    #[test]
    fn parse_enabled_stages_ignores_retired_oracle() {
        // A stale unit override or .env naming the folded-in stage must warn, not fail the
        // boot (Session B cutover safety).
        let stages = parse_enabled_stages("sigil,oracle").unwrap();
        assert_eq!(stages.len(), 1);
        assert!(stages.contains("sigil"));
        assert!(!stages.contains("oracle"));
    }
}

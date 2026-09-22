//! scoracle-cognition — the Rust Cognition Harness service binary.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop. On boot it connects, verifies Ollama, recovers stale
//! leases, and drains each REGISTERED stage to empty; with no handlers it idles.
//!
//! Plugins register from `COGNITION_STAGES` (default: every registered fleet task).

use anyhow::Result;
use scoracle_cognition::application::models::Models;
use scoracle_cognition::application::plugins;
use scoracle_cognition::application::queue::worker;
use scoracle_cognition::runtime::buildinfo;
use scoracle_cognition::runtime::config;
use scoracle_cognition::runtime::db;
use scoracle_cognition::runtime::providers::ollama;
use scoracle_cognition::runtime::providers::openai;
use scoracle_cognition::runtime::route::Router;
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

    // Env-driven task selection. The available/default values come from the registered
    // manifest fleet, so a task is not named separately in service configuration.
    let configured_stages = std::env::var("COGNITION_STAGES").ok();
    let enabled = plugins::enabled_from_config(configured_stages.as_deref())?;

    // Shared database, routing, budget, and context-window capabilities.
    let models = std::sync::Arc::new(Models {
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?,
        // The same ceiling the worker enforces, handed to the handlers so a multi-call stage can
        // land inside it under its own power rather than being cancelled at it.
        handler_budget: cfg.handler_timeout,
        voice_num_ctx: cfg.voice_num_ctx,
    });

    // Each plugin owns exactly one enabled queue stage via its manifest; scheduling caps
    // come from the same manifest. The worker validates the fleet (unique ids, unique
    // task ownership) at construction.
    let handlers = plugins::build(pool.clone(), models.clone(), &enabled)?;
    info!(stages = ?enabled, plugins = handlers.len(), "registered plugins");
    info!(
        plugins = scoracle_cognition::studio::fleet::ALL.len(),
        ids = scoracle_cognition::studio::fleet::ALL
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        "resolved plugin fleet"
    );
    // Log switches that change what the deploy writes.
    info!(
        packet_compile = cfg.packet_compile,
        "desk: storyline assembly always on; packet compile gated by COGNITION_PACKET_COMPILE"
    );
    // Every voice derives its prompt budget from this shared window.
    info!(
        voice_num_ctx = cfg.voice_num_ctx,
        pinned = std::env::var("VOICE_NUM_CTX").is_ok(),
        envelope = if scoracle_cognition::studio::model::small_voice_window(cfg.voice_num_ctx) {
            "small: reservations ≤700, crown cards capped, journalist corpus 8"
        } else {
            "wide: larger reservations, no card caps, journalist corpus 40"
        },
        "VOICE WINDOW: every voice on this host requests num_ctx {}",
        cfg.voice_num_ctx
    );

    let worker = worker::Worker::new(
        pool,
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

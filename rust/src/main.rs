//! scoracle-cognition — the Rust Cognition Harness service binary.
//!
//! A durable `pipeline_work` queue consumer plus an Ollama client, wired to a
//! LISTEN/NOTIFY drain loop. On boot it connects, verifies Ollama, recovers stale
//! leases, and drains each REGISTERED stage to empty; with no handlers it idles.
//!
//! Handlers register from `COGNITION_STAGES` (comma-separated; default = every stage).
//! Post Step-3 cutover (2026-06-28) the Rust daemon owns all LLM queue stages —
//! graph, editor, investigate_entity, peak, momentum, transfers, narratives, vibe, sigil — and the Go API's derive worker is retired
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
use scoracle_cognition::junctions::{
    analyst, editor, graph, influencer, insider, journalist, oracle, scout,
};
use scoracle_cognition::junctions::investigator::boxscore;
use scoracle_cognition::{config, db, embed, ollama, openai, stage, worker};
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
            Ok(()) => info!(base_url = %host, backend = kind, max_concurrent = permits, "model host reachable"),
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

    // Env-driven stage registration (COGNITION_STAGES, comma-separated; default = every stage).
    // Post Step-3 cutover the daemon owns the live cognition stages. Headlines has been folded into
    // narratives, and Phase 9 demolished the legacy rail's two stages, so the news rail is
    // editor -> graph -> transfers -> narratives -> vibe -> momentum -> sigil. The Go derive worker is
    // retired. To revert Step 3 in an emergency, set
    // DERIVE_WORKER_ENABLED=true (re-arm Go) and stop this service — see RUNBOOK.md §3 rollback.
    let enabled = parse_enabled_stages(&std::env::var("COGNITION_STAGES").unwrap_or_else(|_| {
        "graph,editor,investigate_entity,fixture_boxscore,peak,momentum,transfers,narratives,vibe,sigil"
            .to_string()
    }))?;

    // The CPU embedder (candle, Plan §1.4) now has exactly ONE consumer left: narratives, for its
    // pre-model corpus clustering and its thread-identity centroid cosine. Scrub's own relevance and
    // novelty gates were deleted in the teardown (§2.1/§2.2) and the stage itself in Phase 9.
    // Appendix A's remaining embedder item — retiring the narratives clustering — is NOT a deletion:
    // it changes what narratives reads and therefore its debounce hash, so it owes its own measured
    // session. Until then this block and `Harness.embedder` stay.
    let embedder = if enabled.contains("narratives") {
        info!(model = %cfg.embed.model_repo, "loading embedder (CPU) for narratives");
        Some(embed::Embedder::from_config(&cfg.embed)?)
    } else {
        None
    };

    // The capability context handed to every stage: the config-driven router (role → local model
    // from COGNITION_ROUTE_*), the embedder (Some only for narratives), the pool.
    let harness = Harness {
        pool,
        router: Router::from_config(&cfg.route, cfg.ollama_timeout, cfg.ollama_max_concurrent)?,
        embedder,
        // The same ceiling the worker enforces, handed to the handlers so a multi-call stage can
        // land inside it under its own power rather than being cancelled at it.
        handler_budget: cfg.handler_timeout,
        voice_num_ctx: cfg.voice_num_ctx,
    };

    // Each handler owns exactly one queue stage. Post Step-3 the daemon owns the live set; the
    // Go derive path is off (DERIVE_WORKER_ENABLED=false). The COGNITION_STAGES env can
    // still be narrowed (e.g. a debug run that wants only `vibe`), but the systemd unit on
    // the prod box hardcodes the full set.
    let mut handlers: Vec<Box<dyn stage::StageHandler>> = Vec::new();
    // graph is article-keyed, now downstream of the Editor's own enqueue (7.13) rather than the
    // retired mig-165 vetted trigger: typed extraction into narrative_events + person-candidate evidence.
    // Wired 2026-07-19 after the fixture gate measured 12/12 at g2.
    if enabled.contains("graph") {
        handlers.push(Box::new(graph::GraphHandler::new()));
    }
    // The Editor is the rail's sole reader since the flip; the legacy `article_read` it once
    // outranked in claim order was demolished in Phase 9. graph stays registered first — it
    // reclaims its slots on the next pass inside the shared archbox group (PLAN-one-rail 3.2/3.8).
    if enabled.contains("editor") {
        handlers.push(Box::new(editor::EditorHandler::new()));
    }
    // The Investigator (Phase 5): registered AFTER the Editor — the Editor outranks it on
    // the shared slot group (max_in_flight 1), so discovery only rides the card's idle time.
    if enabled.contains("investigate_entity") {
        handlers.push(Box::new(
            scoracle_cognition::junctions::investigator::entity::InvestigateEntityHandler::new()?,
        ));
    }
    if enabled.contains("fixture_boxscore") {
        handlers.push(Box::new(boxscore::FixtureBoxscoreHandler::new()));
    }
    if enabled.contains("transfers") {
        handlers.push(Box::new(insider::TransferHandler::new()));
    }
    // narratives needs the CPU embedder loaded above for its near-duplicate dedup step.
    if enabled.contains("narratives") {
        handlers.push(Box::new(journalist::NarrativesHandler::new()));
    }
    if enabled.contains("vibe") {
        handlers.push(Box::new(influencer::VibeHandler::new()));
    }
    // PEAK feeds Momentum/Sigil, but it does not feed the news rail. Keep it after
    // the news-product stages so a nightly stat backlog cannot delay The Journalist.
    if enabled.contains("peak") {
        handlers.push(Box::new(scout::PeakHandler::new()));
    }
    // momentum consumes PEAK + vibe, so it registers after both: a vibe hand-off
    // (enqueue_momentum_if_needed) drains in the same tick pass instead of waiting
    // for the next NOTIFY/safety-net wake.
    if enabled.contains("momentum") {
        handlers.push(Box::new(analyst::MomentumHandler::new()));
    }
    // sigil is the terminal stage: decide → voice as two internal steps of one work item
    // (the oracle stage folded in, Session B 2026-07-16).
    if enabled.contains("sigil") {
        handlers.push(Box::new(oracle::SigilHandler::new()));
    }
    info!(stages = ?enabled, handlers = handlers.len(), "registered stage handlers");
    // The Desk's switch is logged loudly, like every other thing that changes what a deploy
    // writes: storylines always assemble (greenfield tables only), packets compile only when
    // this says so (PLAN-one-rail 6.3 — mig 206's Journalist arm fans unconditionally).
    info!(
        packet_compile = cfg.packet_compile,
        "desk: storyline assembly always on; packet compile gated by COGNITION_PACKET_COMPILE"
    );
    // The rail the voices read (7.1). Louder than the Desk switch, because this one decides what
    // every voice's prompt is made of: under `legacy` the corpora and the prompt consts are
    // byte-identical to the pre-Phase-7 binary. Phase 8 flips it.
    // The voice window. The RAIL boot line went with the rail itself in the Phase 9 prune — there
    // is one corpus now, so announcing which one would be noise. This line stays: every
    // reservation and context cap in the six voices follows THIS number, so a boot that does not
    // state it leaves the budgets unexplainable from the journal.
    info!(
        voice_num_ctx = cfg.voice_num_ctx,
        pinned = std::env::var("VOICE_NUM_CTX").is_ok(),
        envelope = if scoracle_cognition::route::small_voice_window(cfg.voice_num_ctx) {
            "small: reservations ≤700, crown cards capped, journalist corpus 8"
        } else {
            "wide: legacy reservations, no card caps, journalist corpus 40"
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
    // `scrub` and `article_read` are the legacy rail's two stages, demolished in Phase 9 (9.1).
    // They land HERE rather than simply disappearing precisely because this list exists: an
    // archbox unit or a stale .env still naming them must warn and boot, not fail closed.
    const RETIRED: &[&str] = &["oracle", "scrub", "article_read"];

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
        let stages = parse_enabled_stages(
            " Graph, editor, fixture_boxscore, peak, momentum, vibe, VIBE ,,sigil ",
        )
        .unwrap();
        assert_eq!(stages.len(), 7);
        assert!(stages.contains("graph"));
        assert!(stages.contains("editor"));
        assert!(stages.contains("fixture_boxscore"));
        assert!(stages.contains("peak"));
        assert!(stages.contains("momentum"));
        assert!(stages.contains("vibe"));
        assert!(stages.contains("sigil"));
    }

    /// The legacy rail's stage names must WARN and drop, never fail a boot — an archbox unit or a
    /// stale .env still naming them is a config lag, not an outage. (Phase 9 demolition, 9.1.)
    #[test]
    fn parse_enabled_stages_tolerates_the_demolished_legacy_stages() {
        let stages = parse_enabled_stages("scrub,article_read,editor").unwrap();
        assert_eq!(stages.len(), 1);
        assert!(stages.contains("editor"));
        assert!(!stages.contains("scrub"));
        assert!(!stages.contains("article_read"));
    }

    #[test]
    fn parse_enabled_stages_rejects_unknown_values() {
        let err = parse_enabled_stages("graph,headlinez")
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

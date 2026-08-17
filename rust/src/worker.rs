//! The Cognition Harness worker — a durable `pipeline_work` consumer.
//!
//! Two cooperating tasks (split after the 2026-07-15 NOTIFY-queue incident, where one
//! hung await in the then-single worker loop froze the LISTEN socket and the drain
//! together, pinning the global NOTIFY queue until every `pipeline_work` statement
//! failed with SQLSTATE 54000):
//!
//! * The **drain** (the `run` future itself) executes recover-then-drain ticks. It is
//!   the only task that touches stage handlers, the embedder, or the GPU, so stage
//!   futures never cross a spawn boundary.
//! * The **supervisor** (spawned) owns everything that must stay responsive no matter
//!   what the drain is doing: the Postgres LISTEN socket (always read — a slow or
//!   wedged drain can no longer pin the NOTIFY queue), the safety-net timer,
//!   SIGINT/SIGTERM, and the no-progress watchdog.
//!
//! Tick requests flow supervisor → drain through a [`tokio::sync::Notify`] whose
//! single stored permit coalesces a burst of NOTIFYs arriving mid-drain into exactly
//! one follow-up tick. The drain reports progress through [`Pulse`] (heartbeat +
//! activity label); when a busy drain stops beating for `COGNITION_WATCHDOG_SECONDS`
//! the supervisor logs the wedged activity and exits the process — systemd
//! (`Restart=always`) boots it clean, the same remediation the incident reached by
//! hand after 34 hours. A per-item `COGNITION_HANDLER_TIMEOUT_SECONDS` bound converts
//! a hung await inside one handler into a normal failed-with-backoff item first, so
//! the watchdog is the backstop, not the path.
//!
//! Shutdown: either signal sets a flag the drain checks at every item boundary
//! (releasing unprocessed claims straight back to 'pending'), then a 75s in-process
//! grace aborts a stuck in-flight item — always inside systemd's 90s TimeoutStopSec,
//! so a stop/restart never escalates to SIGKILL.
//!
//! Phase 0 safety property: with NO handlers registered, `tick` short-circuits
//! before touching the queue — the scaffold only connects, pings Ollama, and
//! LISTENs. It performs zero writes, so it is safe to run against any DB while
//! you review the foundation.

use crate::harness::Harness;
use crate::junctions::editor;
use crate::stage::StageHandler;
use crate::work::{self, retry_backoff, Stage, MAX_ATTEMPTS};
use anyhow::{anyhow, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

/// Query how many storylines were updated in the last hour to determine velocity.
async fn storyline_velocity(pool: &PgPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) 
        FROM storylines 
        WHERE last_article_at > NOW() - INTERVAL '1 hour'
        "#,
    )
    .fetch_one(pool)
    .await
    .context("query storyline velocity")?;
    
    Ok(count)
}

/// Calculate the next desk interval based on storyline velocity.
/// 
/// High velocity (>10 storylines/hour) = 120 seconds (less frequent, save resources)
/// Medium velocity (3-10 storylines/hour) = 60 seconds (baseline)
/// Low velocity (<3 storylines/hour) = 30 seconds (more responsive)
fn velocity_adaptive_interval(velocity: i64) -> Duration {
    match velocity {
        v if v > 10 => Duration::from_secs(120),
        v if v < 3 => Duration::from_secs(30),
        _ => Duration::from_secs(60),
    }
}


const NOTIFY_CHANNEL: &str = "pipeline_work_ready";

/// Supervisor cadence for watchdog progress checks.
const WATCHDOG_POLL: Duration = Duration::from_secs(60);

/// How long a shutdown signal waits for the in-flight item before the drain is
/// dropped mid-await. Must stay under systemd's TimeoutStopSec (default 90s) so a
/// stop never escalates to SIGKILL: signal → flag (drain exits at the next item
/// boundary) → grace → abort → clean exit.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(75);

/// The live worker rotates stages one item at a time by default. LLM/model calls dominate
/// runtime, so minimizing product-stage latency is worth the extra cheap claims. A stage with no
/// model call overrides this via [`StageHandler::rotation_batch`] — see the note there.
const STAGE_ROTATION_BATCH: i64 = 1;

/// How often the Desk runs, on its OWN task, independent of the drain.
///
/// It used to ride `tick()` immediately after `drain_all` — which meant it only ran once EVERY
/// registered stage had drained to empty. Measured 2026-08-06 (the D-T14 (b) session): with 6,096
/// Editor items pending at ~3/min and 30,222 legacy `article_read` items behind them, "empty" is
/// a day and a half away, so the Desk was never called at all. `COGNITION_PACKET_COMPILE=true`
/// was set and provably inert — packets stayed at 0 because the compiler never ran, not because
/// it failed. A queue-length-dependent cadence is not a cadence.
///
/// Safe on its own task because the Desk is DB-only: dormancy is one UPDATE, compilation is
/// SELECTs plus an INSERT per storyline. No model call, no GPU, no embedder — none of the things
/// the 07-15 incident split kept off the supervisor's task. It deliberately does NOT beat the
/// drain's `Pulse`: the watchdog's question is whether the DRAIN is wedged, and a Desk heartbeat
/// answering it would mask exactly the stall the watchdog exists to catch.
const DESK_INTERVAL: Duration = Duration::from_secs(60);

/// Pulse is the drain's progress instrument, shared with the supervisor. The drain
/// beats it at every step boundary (claim, per-item handle, bookkeeping);
/// the supervisor reads it to tell a long-but-alive drain (beats keep advancing) from
/// a wedged one (busy with a stale beat). `activity` names the step the last beat
/// belongs to, so a watchdog fire points at the hung await instead of leaving a
/// silent journal (incident follow-up: the 07-13 wedge never named its hang site).
struct Pulse {
    busy: AtomicBool,
    at: StdMutex<Instant>,
    activity: StdMutex<String>,
}

impl Pulse {
    fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            at: StdMutex::new(Instant::now()),
            activity: StdMutex::new("idle".to_string()),
        }
    }

    fn begin(&self, activity: &str) {
        self.busy.store(true, Ordering::Release);
        self.beat(activity);
    }

    fn beat(&self, activity: &str) {
        *self.at.lock().unwrap() = Instant::now();
        *self.activity.lock().unwrap() = activity.to_string();
    }

    fn idle(&self) {
        self.beat("idle");
        self.busy.store(false, Ordering::Release);
    }

    /// snapshot returns (busy, age of last beat, last activity) for the watchdog.
    fn snapshot(&self) -> (bool, Duration, String) {
        (
            self.busy.load(Ordering::Acquire),
            self.at.lock().unwrap().elapsed(),
            self.activity.lock().unwrap().clone(),
        )
    }
}

/// One handler's claim ceiling: `(max_in_flight, slot_group)`.
type StageCap = (usize, Option<(&'static str, usize)>);
type StageCaps = Vec<StageCap>;

/// resolve_drain_concurrency picks the drain's global in-flight ceiling. An explicit
/// `COGNITION_DRAIN_CONCURRENCY` wins; otherwise it is derived from the stages' own caps, which
/// by construction can never bind.
///
/// Getting this wrong starves a machine rather than merely slowing it. Sizing the ceiling to the
/// topology's total GPU permits (4 local + 1 Mac = 5) looks right and is not: the drain claims in
/// DAG order, so scrub and the two gemma3 stages fill all 5 and the six Mac stages get nothing at
/// all until the local backlog — 2,580 items at the time of writing — is empty. The Mac would idle
/// for hours while Archbox worked, which is the exact inversion of the goal.
///
/// Grouped stages contribute their GROUP's budget once, not each stage's ceiling. The Editor and
/// graph may each claim up to 4, but only 4 between them, so summing both would inflate the global
/// budget by slots that can never be used at once — and a global budget that cannot bind is a
/// global budget that stops protecting the stages behind it.
fn resolve_drain_concurrency(configured: Option<usize>, caps: &[StageCap]) -> usize {
    configured
        .unwrap_or_else(|| {
            let mut total = 0usize;
            let mut counted: Vec<&'static str> = Vec::new();
            for (cap, group) in caps {
                match group {
                    Some((name, budget)) => {
                        if !counted.contains(name) {
                            counted.push(name);
                            total += (*budget).max(1);
                        }
                    }
                    None => total += (*cap).max(1),
                }
            }
            total
        })
        .max(1)
}

/// stage_room is how many items one stage may claim right now: its own remaining cap, further
/// limited by what is left of the global budget. Zero means skip it this pass.
fn stage_room(cap: usize, running: usize, budget_left: usize) -> usize {
    cap.max(1).saturating_sub(running).min(budget_left)
}

/// stalled is the watchdog predicate: a drain that claims to be busy but whose last
/// beat is older than `threshold` is wedged — a healthy drain beats at every item and
/// step boundary. Zero disables. Pure for tests.
fn stalled(busy: bool, beat_age: Duration, threshold: Duration) -> bool {
    !threshold.is_zero() && busy && beat_age >= threshold
}

/// Supervision is the Send-only slice of worker state the spawned supervisor needs:
/// pool for the LISTEN socket, policy durations, and the shared drain plumbing.
struct Supervision {
    pool: PgPool,
    safety_net: Duration,
    watchdog: Duration,
    pulse: Arc<Pulse>,
    tick: Arc<Notify>,
    cause: Arc<StdMutex<&'static str>>,
    shutdown: Arc<AtomicBool>,
}

impl Supervision {
    /// request asks the drain for a tick. `notify_one` stores at most one permit, so
    /// any burst arriving while a tick runs coalesces into exactly one follow-up tick
    /// (incident follow-up: the backlog chew showed ~5 full sweeps per 6ms). Coalesced
    /// ticks attribute to the most recent cause — good enough for logs.
    fn request(&self, cause: &'static str) {
        *self.cause.lock().unwrap() = cause;
        self.tick.notify_one();
    }
}

/// connect_listener retries until the LISTEN subscription holds. While Postgres is
/// away the drain's own claims fail visibly too; the safety-net tick resumes work
/// the moment it is back, and a fresh `LISTEN` covers notifications from then on.
async fn connect_listener(pool: &PgPool) -> PgListener {
    loop {
        match PgListener::connect_with(pool).await {
            Ok(mut listener) => match listener.listen(NOTIFY_CHANNEL).await {
                Ok(()) => {
                    info!(channel = NOTIFY_CHANNEL, "listening for work notifications");
                    return listener;
                }
                Err(e) => error!(error = %format!("{e:#}"), "LISTEN failed; retrying in 5s"),
            },
            Err(e) => {
                error!(error = %format!("{e:#}"), "listener connect failed; retrying in 5s")
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// supervise loops on the responsive half of the worker until a shutdown signal
/// arrives, then hands the drain its grace period and returns. The watchdog path
/// never returns — it exits the process for systemd to restart clean.
async fn supervise(sv: Supervision) {
    // Persistent streams, registered once: a signal arriving while the drain holds
    // the runtime's attention is latched and observed at the next poll. (The old
    // single-loop worker recreated `ctrl_c()` per select iteration, so a signal
    // landing mid-tick was swallowed — the reproduced stop → 90s → SIGKILL.)
    let mut sigint = signal(SignalKind::interrupt()).expect("register SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("register SIGTERM handler");
    let mut watchdog_poll = tokio::time::interval(WATCHDOG_POLL);
    watchdog_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut listener = connect_listener(&sv.pool).await;

    let signal_name;
    loop {
        tokio::select! {
            _ = sigint.recv() => { signal_name = "SIGINT"; break; }
            _ = sigterm.recv() => { signal_name = "SIGTERM"; break; }
            _ = tokio::time::sleep(sv.safety_net) => sv.request("safety-net"),
            recv = listener.recv() => match recv {
                Ok(_note) => sv.request("notify"),
                Err(e) => {
                    error!(error = %format!("{e:#}"), "listener error; reconnecting in 1s");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    listener = connect_listener(&sv.pool).await;
                    // Anything NOTIFYed during the gap is gone — tick once to cover it.
                    sv.request("listener-reconnect");
                }
            },
            _ = watchdog_poll.tick() => {
                let (busy, beat_age, activity) = sv.pulse.snapshot();
                if stalled(busy, beat_age, sv.watchdog) {
                    error!(
                        activity = %activity,
                        stalled_secs = beat_age.as_secs(),
                        "watchdog: drain made no progress past threshold — exiting for a clean systemd restart"
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    info!(
        signal = signal_name,
        grace_secs = SHUTDOWN_GRACE.as_secs(),
        "shutdown signal received; stopping drain at the next item boundary"
    );
    sv.shutdown.store(true, Ordering::Release);
    sv.tick.notify_one(); // wake an idle drain so it observes the flag immediately
    tokio::time::sleep(SHUTDOWN_GRACE).await;
}

/// The Desk's own task: everything the newsroom does between reads, on a cadence that does not
/// depend on the queue being empty. Spawned only on the machine that seats the Editor (the Mac's
/// voices have no Desk) — see [`DESK_INTERVAL`] for why it stopped riding the drain.
struct Desk {
    pool: PgPool,
    /// Whether the Desk compiles packets (`COGNITION_PACKET_COMPILE`, default off — see
    /// `config::Config::packet_compile`). Storyline assembly and dormancy are unconditional;
    /// only compilation is gated.
    packet_compile: bool,
    /// Unix seconds of the last dormancy sweep, 0 until the first runs. Hourly: dormancy is a
    /// 14-day fact, so a per-sweep pass would be a full-table UPDATE scan that finds nothing.
    last_desk_sweep: Arc<AtomicI64>,
    /// Set by the supervisor on SIGINT/SIGTERM; the Desk stops at the next boundary.
    shutdown: Arc<AtomicBool>,
}

impl Desk {
    /// The Desk's periodic work (PLAN-one-rail 6.3/6.4) — deterministic code, zero model calls.
    ///
    /// Two cadences in one pass:
    ///   * **hourly** — the storyline lifecycle sweep: an open storyline nobody has added to for
    ///     14 days goes dormant, which is what keeps the attachment rule's candidate set honest.
    ///   * **every sweep** — packet compilation, which carries its own 15-minute quiet debounce
    ///     in SQL (`packet::compile_dirty`), so a story arriving as a burst compiles once. OFF
    ///     unless `COGNITION_PACKET_COMPILE` says otherwise: `INSERT ON packets` fires mig 206's
    ///     voice fan-out, whose `narratives` arm was unconditional until mig 212 gated it on a
    ///     subscription — before that gate, and while a legacy rail still existed, compiling
    ///     would have fought the `article_read` enqueue over the same `pipeline_work` row (the
    ///     mig-197 churn loop). Both are history: that seat was demolished in Phase 9.1 and there
    ///     is one rail. With the subscription table empty the trigger is inert and this is a
    ///     shadow compile.
    ///
    /// Failure is logged and swallowed, like the dedup sweep: the Desk is downstream of reads
    /// that are already persisted, and it must never take the process down.
    async fn sweep(&self, cause: &str) {
        const DORMANCY_SWEEP_INTERVAL_SECS: i64 = 3_600;
        /// Storylines compiled per sweep. A ceiling, not a target: the backfill's first quiet
        /// window has thousands of dirty storylines, and one sweep is not the place to compile
        /// all of them at once.
        const COMPILE_BATCH: i64 = 200;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let last = self.last_desk_sweep.load(Ordering::Acquire);
        if last == 0 || now.saturating_sub(last) >= DORMANCY_SWEEP_INTERVAL_SECS {
            self.last_desk_sweep.store(now, Ordering::Release);
            match editor::storyline::mark_dormant(&self.pool).await {
                Ok(n) if n > 0 => info!(dormant = n, cause, "storylines went dormant"),
                Ok(_) => debug!(cause, "dormancy sweep: nothing quiet enough"),
                Err(e) => error!(error = %format!("{e:#}"), cause, "dormancy sweep failed"),
            }
        }

        if !self.packet_compile {
            return;
        }
        match editor::packet::compile_dirty(&self.pool, COMPILE_BATCH).await {
            Ok(n) if n > 0 => info!(packets = n, cause, "packets compiled"),
            Ok(_) => debug!(cause, "packet compile: nothing dirty and quiet"),
            Err(e) => error!(error = %format!("{e:#}"), cause, "packet compile failed"),
        }
    }
}

/// desk_loop sweeps with velocity-aware scheduling until shutdown. The first sweep runs
/// immediately, so a restart compiles what settled while the process was down instead of waiting
/// a full interval. The cadence adapts based on storyline velocity: high activity (>10/hour) uses
/// 120s intervals to conserve resources, low activity (<3/hour) uses 30s intervals for faster
/// responsiveness, and baseline activity uses 60s.
async fn desk_loop(desk: Desk) {
    let mut cause = "startup";
    loop {
        if desk.shutdown.load(Ordering::Acquire) {
            debug!("desk loop stopped cleanly");
            return;
        }
        desk.sweep(cause).await;
        cause = "interval";
        
        // Query velocity and adapt the cadence
        let velocity = match storyline_velocity(&desk.pool).await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %format!("{e:#}"), "failed to query storyline velocity, using baseline interval");
                5 // default to baseline
            }
        };
        let interval = velocity_adaptive_interval(velocity);
        debug!(velocity, interval_secs = interval.as_secs(), "desk loop cadence");
        
        tokio::time::sleep(interval).await;
    }
}

pub struct Worker {
    // The queue host owns the pool directly for its claim/complete/fail/LISTEN mechanics
    // (platform plumbing, not cognition), and owns the `Harness` it hands to each stage
    // (the capability context — same Arc-backed pool inside, plus the model router). The
    // proven drain loop is unchanged; only the per-item `handle` call passes the harness.
    pool: PgPool,
    harness: Harness,
    handlers: Vec<Box<dyn StageHandler>>,
    safety_net: Duration,
    stale_lease: Duration,
    /// Per-item ceiling on one stage handler run (`COGNITION_HANDLER_TIMEOUT_SECONDS`;
    /// zero disables). A hung await inside a handler fails the item after this long
    /// instead of stalling the drain forever.
    handler_timeout: Duration,
    /// The supervisor's no-progress threshold (`COGNITION_WATCHDOG_SECONDS`; zero
    /// disables): a busy drain whose heartbeat is older than this exits the process.
    watchdog: Duration,
    /// Resolved ceiling on claimed items in flight across all stages: either
    /// `COGNITION_DRAIN_CONCURRENCY` or, unset, the sum of the registered stages'
    /// `max_in_flight`. A claim bound, not a GPU bound — see `drain_all`.
    drain_concurrency: usize,
    /// Set by the supervisor on SIGINT/SIGTERM; the drain exits at the next boundary.
    shutdown: Arc<AtomicBool>,
    /// Latest tick cause, written by the supervisor with each request.
    cause: Arc<StdMutex<&'static str>>,
    /// Unix seconds of the last exact-title dedup sweep, 0 until the first runs. The sweep is
    /// hourly, not per-tick: it closes a RACE (two copies scrubbed before either has vetted
    /// membership are invisible to each other in `novelty::gate`), so it only has work to do
    /// after a scrub batch has settled. Running it every tick would be pure write amplification.
    last_dedup_sweep: Arc<AtomicI64>,
    /// Unix seconds of the last Desk sweep (PLAN-one-rail 6.4), 0 until the first runs. Hourly
    /// for the same reason as the dedup sweep: dormancy is a 14-day fact, so a per-tick pass
    /// would be a full-table UPDATE scan that finds nothing 3,600 times an hour.
    last_desk_sweep: Arc<AtomicI64>,
    /// Whether the Desk compiles packets (`COGNITION_PACKET_COMPILE`, default off — see
    /// `config::Config::packet_compile` and `editor::packet`'s fan-out note). Storyline
    /// assembly runs regardless; it happens inside the Editor's handle, not here.
    packet_compile: bool,
}

impl Worker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        harness: Harness,
        handlers: Vec<Box<dyn StageHandler>>,
        safety_net: Duration,
        stale_lease: Duration,
        handler_timeout: Duration,
        watchdog: Duration,
        drain_concurrency: Option<usize>,
        packet_compile: bool,
    ) -> Self {
        let pool = harness.pool.clone();
        // Unset means "let the per-stage caps govern": the sum of every stage's `max_in_flight`
        // is by construction the point past which the ceiling can never bind, so no stage is
        // ever starved by a global number nobody tuned. An explicit value is a throttle.
        let caps: StageCaps = handlers
            .iter()
            .map(|h| (h.max_in_flight(), h.slot_group()))
            .collect();
        let drain_concurrency = resolve_drain_concurrency(drain_concurrency, &caps);
        Self {
            pool,
            harness,
            handlers,
            safety_net,
            stale_lease,
            handler_timeout,
            watchdog,
            drain_concurrency,
            shutdown: Arc::new(AtomicBool::new(false)),
            cause: Arc::new(StdMutex::new("startup")),
            last_dedup_sweep: Arc::new(AtomicI64::new(0)),
            last_desk_sweep: Arc::new(AtomicI64::new(0)),
            packet_compile,
        }
    }

    /// Run until SIGINT/SIGTERM. Drains on start, on NOTIFY, and on the safety-net
    /// tick — all delivered as coalesced tick requests from the supervisor task.
    pub async fn run(&self) -> Result<()> {
        let stages: Vec<&'static str> = self.handlers.iter().map(|h| h.stage().as_str()).collect();
        info!(?stages, "cognition harness worker starting");
        if stages.is_empty() {
            warn!("no stage handlers registered — worker idles (Phase 0 scaffold)");
        }

        let pulse = Arc::new(Pulse::new());
        let tick = Arc::new(Notify::new());
        let mut supervisor = tokio::spawn(supervise(Supervision {
            pool: self.pool.clone(),
            safety_net: self.safety_net,
            watchdog: self.watchdog,
            pulse: pulse.clone(),
            tick: tick.clone(),
            cause: self.cause.clone(),
            shutdown: self.shutdown.clone(),
        }));

        // The Desk sweeps on its own task, not behind the drain — but only where the Editor is
        // seated. The Mac runs the voices and has no Desk, and this is the check that says so.
        if self.handlers.iter().any(|h| h.stage() == Stage::Editor) {
            info!(
                interval_secs = DESK_INTERVAL.as_secs(),
                packet_compile = self.packet_compile,
                "desk loop starting (own task, independent of the drain)"
            );
            tokio::spawn(desk_loop(Desk {
                pool: self.pool.clone(),
                packet_compile: self.packet_compile,
                last_desk_sweep: self.last_desk_sweep.clone(),
                shutdown: self.shutdown.clone(),
            }));
        }

        // The boot recover-and-drain rides the normal request path, so even startup
        // recovery runs under full signal + watchdog coverage.
        *self.cause.lock().unwrap() = "startup";
        tick.notify_one();

        loop {
            if self.shutting_down() {
                break;
            }
            tokio::select! {
                _ = tick.notified() => {}
                exit = &mut supervisor => {
                    note_supervisor_exit(exit);
                    return Ok(());
                }
            }
            if self.shutting_down() {
                break;
            }
            let cause = *self.cause.lock().unwrap();
            tokio::select! {
                _ = self.tick(cause, &pulse) => {}
                exit = &mut supervisor => {
                    // The shutdown grace expired (or the supervisor died) with a tick
                    // still in flight: dropping the tick future aborts the current item
                    // mid-await — nothing was persisted for it (fail-closed stages), and
                    // its lease recovers via requeue_stale.
                    note_supervisor_exit(exit);
                    warn!("shutdown grace expired mid-tick; in-flight item left to stale-lease recovery");
                    return Ok(());
                }
            }
        }
        info!("drain loop stopped cleanly");
        Ok(())
    }

    fn shutting_down(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Collapse byte-identical CROSS-SOURCE articles onto one canonical copy, at most hourly.
    ///
    /// This closes a race `novelty::gate` structurally cannot: that gate compares a freshly
    /// scrubbed article against recent canonical coverage of its OWN VETTED ENTITIES, so two
    /// copies scrubbed in the same pass — before either has vetted membership — are invisible to
    /// each other. Measured 2026-07-28: 2,057 of 32,016 corpus-visible articles in a 14-day window
    /// were exact-title duplicates of another corpus-visible article, which is 6.4% of every
    /// busyness verdict counting one story twice.
    ///
    /// A sweep is the only thing that can fix a race, and hourly is the right cadence: it has work
    /// only after a scrub batch has settled, so per-tick would be pure write amplification.
    ///
    /// The guards live in `collapse_exact_title_duplicates` (mig 196) — cross-source only, a
    /// minimum title length, and a canonical that prefers the corpus-visible copy. Failure is
    /// logged and swallowed: this is hygiene, and it must never block a drain.
    async fn sweep_exact_title_duplicates(&self, cause: &str, pulse: &Pulse) {
        const SWEEP_INTERVAL_SECS: i64 = 3_600;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let last = self.last_dedup_sweep.load(Ordering::Acquire);
        if last != 0 && now.saturating_sub(last) < SWEEP_INTERVAL_SECS {
            return;
        }
        self.last_dedup_sweep.store(now, Ordering::Release);

        pulse.begin("dedup-sweep");
        // 72h matches the novelty gate's own `novelty_lookback`: anything older has already been
        // swept, and re-scanning the full narratives window every hour buys nothing.
        let res: Result<i32, sqlx::Error> = sqlx::query_scalar(
            "SELECT public.collapse_exact_title_duplicates(interval '72 hours', 30)",
        )
        .fetch_one(&self.pool)
        .await;
        match res {
            Ok(n) if n > 0 => info!(collapsed = n, cause, "exact-title duplicates collapsed"),
            Ok(_) => debug!(cause, "dedup sweep: nothing to collapse"),
            Err(e) => error!(error = %format!("{e:#}"), cause, "dedup sweep failed"),
        }
    }

    /// One recover-then-drain cycle. No-op when no handlers are registered, so
    /// the scaffold never mutates the queue.
    async fn tick(&self, cause: &str, pulse: &Pulse) {
        if self.handlers.is_empty() {
            debug!(cause, "tick: no handlers registered; nothing to do");
            return;
        }
        pulse.begin("requeue-stale");
        match work::requeue_stale(&self.pool, self.stale_lease).await {
            Ok(n) if n > 0 => info!(recovered = n, cause, "requeued stale work"),
            Ok(_) => {}
            Err(e) => error!(error = %format!("{e:#}"), cause, "requeue stale failed"),
        }
        self.sweep_exact_title_duplicates(cause, pulse).await;
        self.drain_all(cause, pulse).await;
        // The Desk used to run here, after the drain. It now has its own task (`desk_loop`),
        // because "after the drain" means "after every stage is empty" — which, with a deep
        // legacy queue, never happens. See DESK_INTERVAL.
        pulse.idle();
    }

    /// Drain every registered stage to empty, keeping up to `drain_concurrency` claimed items
    /// in flight at once.
    ///
    /// This was a strictly sequential `for handler { for item { handle().await } }` until the
    /// topology split made it the bottleneck: with The Editor and graph local on gemma3:4b and
    /// the six character voices on the Mac, a sequential drain meant Archbox idled through every
    /// Mac generation and the Mac idled through every local one. Measured 255 calls/hour against
    /// a 310 baseline (-18%) with the Mac at ~89% utilisation — nothing broken, just half-used.
    ///
    /// **The governor is now the scheduler.** The drain's job is only to keep work OFFERED; the
    /// per-host semaphores from the topology split (`route.rs::governor_for`) decide what
    /// actually runs on each machine. Archbox pulls continuously up to its slot count, while the
    /// Mac's single permit keeps its six voices sequenced — which is the intended shape, not a
    /// limitation: 16 GB will not hold two KV allocations for a 14B.
    ///
    /// Registration order still encodes the DAG, and is still the claim order, so a hand-off
    /// enqueued by an item that just finished is picked up on the very next top-up pass.
    ///
    /// Concurrency is *intra-task*: the futures live in a `FuturesUnordered` polled by this one
    /// drain task and are never spawned. That preserves the property the 07-15 incident split
    /// was built on — stage futures, the embedder and the GPU stay off the supervisor's task, so
    /// nothing a handler does can pin the LISTEN socket.
    async fn drain_all(&self, cause: &str, pulse: &Pulse) {
        let budget = self.drain_concurrency.max(1);
        let mut inflight = FuturesUnordered::new();
        // In-flight items per stage, so no one stage can own the whole budget. Keyed by the
        // stage's static name because `Stage` is not `Hash`.
        let mut per_stage: HashMap<&'static str, usize> = HashMap::new();
        // In-flight per slot group, so stages sharing one backend's parallel slots divide them on
        // demand instead of by a fixed split. Keyed by group name; ungrouped stages never appear.
        let mut per_group: HashMap<&'static str, usize> = HashMap::new();
        // stage name -> its group, so a finishing item can decrement the right counter without
        // asking the handler again.
        let group_of: HashMap<&'static str, (&'static str, usize)> = self
            .handlers
            .iter()
            .filter_map(|h| h.slot_group().map(|g| (h.stage().as_str(), g)))
            .collect();

        loop {
            // Top up in registration (DAG) order until the budget is full or nothing is
            // claimable. A stage with nothing pending is skipped, not waited on.
            let mut claimed_any = false;
            for handler in &self.handlers {
                if self.shutting_down() || inflight.len() >= budget {
                    break;
                }
                let stage = handler.stage();
                // The per-stage cap is what keeps the DAG order from becoming a priority
                // order. Without it the first stage with a deep queue and a big
                // `rotation_batch` takes every slot: scrub claims 256, graph and the Editor
                // claim 8, and with a budget of 5 whichever registers first starves the rest —
                // including starving the GPU entirely behind model-free scrub work.
                let running = *per_stage.get(stage.as_str()).unwrap_or(&0);
                let mut room = stage_room(handler.max_in_flight(), running, budget - inflight.len());
                // A grouped stage is additionally bounded by what its co-tenants have left. This
                // is what lets The Editor spread into graph's idle slots without being able to
                // oversubscribe the card when graph is working.
                if let Some((name, group_budget)) = handler.slot_group() {
                    let group_running = *per_group.get(name).unwrap_or(&0);
                    room = room.min(group_budget.saturating_sub(group_running));
                }
                if room == 0 {
                    continue;
                }
                pulse.beat(&format!("claim {stage}"));
                let batch = handler
                    .rotation_batch()
                    .max(STAGE_ROTATION_BATCH)
                    .min(room as i64);
                let items = match work::claim(&self.pool, stage, batch).await {
                    Ok(items) => items,
                    Err(e) => {
                        error!(error = %format!("{e:#}"), %stage, cause, "claim failed");
                        break;
                    }
                };
                if items.is_empty() {
                    continue;
                }
                // The shutdown flag can flip while a claim is in flight. Those rows are ours
                // and nothing has touched them yet, so hand them straight back rather than
                // leaving them to the 30-min stale-lease sweep.
                if self.shutting_down() {
                    self.release_rest(&items).await;
                    break;
                }
                claimed_any = true;
                debug!(%stage, n = items.len(), cause, "draining batch");
                *per_stage.entry(stage.as_str()).or_insert(0) += items.len();
                if let Some((name, _)) = handler.slot_group() {
                    *per_group.entry(name).or_insert(0) += items.len();
                }
                for item in items {
                    inflight.push(self.run_one(handler.as_ref(), item, pulse));
                }
            }

            if inflight.is_empty() {
                // Nothing running and nothing claimable: the queue is drained.
                if !claimed_any {
                    break;
                }
                continue;
            }

            // Wait for exactly one item to finish, then loop to top up. Finishing one item
            // frees one host slot AND may have enqueued downstream work, so re-claiming here
            // is what keeps every machine's slots offered work continuously.
            if let Some(done) = inflight.next().await {
                if let Some(n) = per_stage.get_mut(done) {
                    *n = n.saturating_sub(1);
                }
                if let Some((name, _)) = group_of.get(done) {
                    if let Some(n) = per_group.get_mut(name) {
                        *n = n.saturating_sub(1);
                    }
                }
            }

            if self.shutting_down() {
                break;
            }
        }

        // Let whatever is still running finish its own bookkeeping. On shutdown the
        // supervisor's 75s grace drops this future if the wait outlasts it — the documented
        // path: nothing was persisted for an aborted item and its lease recovers via
        // requeue_stale.
        while inflight.next().await.is_some() {}
    }

    /// run_one processes a single claimed item end to end: bounded handle, then complete-or-fail
    /// bookkeeping. Split out of `drain_all` so N of these can be in flight at once.
    ///
    /// Note what concurrency does to [`Pulse`]: the beat is shared, so a busy drain now beats for
    /// whichever item moved last and the watchdog can no longer see one wedged item behind others
    /// making progress. That is acceptable because the per-item `handler_timeout` — not the
    /// watchdog — is the guard for a hung handler; the watchdog remains the backstop for a drain
    /// where *everything* has stopped, which still shows up as a stale beat.
    /// Returns the stage's name so the drain can decrement that stage's in-flight count.
    async fn run_one(
        &self,
        handler: &dyn StageHandler,
        item: work::Item,
        pulse: &Pulse,
    ) -> &'static str {
        let stage = handler.stage();
        pulse.beat(&format!(
            "handle {stage} {}/{} {}",
            item.entity_type, item.entity_id, item.sport
        ));
        let outcome = self.handle_bounded(handler, &item).await;
        pulse.beat(&format!("bookkeep {stage}"));
        match outcome {
            Ok(()) => {
                if let Err(e) = work::complete(&self.pool, &item).await {
                    error!(error = %format!("{e:#}"), %stage, "complete failed");
                }
                // The Oracle's completion barrier, offered from exactly one place and only AFTER
                // the row is gone. Pillar handlers used to ask this themselves, which was correct
                // while the drain ran one item at a time and is not correct now: with several
                // items in flight, two pillars for the same entity could both ask while both rows
                // were still 'running', both see the other outstanding, and both decline — the
                // entity never crowned, with nothing in any log to say so. Asking after completion
                // makes the question monotone, so whoever finishes last always sees zero.
                //
                // Best-effort by design. A failed enqueue must not fail an item whose real work is
                // already persisted and whose row is already deleted; the next pillar to settle for
                // this entity asks again, and Sigil's input-hash debounce makes a duplicate cheap.
                if work::PILLAR_STAGES.contains(&item.stage) {
                    if let Err(e) = crate::junctions::oracle::enqueue_oracle_if_pillars_settled(
                        &self.pool,
                        &item.entity_type,
                        item.entity_id,
                        &item.sport,
                        item.input_version.clone(),
                    )
                    .await
                    {
                        warn!(
                            error = %format!("{e:#}"), %stage, entity = item.entity_id,
                            "oracle barrier check failed (best-effort)"
                        );
                    }
                }
            }
            Err(e) => {
                let backoff = retry_backoff(item.attempts);
                warn!(
                    error = %format!("{e:#}"),
                    %stage,
                    entity = item.entity_id,
                    backoff_secs = backoff.as_secs(),
                    "handler failed; backing off"
                );
                if let Err(e2) =
                    work::fail(&self.pool, &item, &format!("{e:#}"), backoff, MAX_ATTEMPTS).await
                {
                    error!(error = %format!("{e2:#}"), %stage, "fail bookkeeping failed");
                }
            }
        }
        stage.as_str()
    }

    /// handle_bounded wraps one handler run in the per-item timeout. A timed-out item
    /// fails with normal backoff — visible, retryable, and it cannot stall the drain
    /// (the watchdog stays the backstop for hangs outside handlers).
    async fn handle_bounded(&self, handler: &dyn StageHandler, item: &work::Item) -> Result<()> {
        if self.handler_timeout.is_zero() {
            return handler.handle(&self.harness, item).await;
        }
        match tokio::time::timeout(self.handler_timeout, handler.handle(&self.harness, item)).await
        {
            Ok(res) => res,
            Err(_) => Err(anyhow!(
                "handler exceeded COGNITION_HANDLER_TIMEOUT_SECONDS ({}s)",
                self.handler_timeout.as_secs()
            )),
        }
    }

    /// release_rest hands a shutdown-interrupted batch's unprocessed claims straight
    /// back to 'pending' so the next boot picks them up immediately instead of
    /// waiting out the 30-min stale-lease recovery.
    async fn release_rest(&self, rest: &[work::Item]) {
        info!(
            released = rest.len(),
            "shutdown: releasing unprocessed claims"
        );
        for item in rest {
            if let Err(e) = work::release(&self.pool, item).await {
                warn!(
                    error = %format!("{e:#}"),
                    stage = %item.stage,
                    entity = item.entity_id,
                    "release failed; stale-lease recovery will pick it up"
                );
            }
        }
    }
}

fn note_supervisor_exit(exit: std::result::Result<(), tokio::task::JoinError>) {
    match exit {
        Ok(()) => info!("supervisor completed shutdown handoff"),
        Err(e) => error!(error = %format!("{e:#}"), "supervisor task failed"),
    }
}

#[cfg(test)]
mod tests {
    use crate::stage::ARCHBOX_GEMMA_SLOTS;
    use super::*;

    #[test]
    fn stalled_requires_busy_and_age_past_threshold() {
        let threshold = Duration::from_secs(2700);
        assert!(!stalled(false, Duration::from_secs(9999), threshold)); // idle never stalls
        assert!(!stalled(true, Duration::from_secs(2699), threshold)); // under threshold
        assert!(stalled(true, Duration::from_secs(2700), threshold)); // at threshold
        assert!(stalled(true, Duration::from_secs(9999), threshold));
    }

    #[test]
    fn stalled_zero_threshold_disables_watchdog() {
        assert!(!stalled(
            true,
            Duration::from_secs(u64::MAX / 2),
            Duration::ZERO
        ));
    }

    /// Production's shape after the Phase 9 demolition (9.1): graph and the Editor share the
    /// archbox card's 4 slots rather than splitting them, then the single-slot stages.
    ///
    /// `scrub` and `article_read` left this table with the legacy rail — the shared group is now
    /// graph + editor. The ceiling arithmetic below is unchanged in KIND (a group still counts
    /// once, at its budget). The single-slot roster here trails production by one — main.rs also
    /// registers `fixture_boxscore` — so the ceiling this asserts is one stage BELOW the live
    /// ceiling: still a valid lower-bound regression check, noted so nobody scores it as exact
    /// (2026-08-10 audit).
    const GEMMA: Option<(&'static str, usize)> = Some(ARCHBOX_GEMMA_SLOTS);
    /// Grouped caps mirror production's `max_in_flight()` = `ARCHBOX_GEMMA_SLOTS.1`, DERIVED so a
    /// re-size of the card's slot budget (4 → 6 on 2026-08-09) cannot strand this fixture.
    const LIVE_CAPS: &[StageCap] = &[
        (ARCHBOX_GEMMA_SLOTS.1, GEMMA), // graph
        (ARCHBOX_GEMMA_SLOTS.1, GEMMA), // editor
        (1, None),                      // investigate_entity
        (1, None),                      // transfers
        (1, None),                      // narratives
        (1, None),                      // vibe
        (1, None),                      // peak
        (1, None),                      // momentum
        (1, None),                      // sigil
    ];

    #[test]
    fn unset_drain_concurrency_counts_a_shared_group_once() {
        // The whole archbox card once + 7 single-slot stages — the card's capacity does not
        // change with membership, only who may use it. Summing both grouped stages would admit
        // claims the host cannot run.
        assert_eq!(
            resolve_drain_concurrency(None, LIVE_CAPS),
            ARCHBOX_GEMMA_SLOTS.1 + 7
        );
    }

    #[test]
    fn the_derived_ceiling_never_binds_before_the_stage_caps_do() {
        // The property that matters: with the ceiling unset, every stage can reach its own cap
        // simultaneously. If this ever fails, some stage is being starved by the global number.
        // Grouped stages count once, at the group budget — that IS the most they can hold at once.
        let budget = resolve_drain_concurrency(None, LIVE_CAPS);
        let mut total = 0usize;
        let mut seen: Vec<&str> = Vec::new();
        for (cap, group) in LIVE_CAPS {
            match group {
                Some((name, b)) => {
                    if !seen.contains(name) {
                        seen.push(name);
                        total += b;
                    }
                }
                None => total += cap,
            }
        }
        assert!(budget >= total, "budget {budget} would starve; caps total {total}");
    }

    #[test]
    fn a_shared_group_lends_idle_slots_and_takes_them_back() {
        let (_, budget) = ARCHBOX_GEMMA_SLOTS;

        // graph idle: the Editor may take the whole card. This is the change — it was pinned to 2
        // while 5,852 reads queued against a card that was half asleep.
        let group_running = 0;
        assert_eq!(
            stage_room(budget, 0, 10).min(budget - group_running),
            budget
        );

        // graph holding 2: the Editor is held to the remainder, never oversubscribing the host.
        let group_running = 2;
        assert_eq!(
            stage_room(budget, 0, 10).min(budget - group_running),
            budget - 2
        );

        // graph holding the whole card: the Editor waits rather than deepening the host's queue.
        let group_running = budget;
        assert_eq!(stage_room(budget, 0, 10).min(budget - group_running), 0);
    }

    #[test]
    fn an_explicit_ceiling_overrides_the_caps_and_can_throttle() {
        assert_eq!(resolve_drain_concurrency(Some(3), LIVE_CAPS), 3);
        // 1 is the documented rollback: the old strictly-sequential drain.
        assert_eq!(resolve_drain_concurrency(Some(1), LIVE_CAPS), 1);
        // 0 would wedge the drain forever, so it clamps.
        assert_eq!(resolve_drain_concurrency(Some(0), LIVE_CAPS), 1);
    }

    #[test]
    fn no_handlers_still_yields_a_usable_ceiling() {
        assert_eq!(resolve_drain_concurrency(None, &[]), 1);
    }

    #[test]
    fn stage_room_stops_one_stage_owning_the_budget() {
        // graph at cap 2 with nothing running may take 2 of a wide-open budget -- NOT the 8 its
        // rotation_batch asks for, which is what starved every stage behind it.
        assert_eq!(stage_room(2, 0, 10), 2);
        // One already running leaves one.
        assert_eq!(stage_room(2, 1, 10), 1);
        // At its cap it is skipped even with budget to spare.
        assert_eq!(stage_room(2, 2, 10), 0);
        // A near-full budget clamps below the stage's own cap.
        assert_eq!(stage_room(2, 0, 1), 1);
        assert_eq!(stage_room(2, 0, 0), 0);
        // A cap of 0 would silently disable a stage; treat it as 1.
        assert_eq!(stage_room(0, 0, 10), 1);
    }

    #[test]
    fn pulse_tracks_busy_beat_and_activity() {
        let pulse = Pulse::new();
        let (busy, _, activity) = pulse.snapshot();
        assert!(!busy);
        assert_eq!(activity, "idle");

        pulse.begin("requeue-stale");
        let (busy, age, activity) = pulse.snapshot();
        assert!(busy);
        assert!(age < Duration::from_secs(5));
        assert_eq!(activity, "requeue-stale");

        pulse.beat("handle vibe team/7 NBA");
        let (busy, _, activity) = pulse.snapshot();
        assert!(busy);
        assert_eq!(activity, "handle vibe team/7 NBA");

        pulse.idle();
        let (busy, _, activity) = pulse.snapshot();
        assert!(!busy);
        assert_eq!(activity, "idle");
    }
}

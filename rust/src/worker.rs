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
use crate::stage::StageHandler;
use crate::work::{self, BACKOFF, CLAIM_BATCH, MAX_ATTEMPTS};
use anyhow::{anyhow, Result};
use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info, warn};

const NOTIFY_CHANNEL: &str = "pipeline_work_ready";

/// Supervisor cadence for watchdog progress checks.
const WATCHDOG_POLL: Duration = Duration::from_secs(60);

/// How long a shutdown signal waits for the in-flight item before the drain is
/// dropped mid-await. Must stay under systemd's TimeoutStopSec (default 90s) so a
/// stop never escalates to SIGKILL: signal → flag (drain exits at the next item
/// boundary) → grace → abort → clean exit.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(75);

/// Pulse is the drain's progress instrument, shared with the supervisor. The drain
/// beats it at every step boundary (claim, per-item handle, bookkeeping, topic-heat);
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

/// stalled is the watchdog predicate: a drain that claims to be busy but whose last
/// beat is older than `threshold` is wedged — a healthy drain beats at every item and
/// step boundary, and the longest legitimately beat-free stretch (the topic-heat full
/// pass) measured ~24 min, under the 45-min default. Zero disables. Pure for tests.
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
    topic_heat_interval: Duration,
    /// Per-item ceiling on one stage handler run (`COGNITION_HANDLER_TIMEOUT_SECONDS`;
    /// zero disables). A hung await inside a handler fails the item after this long
    /// instead of stalling the drain forever.
    handler_timeout: Duration,
    /// The supervisor's no-progress threshold (`COGNITION_WATCHDOG_SECONDS`; zero
    /// disables): a busy drain whose heartbeat is older than this exits the process.
    watchdog: Duration,
    /// Set by the supervisor on SIGINT/SIGTERM; the drain exits at the next boundary.
    shutdown: Arc<AtomicBool>,
    /// Latest tick cause, written by the supervisor with each request.
    cause: Arc<StdMutex<&'static str>>,
    last_topic_heat: Mutex<Option<Instant>>,
    /// Cross-pass embedding cache — steady-state refreshes embed only new/changed articles.
    topic_heat_cache: Mutex<crate::bucket::TopicHeatCache>,
}

impl Worker {
    pub fn new(
        harness: Harness,
        handlers: Vec<Box<dyn StageHandler>>,
        safety_net: Duration,
        stale_lease: Duration,
        topic_heat_interval: Duration,
        handler_timeout: Duration,
        watchdog: Duration,
    ) -> Self {
        let pool = harness.pool.clone();
        Self {
            pool,
            harness,
            handlers,
            safety_net,
            stale_lease,
            topic_heat_interval,
            handler_timeout,
            watchdog,
            shutdown: Arc::new(AtomicBool::new(false)),
            cause: Arc::new(StdMutex::new("startup")),
            last_topic_heat: Mutex::new(None),
            topic_heat_cache: Mutex::new(crate::bucket::TopicHeatCache::new()),
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
        // Drain BEFORE the topic-heat refresh: pending cognition work never waits behind the
        // CPU embedder (measured 2026-07-12 — a full refresh pass took ~24 min and starved the
        // queue for the whole worker tick).
        self.drain_all(cause, pulse).await;
        pulse.beat("topic-heat refresh");
        self.maybe_refresh_topic_heat(cause, pulse).await;
        pulse.idle();
    }

    async fn maybe_refresh_topic_heat(&self, cause: &str, pulse: &Pulse) {
        if self.topic_heat_interval.is_zero() {
            return;
        }
        let mut last = self.last_topic_heat.lock().await;
        let due = match *last {
            None => true,
            Some(t) => t.elapsed() >= self.topic_heat_interval,
        };
        if !due {
            return;
        }
        *last = Some(Instant::now());
        drop(last);

        let mut cache = self.topic_heat_cache.lock().await;
        let beat = |activity: &str| pulse.beat(activity);
        match crate::bucket::refresh_topic_heat(&self.harness, &mut cache, &beat).await {
            Ok(r) if r.updated > 0 => info!(
                updated = r.updated,
                embedded = r.embedded,
                cached = r.cached,
                cause,
                "refreshed topic heat"
            ),
            Ok(_) => {}
            Err(e) => warn!(error = %format!("{e:#}"), cause, "topic heat refresh failed"),
        }
    }

    /// Drain every registered stage to empty. Iterates in registration order;
    /// when stages are added, register them in dependency order (transfers,
    /// narratives, vibe, sigil) to match the Go `DrainAll` sequence.
    async fn drain_all(&self, cause: &str, pulse: &Pulse) {
        for handler in &self.handlers {
            let stage = handler.stage();
            loop {
                if self.shutting_down() {
                    return;
                }
                pulse.beat(&format!("claim {stage}"));
                let items = match work::claim(&self.pool, stage, CLAIM_BATCH).await {
                    Ok(items) => items,
                    Err(e) => {
                        error!(error = %format!("{e:#}"), %stage, cause, "claim failed");
                        break;
                    }
                };
                if items.is_empty() {
                    break;
                }
                debug!(%stage, n = items.len(), cause, "draining batch");
                for (idx, item) in items.iter().enumerate() {
                    if self.shutting_down() {
                        self.release_rest(&items[idx..]).await;
                        return;
                    }
                    pulse.beat(&format!(
                        "handle {stage} {}/{} {}",
                        item.entity_type, item.entity_id, item.sport
                    ));
                    let outcome = self.handle_bounded(handler.as_ref(), item).await;
                    pulse.beat(&format!("bookkeep {stage}"));
                    match outcome {
                        Ok(()) => {
                            if let Err(e) = work::complete(&self.pool, item).await {
                                error!(error = %format!("{e:#}"), %stage, "complete failed");
                            }
                        }
                        Err(e) => {
                            warn!(error = %format!("{e:#}"), %stage, entity = item.entity_id, "handler failed; backing off");
                            if let Err(e2) =
                                work::fail(&self.pool, item, &format!("{e:#}"), BACKOFF, MAX_ATTEMPTS)
                                    .await
                            {
                                error!(error = %format!("{e2:#}"), %stage, "fail bookkeeping failed");
                            }
                        }
                    }
                }
            }
        }
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
        info!(released = rest.len(), "shutdown: releasing unprocessed claims");
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
        assert!(!stalled(true, Duration::from_secs(u64::MAX / 2), Duration::ZERO));
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

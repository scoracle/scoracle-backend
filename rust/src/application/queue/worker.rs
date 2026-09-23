//! Durable `pipeline_work` consumer with responsive supervision and handoff dispatch:
//!
//! * The **drain** (the `run` future itself) executes recover-then-drain ticks. It is
//!   the only task that touches stage handlers or the GPU.
//! * The **outbox** future is polled alongside both idle waiting and active draining,
//!   so committed handoffs cannot stall behind a model call or sustained queue inflow.
//! * The **supervisor** (spawned) owns everything that must stay responsive no matter
//!   what the drain is doing: the Postgres LISTEN socket (always read — a slow or
//!   wedged drain can no longer pin the NOTIFY queue), the safety-net timer,
//!   SIGINT/SIGTERM, and the no-progress watchdog.
//!
//! Tick requests flow supervisor → drain through a [`tokio::sync::Notify`] whose
//! single stored permit coalesces a burst into one follow-up tick. Per-item timeouts fail
//! handlers with normal backoff; [`Pulse`] lets the supervisor restart a wholly wedged drain.
//!
//! Shutdown: either signal sets a flag the drain checks at every item boundary
//! (releasing unprocessed claims straight back to 'pending'), then a 75s in-process
//! grace aborts a stuck in-flight item — always inside systemd's 90s TimeoutStopSec,
//! so a stop/restart never escalates to SIGKILL.

use crate::application::queue::work::{self, retry_backoff, TaskKey, MAX_ATTEMPTS};
use crate::studio::plugin::{PluginRegistry, ScheduledOperation};
use anyhow::{anyhow, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

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
/// model call overrides this via [`WorkHandler::rotation_batch`] — see the note there.
const STAGE_ROTATION_BATCH: i64 = 1;

/// Stale-lease recovery cadence, independent of queue depth.
const STALE_RECOVERY_INTERVAL: Duration = Duration::from_secs(60);

// Durable handoffs must progress even while a drain never reaches empty or a
// model call is waiting. Bound each batch so the dispatcher yields to model work.
const OUTBOX_INTERVAL: Duration = Duration::from_secs(1);
const OUTBOX_TIMEOUT: Duration = Duration::from_secs(30);

async fn outbox_loop(
    pool: &PgPool,
    reactions: &crate::application::queue::outbox::ReactionRegistry,
    tick: &Notify,
    shutdown: &AtomicBool,
) {
    let mut interval = tokio::time::interval(OUTBOX_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if shutdown.load(Ordering::Acquire) {
            // The worker's existing grace policy owns shutdown. Do not end the
            // outer select and prematurely drop its in-flight handlers.
            std::future::pending::<()>().await;
        }
        match tokio::time::timeout(
            OUTBOX_TIMEOUT,
            crate::application::queue::outbox::drain(pool, reactions, 100),
        )
        .await
        {
            Ok(Ok(n)) if n > 0 => {
                info!(reconciled = n, "application outbox drained");
                tick.notify_one();
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => error!(error = %format!("{e:#}"), "application outbox drain failed"),
            Err(_) => {
                warn!("application outbox batch timed out; unacknowledged events remain retryable")
            }
        }
    }
}

/// Drain heartbeat shared with the supervisor. `activity` names the last step for watchdog logs.
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

    #[cfg(test)]
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

/// Pick the drain's global in-flight ceiling. An explicit
/// `COGNITION_DRAIN_CONCURRENCY` wins; otherwise it is derived from the stages' own caps, which
/// by construction can never bind.
///
/// Grouped tasks contribute their group's budget once, not each task's ceiling. Two tasks may
/// each claim up to four but share only four slots, so summing both would inflate the global
/// budget by capacity that cannot be used at once.
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

/// Run plugin-owned maintenance independently of queue depth. The host owns only
/// invocation and shutdown; domain work and cadence live behind the operation.
async fn scheduled_loop(operation: Arc<dyn ScheduledOperation>, shutdown: Arc<AtomicBool>) {
    let mut cause = "startup";
    loop {
        if shutdown.load(Ordering::Acquire) {
            debug!(
                operation = operation.name(),
                "scheduled operation stopped cleanly"
            );
            return;
        }
        let interval = operation.run(cause).await;
        cause = "interval";
        debug!(
            operation = operation.name(),
            interval_secs = interval.as_secs(),
            "scheduled operation cadence"
        );
        tokio::time::sleep(interval).await;
    }
}

pub struct Worker {
    // Queue, recovery, and maintenance use storage; model dependencies stay in plugins.
    pool: PgPool,
    /// The registered cognitive fleet. The drain resolves each claim through the
    /// registry rather than indexing a bare handler list, so task ownership is
    /// validated once at boot instead of implied by construction order. Scheduling
    /// caps are read from each plugin's manifest `ResourceProfile`.
    fleet: PluginRegistry,
    reactions: crate::application::queue::outbox::ReactionRegistry,
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
}

impl Worker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: PgPool,
        handlers: Vec<std::sync::Arc<dyn crate::studio::plugin::StudioPlugin>>,
        reactions: crate::application::queue::outbox::ReactionRegistry,
        safety_net: Duration,
        stale_lease: Duration,
        handler_timeout: Duration,
        watchdog: Duration,
        drain_concurrency: Option<usize>,
    ) -> Self {
        // Unset means "let the per-stage caps govern": the sum of every stage's `max_in_flight`
        // is by construction the point past which the ceiling can never bind, so no stage is
        // ever starved by a global number nobody tuned. An explicit value is a throttle.
        // Caps are read from the plugins' manifests — the resource profile is the single source.
        let caps: StageCaps = handlers
            .iter()
            .map(|p| {
                let r = &p.manifest().resources;
                (r.max_in_flight, r.slot_group)
            })
            .collect();
        let drain_concurrency = resolve_drain_concurrency(drain_concurrency, &caps);
        // A duplicate task ownership or inconsistent manifest fails here, at
        // construction, rather than at claim time.
        let fleet =
            PluginRegistry::new(handlers).expect("registered plugins must form a valid fleet");
        Self {
            pool,
            fleet,
            reactions,
            safety_net,
            stale_lease,
            handler_timeout,
            watchdog,
            drain_concurrency,
            shutdown: Arc::new(AtomicBool::new(false)),
            cause: Arc::new(StdMutex::new("startup")),
        }
    }

    /// The registered fleet in registration order.
    pub fn plugins(&self) -> &[std::sync::Arc<dyn crate::studio::plugin::StudioPlugin>] {
        self.fleet.plugins()
    }

    /// Run until SIGINT/SIGTERM. Drains on start, on NOTIFY, and on the safety-net
    /// tick — all delivered as coalesced tick requests from the supervisor task.
    pub async fn run(&self) -> Result<()> {
        let stages: Vec<&'static str> = self
            .fleet
            .plugins()
            .iter()
            .map(|p| p.manifest().task.as_str())
            .collect();
        let plugins: Vec<&'static str> = self
            .fleet
            .plugins()
            .iter()
            .map(|p| p.manifest().id.as_str())
            .collect();
        info!(?stages, ?plugins, "cognition harness worker starting");
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

        // Poll alongside both idle waits and active drains. Keeping this future
        // owned by run() makes cancellation release its transaction/row locks;
        // it cannot outlive the worker or depend on a queue-empty tick boundary.
        let outbox = outbox_loop(&self.pool, &self.reactions, &tick, &self.shutdown);
        tokio::pin!(outbox);

        for operation in self
            .fleet
            .plugins()
            .iter()
            .flat_map(|plugin| plugin.scheduled_operations())
        {
            info!(
                operation = operation.name(),
                "scheduled operation starting (independent of the drain)"
            );
            tokio::spawn(scheduled_loop(operation, self.shutdown.clone()));
        }

        // Recover crashed or aborted claims on the lease clock, independent of drain depth.
        {
            let pool = self.pool.clone();
            let lease = self.stale_lease;
            let shutdown = self.shutdown.clone();
            info!(
                interval_secs = STALE_RECOVERY_INTERVAL.as_secs(),
                lease_secs = lease.as_secs(),
                "stale-lease recovery loop starting (own task, independent of the drain)"
            );
            tokio::spawn(async move {
                loop {
                    if shutdown.load(Ordering::Acquire) {
                        debug!("stale-lease recovery loop stopped cleanly");
                        return;
                    }
                    match work::requeue_stale(&pool, lease).await {
                        Ok(n) if n > 0 => info!(recovered = n, "requeued stale work"),
                        Ok(_) => {}
                        Err(e) => error!(error = %format!("{e:#}"), "requeue stale failed"),
                    }
                    tokio::time::sleep(STALE_RECOVERY_INTERVAL).await;
                }
            });
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
                _ = &mut outbox, if !self.fleet.plugins().is_empty() => {
                    return Err(anyhow!("application outbox dispatcher exited"));
                }
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
                _ = &mut outbox, if !self.fleet.plugins().is_empty() => {
                    return Err(anyhow!("application outbox dispatcher exited"));
                }
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
        if self.fleet.plugins().is_empty() {
            debug!(cause, "tick: no handlers registered; nothing to do");
            return;
        }
        // Durable handoffs are polled independently by run(), even while this
        // tick stays in drain_all under sustained inflow.
        self.drain_all(cause, pulse).await;
        pulse.idle();
    }

    /// Drain every registered stage to empty, keeping up to `drain_concurrency` claimed items
    /// in flight at once.
    ///
    /// **The governor is the scheduler.** The drain's job is only to keep work OFFERED; the
    /// per-host semaphores (`route.rs::governor_for` — one host today, more again if a role's
    /// `_BASE_URL` moves) decide what actually runs. Archbox pulls continuously up to its slot
    /// count; the per-stage caps and the shared `ARCHBOX_SLOTS` group decide who holds them.
    ///
    /// Rotate the first claimant after each successful claim. Fixed registration
    /// order starves later stages when earlier stages continuously refill shared
    /// slots, even when every individual stage stays below its own cap.
    ///
    /// Concurrency is *intra-task*: the futures live in a `FuturesUnordered` polled by this one
    /// drain task and are never spawned, so handlers cannot pin the supervisor's LISTEN socket.
    async fn drain_all(&self, cause: &str, pulse: &Pulse) {
        let budget = self.drain_concurrency.max(1);
        let mut inflight = FuturesUnordered::new();
        // In-flight items per stage, so no one stage can own the whole budget. Keyed by the
        // stage's static name because `TaskKey` is not `Hash`.
        let mut per_stage: HashMap<&'static str, usize> = HashMap::new();
        // In-flight per slot group, so stages sharing one backend's parallel slots divide them on
        // demand instead of by a fixed split. Keyed by group name; ungrouped stages never appear.
        let mut per_group: HashMap<&'static str, usize> = HashMap::new();
        // stage name -> its group, so a finishing item can decrement the right counter without
        // asking the handler again.
        let group_of: HashMap<&'static str, (&'static str, usize)> = self
            .fleet
            .plugins()
            .iter()
            .filter_map(|p| {
                p.manifest()
                    .resources
                    .slot_group
                    .map(|g| (p.manifest().task.as_str(), g))
            })
            .collect();

        let mut next_handler = 0;
        loop {
            // Resume after the last successful claimant, including when the
            // global budget or a shared model-slot group was exhausted.
            let mut claimed_any = false;
            let start = next_handler;
            let plugins = self.fleet.plugins();
            for offset in 0..plugins.len() {
                let index = (start + offset) % plugins.len();
                let plugin = &plugins[index];
                if self.shutting_down() || inflight.len() >= budget {
                    break;
                }
                let manifest = plugin.manifest();
                let stage = manifest.task;
                // Per-stage caps keep DAG claim order from becoming strict priority order.
                let running = *per_stage.get(stage.as_str()).unwrap_or(&0);
                let mut room = stage_room(
                    manifest.resources.max_in_flight,
                    running,
                    budget - inflight.len(),
                );
                // A grouped task is additionally bounded by what its co-tenants have left. This
                // lets one task borrow idle slots without oversubscribing the shared backend.
                if let Some((name, group_budget)) = manifest.resources.slot_group {
                    let group_running = *per_group.get(name).unwrap_or(&0);
                    room = room.min(group_budget.saturating_sub(group_running));
                }
                if room == 0 {
                    continue;
                }
                pulse.beat(&format!("claim {stage}"));
                let batch = manifest
                    .resources
                    .rotation_batch
                    .max(STAGE_ROTATION_BATCH)
                    .min(room as i64);
                let items =
                    match work::claim_with_policy(&self.pool, stage, manifest.claim_policy, batch)
                        .await
                    {
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
                next_handler = (index + 1) % plugins.len();
                debug!(%stage, n = items.len(), cause, "draining batch");
                *per_stage.entry(stage.as_str()).or_insert(0) += items.len();
                if let Some((name, _)) = manifest.resources.slot_group {
                    *per_group.entry(name).or_insert(0) += items.len();
                }
                for item in items {
                    inflight.push(self.run_one(plugin.as_ref(), item, pulse));
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
        plugin: &dyn crate::studio::plugin::StudioPlugin,
        item: work::Item,
        pulse: &Pulse,
    ) -> &'static str {
        let stage = plugin.manifest().task;
        pulse.beat(&format!(
            "handle {stage} {}/{} {}",
            item.entity_type, item.entity_id, item.sport
        ));
        let outcome = self.execute_bounded(plugin, &item).await;
        pulse.beat(&format!("bookkeep {stage}"));
        match outcome {
            Ok(crate::studio::plugin::PluginOutcome::Deferred {
                made_progress,
                note,
                delay,
            }) => {
                // Deferral is a progress-guaranteed contract. A round that resolved
                // nothing falls to the retry ladder instead of getting a free turn.
                if made_progress {
                    match work::defer(&self.pool, &item, delay, &note).await {
                        Ok(true) => debug!(
                            %stage, entity = item.entity_id, %note, "plugin deferred partial progress"
                        ),
                        Ok(false) => debug!(
                            %stage, entity = item.entity_id, "defer ignored: claim was superseded"
                        ),
                        Err(e) => {
                            error!(error = %format!("{e:#}"), %stage, "defer bookkeeping failed")
                        }
                    }
                } else {
                    warn!(%stage, entity = item.entity_id, %note, "defer without progress; retry ladder applies");
                    self.fail_claimed(&item, &stage, &note).await;
                }
            }
            Ok(crate::studio::plugin::PluginOutcome::Committed) => debug!(
                %stage,
                entity = item.entity_id,
                "plugin committed product, follow-up intent, and exact claim"
            ),
            Ok(crate::studio::plugin::PluginOutcome::Superseded) => debug!(
                %stage,
                entity = item.entity_id,
                "plugin publication skipped: claim was superseded"
            ),
            Err(e) => {
                self.fail_claimed(&item, &stage, &format!("{e:#}")).await;
            }
        }
        stage.as_str()
    }

    /// fail_claimed walks the retry ladder for a failed item: visible backoff,
    /// retryable, dead-letter at MAX_ATTEMPTS.
    async fn fail_claimed(&self, item: &work::Item, stage: &TaskKey, cause: &str) {
        let backoff = retry_backoff(item.attempts);
        warn!(
            error = %cause,
            %stage,
            entity = item.entity_id,
            backoff_secs = backoff.as_secs(),
            "plugin failed; backing off"
        );
        match work::fail(&self.pool, item, cause, backoff, MAX_ATTEMPTS).await {
            Ok(true) => {}
            Ok(false) => debug!(
                %stage,
                entity = item.entity_id,
                "failure ignored: claim was superseded"
            ),
            Err(e2) => error!(error = %format!("{e2:#}"), %stage, "fail bookkeeping failed"),
        }
    }

    /// execute_bounded wraps one plugin run in the per-item timeout. A timed-out item
    /// fails with normal backoff — visible, retryable, and it cannot stall the drain
    /// (the watchdog stays the backstop for hangs outside plugins).
    async fn execute_bounded(
        &self,
        plugin: &dyn crate::studio::plugin::StudioPlugin,
        item: &work::Item,
    ) -> Result<crate::studio::plugin::PluginOutcome> {
        if self.handler_timeout.is_zero() {
            return plugin.execute(item).await;
        }
        match tokio::time::timeout(self.handler_timeout, plugin.execute(item)).await {
            Ok(res) => res,
            Err(_) => Err(anyhow!(
                "plugin exceeded COGNITION_HANDLER_TIMEOUT_SECONDS ({}s)",
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
            match work::release(&self.pool, item).await {
                Ok(true) => {}
                Ok(false) => debug!(
                    stage = %item.stage,
                    entity = item.entity_id,
                    "release ignored: claim was superseded"
                ),
                Err(e) => {
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
}

fn note_supervisor_exit(exit: std::result::Result<(), tokio::task::JoinError>) {
    match exit {
        Ok(()) => info!("supervisor completed shutdown handoff"),
        Err(e) => error!(error = %format!("{e:#}"), "supervisor task failed"),
    }
}

#[cfg(test)]
mod test_support {
    use crate::application::queue::work::ClaimPolicy;
    use crate::studio::plugin::{PluginId, PluginManifest, ResourceProfile};

    pub(super) static GRAPH_TEST_MANIFEST: PluginManifest = PluginManifest {
        id: PluginId::new("test.graph"),
        task: crate::plugins::graph::manifest::TASK,
        claim_policy: ClaimPolicy::FIFO,
        inference_routes: &[],
        resources: ResourceProfile::unbounded_batch(1),
        tools: &[],
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::support::resources::ARCHBOX_SLOTS;
    use crate::studio::plugin::{PluginOutcome, StudioPlugin};

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

    /// Production's shape after the 2026-08-20 consolidation: every model stage shares the
    /// archbox card — graph and the Editor at the full group budget, the four voices capped
    /// at 2 within it — then the single-slot stages (investigate/transfers/momentum).
    ///
    /// The ceiling arithmetic below is unchanged in KIND (a group still counts once, at its
    /// budget). The single-slot roster here trails production by one — main.rs also registers
    /// `fixture_boxscore` — so the ceiling this asserts is one stage BELOW the live ceiling:
    /// still a valid lower-bound regression check, noted so nobody scores it as exact
    /// (2026-08-10 audit).
    const CARD: Option<(&'static str, usize)> = Some(ARCHBOX_SLOTS);
    /// Grouped caps mirror production's `max_in_flight()`, DERIVED from `ARCHBOX_SLOTS.1` where
    /// production derives, so a re-size of the card's slot budget cannot strand this fixture.
    const LIVE_CAPS: &[StageCap] = &[
        (ARCHBOX_SLOTS.1, CARD), // graph
        (ARCHBOX_SLOTS.1, CARD), // editor
        (1, None),               // investigate_entity
        (1, None),               // transfers
        (2, CARD),               // narratives
        (2, CARD),               // vibe
        (2, CARD),               // peak
        (1, None),               // momentum
        (2, CARD),               // sigil
    ];

    #[test]
    fn unset_drain_concurrency_counts_a_shared_group_once() {
        // The whole archbox card once + 3 single-slot stages — the card's capacity does not
        // change with membership, only who may use it. Summing the grouped stages would admit
        // claims the host cannot run.
        assert_eq!(
            resolve_drain_concurrency(None, LIVE_CAPS),
            ARCHBOX_SLOTS.1 + 3
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
        assert!(
            budget >= total,
            "budget {budget} would starve; caps total {total}"
        );
    }

    #[test]
    fn a_shared_group_lends_idle_slots_and_takes_them_back() {
        let (_, budget) = ARCHBOX_SLOTS;

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

    use super::test_support::GRAPH_TEST_MANIFEST;

    struct PreparedHandler(Option<PluginOutcome>);

    #[async_trait::async_trait]
    impl StudioPlugin for PreparedHandler {
        fn manifest(&self) -> &'static crate::studio::plugin::PluginManifest {
            &GRAPH_TEST_MANIFEST
        }
        async fn execute(&self, item: &work::Item) -> Result<PluginOutcome> {
            assert_eq!(item.claim_token.as_deref(), Some("exact-lease"));
            assert_eq!(item.input_version.as_deref(), Some("captured-revision"));
            match &self.0 {
                Some(outcome) => Ok(outcome.clone()),
                None => std::future::pending().await,
            }
        }
    }

    fn offline_worker(timeout: Duration) -> Worker {
        Worker::new(
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgresql://localhost/unused")
                .unwrap(),
            Vec::new(),
            crate::application::queue::outbox::ReactionRegistry::empty(),
            Duration::from_secs(60),
            Duration::from_secs(60),
            timeout,
            Duration::ZERO,
            None,
        )
    }

    fn prepared_claim() -> work::Item {
        work::Item {
            stage: crate::plugins::graph::manifest::TASK,
            entity_type: "article".into(),
            entity_id: 1,
            sport: "NBA".into(),
            input_version: Some("captured-revision".into()),
            claim_token: Some("exact-lease".into()),
            attempts: 1,
        }
    }

    #[tokio::test]
    async fn bound_handlers_preserve_exact_claim_and_all_durable_receipts_without_services() {
        for timeout in [Duration::ZERO, Duration::from_secs(1)] {
            let worker = offline_worker(timeout);
            for expected in [
                PluginOutcome::Committed,
                PluginOutcome::Superseded,
                PluginOutcome::deferred("test", Duration::ZERO),
            ] {
                assert_eq!(
                    worker
                        .execute_bounded(
                            &PreparedHandler(Some(expected.clone())),
                            &prepared_claim()
                        )
                        .await
                        .unwrap(),
                    expected
                );
            }
        }
    }

    #[tokio::test]
    async fn bound_handler_timeout_remains_a_retryable_error() {
        let worker = offline_worker(Duration::from_millis(1));
        let error = worker
            .execute_bounded(&PreparedHandler(None), &prepared_claim())
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("plugin exceeded COGNITION_HANDLER_TIMEOUT_SECONDS"));
    }
}

#[cfg(test)]
mod postgres_recovery_rehearsal {
    use super::test_support::GRAPH_TEST_MANIFEST;
    use super::*;
    use crate::application::queue::work::Item;
    use crate::studio::plugin::{PluginOutcome, StudioPlugin};

    async fn fixture(sport: &str) -> PgPool {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(8)
            .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES ($1,$1,2026) ON CONFLICT DO NOTHING")
            .bind(sport).execute(&pool).await.unwrap();
        for table in ["pipeline_work", "application_outbox"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
                .bind(sport)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    fn item(stage: TaskKey, sport: &str, id: i64) -> Item {
        Item {
            stage,
            sport: sport.into(),
            entity_type: "team".into(),
            entity_id: id,
            input_version: Some("v1".into()),
            attempts: 0,
            claim_token: None,
        }
    }

    struct Refilling {
        pool: PgPool,
        stage: TaskKey,
        shared: bool,
        order: Arc<StdMutex<Vec<TaskKey>>>,
    }

    const fn test_manifest(
        id: &'static str,
        task: TaskKey,
        shared: bool,
    ) -> crate::studio::plugin::PluginManifest {
        crate::studio::plugin::PluginManifest {
            id: crate::studio::plugin::PluginId::new(id),
            task,
            claim_policy: crate::application::queue::work::ClaimPolicy::FIFO,
            inference_routes: &[],
            resources: crate::studio::plugin::ResourceProfile {
                max_in_flight: 1,
                slot_group: if shared {
                    Some(("test-shared", 1))
                } else {
                    None
                },
                rotation_batch: 1,
            },
            tools: &[],
        }
    }

    const NARRATIVES_TEST: crate::studio::plugin::PluginManifest = test_manifest(
        "test.narratives",
        crate::plugins::journalist::manifest::TASK,
        false,
    );
    const RATING_TEST: crate::studio::plugin::PluginManifest =
        test_manifest("test.rating", crate::plugins::scout::manifest::TASK, false);
    const NARRATIVES_SHARED_TEST: crate::studio::plugin::PluginManifest = test_manifest(
        "test.narratives",
        crate::plugins::journalist::manifest::TASK,
        true,
    );
    const RATING_SHARED_TEST: crate::studio::plugin::PluginManifest =
        test_manifest("test.rating", crate::plugins::scout::manifest::TASK, true);

    impl Refilling {
        fn manifest(&self) -> &'static crate::studio::plugin::PluginManifest {
            match (self.stage, self.shared) {
                (crate::plugins::journalist::manifest::TASK, false) => &NARRATIVES_TEST,
                (crate::plugins::scout::manifest::TASK, false) => &RATING_TEST,
                (crate::plugins::journalist::manifest::TASK, true) => &NARRATIVES_SHARED_TEST,
                (crate::plugins::scout::manifest::TASK, true) => &RATING_SHARED_TEST,
                _ => unreachable!("test double stages"),
            }
        }
    }

    #[async_trait::async_trait]
    impl StudioPlugin for Refilling {
        fn manifest(&self) -> &'static crate::studio::plugin::PluginManifest {
            Refilling::manifest(self)
        }
        async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
            let count = {
                let mut order = self.order.lock().unwrap();
                order.push(self.stage);
                order.iter().filter(|s| **s == self.stage).count()
            };
            let mut tx = self.pool.begin().await?;
            assert!(work::lock_claim(&mut tx, item).await?);
            assert!(work::complete_in_transaction(&mut tx, item).await?);
            // Keep the first stage continuously ready while a later stage waits.
            if self.stage == crate::plugins::journalist::manifest::TASK && count < 20 {
                work::enqueue(&mut *tx, item).await?;
            }
            tx.commit().await?;
            Ok(PluginOutcome::Committed)
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn sustained_inflow_cannot_starve_later_stages_at_global_or_shared_capacity() {
        for shared in [false, true] {
            let sport = if shared {
                "ZZ_FAIR_SHARED"
            } else {
                "ZZ_FAIR_GLOBAL"
            };
            let pool = fixture(sport).await;
            let order = Arc::new(StdMutex::new(Vec::new()));
            let handlers: Vec<std::sync::Arc<dyn StudioPlugin>> = [
                crate::plugins::journalist::manifest::TASK,
                crate::plugins::scout::manifest::TASK,
            ]
            .into_iter()
            .map(|stage| {
                std::sync::Arc::new(Refilling {
                    pool: pool.clone(),
                    stage,
                    shared,
                    order: order.clone(),
                }) as std::sync::Arc<dyn StudioPlugin>
            })
            .collect();
            for stage in [
                crate::plugins::journalist::manifest::TASK,
                crate::plugins::scout::manifest::TASK,
            ] {
                work::enqueue(&pool, &item(stage, sport, 9_600_100))
                    .await
                    .unwrap();
            }
            let worker = Worker::new(
                pool.clone(),
                handlers,
                crate::application::queue::outbox::ReactionRegistry::empty(),
                Duration::from_secs(60),
                Duration::from_secs(1800),
                Duration::from_secs(5),
                Duration::ZERO,
                Some(if shared { 2 } else { 1 }),
            );
            tokio::time::timeout(
                Duration::from_secs(5),
                worker.drain_all("test", &Pulse::new()),
            )
            .await
            .unwrap();
            let order = order.lock().unwrap();
            assert_eq!(order.len(), 21);
            assert_eq!(
                order[1],
                crate::plugins::scout::manifest::TASK,
                "a continuously ready earlier stage monopolized capacity: {order:?}"
            );
        }
    }

    struct Blocked {
        pool: PgPool,
        entered: Arc<Notify>,
        release: Arc<Notify>,
    }
    #[async_trait::async_trait]
    impl StudioPlugin for Blocked {
        fn manifest(&self) -> &'static crate::studio::plugin::PluginManifest {
            &GRAPH_TEST_MANIFEST
        }
        async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
            self.entered.notify_one();
            self.release.notified().await;
            let mut tx = self.pool.begin().await?;
            assert!(work::lock_claim(&mut tx, item).await?);
            assert!(work::complete_in_transaction(&mut tx, item).await?);
            tx.commit().await?;
            Ok(PluginOutcome::Committed)
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn handoffs_dispatch_while_a_claimed_handler_cannot_finish() {
        let sport = "ZZ_BUSY_OUTBOX";
        let pool = fixture(sport).await;
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let worker = Worker::new(
            pool.clone(),
            vec![
                std::sync::Arc::new(Blocked {
                    pool: pool.clone(),
                    entered: entered.clone(),
                    release: release.clone(),
                }),
                std::sync::Arc::new(Terminal(pool.clone())),
            ],
            crate::application::plugins::build_reactions(pool.clone()).unwrap(),
            Duration::from_secs(60),
            Duration::from_secs(1800),
            Duration::from_secs(10),
            Duration::ZERO,
            Some(1),
        );
        work::enqueue(
            &pool,
            &item(crate::plugins::graph::manifest::TASK, sport, 9_600_101),
        )
        .await
        .unwrap();
        let pulse = Pulse::new();
        let drain = worker.drain_all("busy", &pulse);
        let tick = Notify::new();
        let dispatcher = outbox_loop(&pool, &worker.reactions, &tick, &worker.shutdown);
        tokio::pin!(drain, dispatcher);
        let prove = async {
            entered.notified().await;
            // Arrives after the drain began: startup-only dispatch cannot pass.
            let mut source = item(crate::plugins::analyst::manifest::TASK, sport, 9_600_102);
            source.claim_token = Some("00000000-0000-4000-8000-000000000103".into());
            let mut tx = pool.begin().await.unwrap();
            crate::plugins::analyst::adapter::record_momentum_completed(&mut tx, &source)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            tick.notified().await;
            let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM application_outbox WHERE sport=$1), (SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='sigil' AND status='pending'), (SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage='graph' AND status='running')")
                .bind(sport).fetch_one(&pool).await.unwrap();
            assert_eq!(counts, (0, 1, 1));
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::select! {
                _ = &mut drain => panic!("blocked drain ended early"),
                _ = &mut dispatcher => panic!("dispatcher exited"),
                _ = prove => {}
            }
        })
        .await
        .unwrap();
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), &mut drain)
            .await
            .unwrap();
        let remaining: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1")
                .bind(sport)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, 0);
    }

    struct Terminal(sqlx::PgPool);
    static SIGIL_TEST_MANIFEST: crate::studio::plugin::PluginManifest =
        crate::studio::plugin::PluginManifest {
            id: crate::studio::plugin::PluginId::new("test.sigil"),
            task: crate::plugins::oracle::manifest::TASK,
            claim_policy: crate::application::queue::work::ClaimPolicy::FIFO,
            inference_routes: &[],
            resources: crate::studio::plugin::ResourceProfile::unbounded_batch(1),
            tools: &[],
        };

    #[async_trait::async_trait]
    impl StudioPlugin for Terminal {
        fn manifest(&self) -> &'static crate::studio::plugin::PluginManifest {
            &SIGIL_TEST_MANIFEST
        }
        async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
            let mut tx = self.0.begin().await?;
            assert!(work::lock_claim(&mut tx, item).await?);
            assert!(work::complete_in_transaction(&mut tx, item).await?);
            tx.commit().await?;
            Ok(PluginOutcome::Committed)
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn outbox_poll_recovers_obligation_without_listen_or_models() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&std::env::var("TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        let sport = "ZZ_WORKER_REHEARSAL";
        sqlx::query("INSERT INTO sports(id,display_name,current_season) VALUES ($1,$1,2026) ON CONFLICT DO NOTHING")
            .bind(sport).execute(&pool).await.unwrap();
        for table in ["pipeline_work", "application_outbox"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE sport=$1"))
                .bind(sport)
                .execute(&pool)
                .await
                .unwrap();
        }
        let source = Item {
            stage: crate::plugins::analyst::manifest::TASK,
            entity_type: "team".into(),
            entity_id: 9_600_002,
            sport: sport.into(),
            input_version: Some("v1".into()),
            attempts: 0,
            claim_token: Some("00000000-0000-4000-8000-000000000002".into()),
        };
        let mut tx = pool.begin().await.unwrap();
        crate::plugins::analyst::adapter::record_momentum_completed(&mut tx, &source)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let worker = Worker::new(
            pool.clone(),
            vec![std::sync::Arc::new(Terminal(pool.clone()))],
            crate::application::plugins::build_reactions(pool.clone()).unwrap(),
            Duration::from_secs(1),
            Duration::from_secs(1800),
            Duration::from_secs(5),
            Duration::from_secs(30),
            Some(1),
        );
        let tick = Notify::new();
        let dispatcher = outbox_loop(&pool, &worker.reactions, &tick, &worker.shutdown);
        let consume = async {
            tick.notified().await;
            worker.tick("outbox", &Pulse::new()).await;
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::select! {
                _ = dispatcher => panic!("dispatcher exited"),
                _ = consume => {}
            }
        })
        .await
        .unwrap();
        let remaining:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM application_outbox WHERE sport=$1)+(SELECT count(*) FROM pipeline_work WHERE sport=$1)")
            .bind(sport).fetch_one(&pool).await.unwrap();
        assert_eq!(remaining, 0);
    }

    const UNRELATED_TASK: crate::application::queue::work::TaskKey =
        crate::application::queue::work::TaskKey::new("test_unrelated_worker");
    static UNRELATED_MANIFEST: crate::studio::plugin::PluginManifest =
        crate::studio::plugin::PluginManifest {
            id: crate::studio::plugin::PluginId::new("test.unrelated-worker"),
            task: UNRELATED_TASK,
            claim_policy: crate::application::queue::work::ClaimPolicy::FIFO,
            inference_routes: &[],
            resources: crate::studio::plugin::ResourceProfile::unbounded_batch(1),
            tools: &[],
        };

    struct Unrelated(PgPool);

    #[async_trait::async_trait]
    impl StudioPlugin for Unrelated {
        fn manifest(&self) -> &'static crate::studio::plugin::PluginManifest {
            &UNRELATED_MANIFEST
        }

        async fn execute(&self, item: &Item) -> Result<PluginOutcome> {
            let mut tx = self.0.begin().await?;
            assert!(work::lock_claim(&mut tx, item).await?);
            assert!(work::complete_in_transaction(&mut tx, item).await?);
            tx.commit().await?;
            Ok(PluginOutcome::Committed)
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated migrated TEST_DATABASE_URL; run serially"]
    async fn unrelated_task_registers_claims_executes_and_completes_without_kernel_edits() {
        let sport = "ZZ_UNRELATED_PLUGIN";
        let pool = fixture(sport).await;
        work::enqueue(&pool, &item(UNRELATED_TASK, sport, 9_600_103))
            .await
            .unwrap();
        let worker = Worker::new(
            pool.clone(),
            vec![std::sync::Arc::new(Unrelated(pool.clone()))],
            crate::application::queue::outbox::ReactionRegistry::empty(),
            Duration::from_secs(60),
            Duration::from_secs(1800),
            Duration::from_secs(5),
            Duration::ZERO,
            Some(1),
        );
        worker.drain_all("test", &Pulse::new()).await;
        let remaining: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pipeline_work WHERE sport=$1 AND stage=$2")
                .bind(sport)
                .bind(UNRELATED_TASK.as_str())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, 0);
    }
}

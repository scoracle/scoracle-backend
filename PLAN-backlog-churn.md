# Plan — churn through the pipeline_work backlog

> ## ⏰ STANDING CHECKPOINT: status update to Scott at **21:00, Mon 2026-07-27**
> This survives context loss. If you are picking this file up cold, the checkpoint is the
> commitment — deliver it even if nothing else here has progressed. What it must answer is in
> [The 9PM checkpoint](#the-9pm-checkpoint) at the bottom, and the data to answer it is being
> collected into `logs/queue-depth.csv` on Archbox right now.

Opened 2026-07-26, out of the plumbing session. Scope here is **throughput** — whether the
system can process the work it generates. Voice and prompt tuning are a separate session;
where a lever is really a prompt lever it is named and handed off, not designed here.

---

## Where this came from

The plumbing session opened on "the queue is barely draining while the Mac's GPU sits idle."
Both halves of that turned out to be wrong, and what replaced them is the reason this plan
exists.

**The Mac was never idle.** `~/Library/Logs/ollama.log` (the launchd daemon's real stdout —
NOT `~/.ollama/logs/server.log`, which belongs to the retired app and stops at 12:52) shows
continuous back-to-back generation, 20–45s per request, ~100/hour sustained since 12:56. The
`1.6% CPU` reading that suggested idleness is not a valid signal: with 100% GPU offload the CPU
genuinely idles while Metal works. **Use the ollama access log, never `ps %cpu`, to judge whether
the character host is busy.**

**The queue is not stalled. It is losing a race.** Every one of the ~945 pending items is routed
to the Mac's single-permit 14B. Pending work for the three Archbox/gemma3 stages — `article_read`,
`graph`, `scrub` — is **zero**. Archbox has four parallel slots and nothing to put in them, while
one serialized 14B carries the entire load.

Measured inflow at the time of writing was ~71 items/hour against a completion rate in the same
rough band. That is the whole problem in one line: **inflow ≈ capacity, so the backlog neither
clears nor explodes — it sits.** The exact margin is not yet trustworthy (the measurement window
straddled a service restart), which is why the sampler exists.

---

## Done — 2026-07-26

- **Harness-killing panic fixed and deployed** (`d89f89f`, live on Archbox 22:03:54).
  `strip_element_blocks` searched a `to_lowercase()` copy and indexed the *original* with those
  offsets. `İ` (U+0130, 2 bytes) lowercases to 3 bytes, so a Galatasaray match report with eleven
  of them drifted the offsets eleven bytes and panicked the process. Blast radius was the whole
  harness — all nine stages, every in-flight item orphaned. **This was the source of the eleven
  stuck `running` rows, not the drain starving `requeue_stale` as originally diagnosed.**
  Verified live: boot sweep recovered all 11, the Galatasaray article now parses, zero panics since.
- **GPU duty-cycle stagger installed** — 2h on, 1h off, rest window 00:00–01:00 and every 3h after.
  Pauses the *consumer*, not ollama: stopping ollama would dead-letter every request admitted
  during the window, whereas pausing `scoracle-cognition` idles the GPU just as completely with
  zero failed requests and no model reload (`OLLAMA_KEEP_ALIVE=24h`).
  Units: `scoracle-cognition-{pause,resume}.{service,timer}`.
- **Queue-depth sampler installed** — every 10 min into `logs/queue-depth.csv`, tagged with whether
  the harness was active, so the duty cycle can be divided out rather than silently depressing the
  rate. `scoracle-qsample.timer`.

### Side effect worth knowing

The stagger **incidentally defangs the `requeue_stale` starvation** (friction 1). `requeue_stale`
runs at boot, before `drain_all`, and every resume is a boot. Orphans now wait at most ~3 hours
instead of indefinitely. The underlying starvation is still real and still worth fixing — but it
is no longer the thing blocking entities from being crowned.

---

## The levers, ranked by leverage

**1. Bucket trending summaries (Scott's idea — highest leverage).**
`sigil` is the largest queue at ~390. Every other lever makes each item faster; this one makes
there be *fewer items*. Grouping trending summaries into one generation covering N entities,
instead of N separate generations, is a multiplicative reduction rather than a linear speedup —
and if it runs on gemma3 it spends capacity that is currently sitting at zero utilisation. It
attacks the queue depth and the host imbalance at the same time. Needs a design pass.

**2. Give Archbox something to do.**
Four idle gemma3 slots against a saturated single-permit 14B is the structural imbalance behind
everything here. Which character work could move to gemma3 is a voice question, not a plumbing
one — but the capacity case is strong enough that it should be asked deliberately rather than
left to default. Lever 1 is one concrete form of this.

**3. Prompt tuning (separate session, as agreed).**
Generations run 20–45s. Cutting the median materially would roughly scale capacity with it. This
is the cheapest large win that needs no architectural change. Hand off to the prompt session with
the note that **throughput is now a first-class reason to shorten prompts**, not just cost.

**4. Fix the handler timeout so it does not measure queueing (friction 2).**
`COGNITION_HANDLER_TIMEOUT_SECONDS=1200` is wall-clock from handler start, but an item's clock
starts when its handler starts, not when it reaches the GPU. `transfers` is the stage that
dead-letters because `insider/mod.rs:2184` calls `vet_pair` — a model call — **once per candidate
pair in a loop**; every other Mac junction has exactly one `extract` site. So one transfers item is
N sequential GPU calls, each queueing behind five other stages at one permit. Dead letters landed
at exactly 1200s after start, and exactly 1200s after that.
This is not just lost work, it is **lost capacity**: an item that burns 20 minutes and then dead-letters
has consumed real generations that are then discarded. Fixing it recovers throughput, not just rows.

**5. Fix `requeue_stale` starvation properly (friction 1).**
Recommended: run it on its own interval rather than once per tick. Safety argument checked —
`requeue_stale` is a single unconditional `UPDATE`, and `handler_timeout` (1200s) < `stale_lease`
(1800s) means a live handler always fails ~10 minutes before its row becomes eligible, so a
concurrent sweep cannot steal in-flight work. **Add a startup guard refusing to boot if
`handler_timeout >= stale_lease`** — nothing currently enforces the ordering that makes this safe,
and both are env-tunable. Priority dropped by the stagger side effect above.

**6. Prioritise by demand.**
945 items drained uniformly serves no one first. If only a fraction are user-visible, ordering by
what the frontend actually requests beats draining in DAG order. Only worth building if the gauge
says we are structurally short of capacity.

---

## The open question the gauge answers

Scott's framing: *once the culprits are cleared, we get a real read on whether we can support the
work we have, or need to scale back.* The culprits are cleared as of 22:03 tonight. So the read
starts now.

**The honest caveat, stated up front:** the stagger cuts wall-clock capacity by a third while
leaving inflow untouched. If capacity and inflow were near parity before, a 2:1 duty cycle makes
the measured backlog **grow** — and the gauge will read "we cannot support this load" partly
because we throttled it. When reading the result, divide by the duty cycle (`harness_active` is in
the CSV for exactly this) before concluding anything structural. The thermal rest is worth having;
it just must not be mistaken for a capacity verdict.

---

## The 9PM checkpoint

Due **21:00, Mon 2026-07-27**. What it must answer:

1. **Net drain rate**, duty-cycle-adjusted, from `logs/queue-depth.csv` on Archbox — is the backlog
   shrinking, flat, or growing? Per stage, since `sigil` and `momentum` dominate.
2. **Inflow vs capacity** as separate numbers, not just the net. Capacity comes from counting
   `/api/generate` lines per hour in `~/Library/Logs/ollama.log` on the Mac during
   `harness_active=1` windows; inflow from the depth deltas plus completions.
3. **Did the panic stay fixed** — `NRestarts` on `scoracle-cognition`, and any new `panicked` lines.
4. **Did the stagger behave** — eight clean pause/resume cycles, no dead-letter spike at window
   edges, and confirmation that each resume's boot sweep recovered any orphans.
5. **Dead-letter trend**, especially `transfers` — is friction 2 still bleeding capacity, and how
   much (generations spent on items that then dead-lettered).
6. **The verdict**, with the duty-cycle caveat applied: can we support current inflow, and if not,
   by what factor are we short? That factor is what decides between "tune prompts" and "bucket" and
   "scale back collection".

Commands are in `HANDOFF-plumbing.md` under *Useful commands*; the two new sources are
`logs/queue-depth.csv` (Archbox) and `~/Library/Logs/ollama.log` (Mac).

---

## Not doing yet, and why

- **Not cutting collection.** Scott's call: keep the frontend collecting normally for a few days so
  the gauge measures real demand. Ingest is cron-driven and fully independent of the harness, so
  the stagger does not touch it.
- **Not rebalancing routes.** Moving character stages to gemma3 changes voice, and voice is out of
  scope here. Raised as lever 2 for a deliberate decision.
- **Not re-arming the two `article_read` dead letters** (`173300`, `176396`, both at `attempts=5`,
  `parse article evidence`). Pre-existing, unrelated to the panic, and unrelated to throughput.

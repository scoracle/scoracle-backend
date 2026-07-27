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

- **Ingestion scaled back to one daily sweep** (`8eb6e02`, binary deployed 22:26). The corpus RSS
  sweep moves from every twelve hours to `0 2 * * *`, and the lookback window moves with it from
  12h to 24h. **The two are coupled and must stay that way** — `timeWindows` is both the `when:`
  token sent to Google News and the cutoff in `filterArticlesByLookback`, so a window narrower than
  the cron period drops the news in between twice over, silently and unrecoverably. A rewritten
  test now fails if anyone narrows it below the cron period.
  Per-entity volume stays bounded by `-rss-limit` (default 12, never overridden in the live
  crontab), which is the property that makes the cadence safe to raise again later.

### Correction to the load story

The "no limit" half of the architectural shift was not real: `-rss-limit` has defaulted to 12 the
whole time and the crontab never overrode it. What actually changed was the **reader** (top-4 per
entity, `COGNITION_ARTICLE_READ_TOP_K`, also never overridden) and the **cadence** (6h → 12h). So
the cap Scott remembered is intact; the cadence and the reader are what grew, and the cadence is
now reverted.

Worth knowing for the gauge: the daily sweep only throttles the *news-driven* chain
(scrub → article_read → narratives → vibe → sigil). `momentum` inflow — ~24/hour, the second-largest
queue — comes from the fixture-processing crons (`nba-process` every 30 min, `nfl-process` twice
hourly), which were deliberately left alone. If tomorrow's gauge still reads short, those crons are
the next dial, not the news sweep.

### Offset the two cards' rest windows

Today the stagger pauses the whole `scoracle-cognition` process, which owns every stage, so **both
cards rest at the same time** — verified in the sampler, where `article_read` pending is exactly
flat through every `active=0` hour (5852→5852, 4856→4856). The Mac and the 1070 do different jobs
and should not go dark together.

**What this does and does not buy.** It does **not** recover the lost third — each card still rests
one hour in three either way, so per-card throughput is unchanged. (An earlier note in this session
said offsetting would "recover most of that third"; that was wrong.) What it buys is **pipeline
continuity**: the Reader keeps producing evidence cards through the Mac's rest hour, so the DAG
keeps flowing instead of the entire system halting. That is a latency and smoothness win, and it
is worth **least** while both queues are deep — a backlogged stage always has work waiting, so
accumulation gains nothing. **Do this after the backlogs clear, not before.**

**Options, cheapest first:**

1. **Split into two systemd units** (recommended). `COGNITION_STAGES` already partitions stages by
   name, and the partition wanted here already exists: `scrub`/`graph`/`article_read` on Archbox,
   the six voices on the Mac. Two units, each with its own pause/resume timers, trivially offset.
   No code change at all.
   - **The wrinkle that will bite:** `COGNITION_STAGES` currently lives in `.env.local`, which
     loads *after* the unit's `Environment=` line and therefore **overrides it**. A per-unit
     `EnvironmentFile=` loaded after `.env.local` is the fix; setting it in the unit body silently
     does nothing.
   - Costs: two pools, two LISTEN sockets, two embedders. The embedder is the real one — BGE loads
     per process — and Phase 3 deletes it anyway, which is another reason to sequence this later.
2. **Rest windows in the host governor** (`route.rs::governor_for`). Precise, one process. But an
   item already claimed would block on the semaphore for the whole rest hour and burn its 1200s
   handler timeout into a dead letter — friction 2 at full force. Needs the drain to stop *admitting*
   for a resting host, not just stop serving it, which means option 3 anyway.
3. **Teach the drain which host each stage routes to.** The honest design: one `backend_host()` on
   `StageHandler`, with both the slot groups added today and the rest windows keyed off it. Largest
   change, and the one that makes this a first-class concept rather than two mechanisms that happen
   to agree.

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
6. **The read budget raised 4 → 10** (set 2026-07-26 23:28). Did the extra reading land on the
   gemma3 card as intended, and did it push `narratives` inflow up? `narratives`' `input_version`
   hashes every article's read status, so each new reading can reopen that entity's narratives row
   — and narratives is **Mac**-routed, the saturated side. If narratives inflow tracks the budget
   increase, drop toward 8. If most reads land in the post-sweep burst before narratives drains the
   entity, the hash settles once and there is no amplification. Measure it; do not assume either way.
7. **Did Phase 2 fix the zero-admit teams?** Fifteen clubs (Nice, Spezia, Leganés, Huesca, Amiens …)
   were admitted nothing by the regex tier that is now gone. Check them by name in the 02:00 funnel.
8. **Candidate model gates** — read `logs/model-eval/*.log` on the **Mac**. A momentum gate against
   `mistral-nemo:12b` was queued for the 18:00 rest window; compare to the **36/37** baseline.
   See "Making the Mac enough" below for what the result decides.
9. **The verdict**, with the duty-cycle caveat applied: can we support current inflow, and if not,
   by what factor are we short? That factor is what decides between "tune prompts" and "bucket" and
   "scale back collection".

---

## Making the Mac enough — the three levers, measured

The M4 is **memory-bandwidth-bound and saturated**: measured 12.4 tok/s decode on a 9.5 GB model is
118 GB/s against the chip's 120 GB/s ceiling. So there is no tuning headroom on the Mac — only
fewer bytes. Prefill (118 tok/s, compute-bound) scales with parameter count, so both halves of a
generation improve roughly in proportion to how much smaller the model is.

**The prize is not the tok/s — it is the permit.** `max_concurrent=1` exists because 16 GB holds one
KV allocation, and that single permit is what serializes the six voices. It is the direct cause of
friction 2: `transfers` makes N sequential calls per item and dead-letters at 1200s while *queueing*.
Free model bytes → free KV bytes → a second permit → the serialization goes away.

### The KV arithmetic — the permit is a context-length decision, not a hardware limit

From the model's own architecture (`/api/show`): 40 layers, 8 KV heads, key/value length 128, and
`OLLAMA_KV_CACHE_TYPE=q8_0` in the launchd plist:

```
40 layers × 8 KV heads × (128 + 128) × 1 byte  =  80 KB per token
```

| context | KV per sequence | model + 2 sequences + buffers |
|---|---|---|
| 16384 (current) | 1.31 GB | 9.1 + 2.62 + ~0.7 = **12.4 GB** — marginal on 16 GB |
| **8192** | **0.66 GB** | 9.1 + 1.31 + ~0.7 = **11.1 GB** — what ONE sequence costs today |
| 8192 + `mistral-nemo:12b` | 0.66 GB | 7.1 + 6×0.66 + ~0.7 = **11.7 GB** — **six** sequences |

**Two sequences at 8192 cost exactly what one at 16384 costs.** The second permit is arithmetically
free, not merely likely. Nemo has the identical KV geometry (40 × 8 × 128), so its 2 GB saving buys
sequences rather than speed alone — six of them, one per character voice.

`max_concurrent=1` was almost certainly correct when the KV cache was f16: 2.62 GB per sequence, and
two genuinely would not fit. `q8_0` halved that and the permit was never revisited. So the
constraint shaping this whole session — six voices serialised, `transfers` dead-lettering at 1200s
on *wait* rather than work — is a setting, not a wall.

**Cheapest possible test:** set `OLLAMA_NUM_PARALLEL=2` in the plist, `launchctl kickstart`, and see
whether it loads and holds. Fully reversible, and a rest window is the free hour to do it in.

| lever | speed | concurrency | quality risk |
|---|---|---|---|
| **`VOICE_NUM_CTX` 16384 → 8192** | none | halves KV/sequence — may buy a permit on its own | **none**, if prompts fit. Journalist at corpus 40 ≈ 2,750 tok + system + 900 predict ≈ 4,500. Fails loudly as truncation, not subtly |
| **`mistral-nemo:12b`** (7.1 GB, pulled) | **1.20× measured** | 2 GB freed → up to 6 permits at 8192 | **gate-clean: 37/37 vs 36/37** |
| `mistral:latest` (4.4 GB) | ~2× | 4.7 GB freed → plausibly 3–4 permits | **high** — this is Mistral **7B v0.3**, two generations back. Instruction-following is exactly what the voice contracts lean on |

Order to try them: **ctx first** (free), then nemo, and treat the 7B as a last resort rather than a
midpoint. A 1.2× model change becomes a ~2.5× system change if it buys the second permit, so the
concurrency question matters more than the tok/s question.

### Measured 2026-07-27, 18:00 rest window

- **Gate: `mistral-nemo:12b` scored 37/37** on the momentum fixtures against ministral's **36/37**
  baseline — it passes the `steady band` check ministral leaks. 8 fixtures, 37 property checks
  (the "37" is checks, not fixtures); verified against the ollama access log as 8 real calls, not a
  cached run. Log: `logs/model-eval/momentum-mistral-nemo_12b-20260727-1803.log`.
  **Read this as "the contract holds", not "the voice is right".** Property checks are mechanical —
  word counts, required/banned phrases. A model can pass all 37 and still sound wrong. The voice
  judgment is Scott's and belongs to the prompt session.
- **Speed: 14.9 tok/s decode vs ministral's 12.4 — 1.20×**, a little under the 1.28× the size ratio
  predicts.
- **A "551 tokens of overhead per call" finding was raised here and is WITHDRAWN — it was a
  benchmark artifact and production never pays it.** Recorded rather than deleted, because the
  measurement is a trap worth knowing about.
  The same 13-token prompt reported `prompt_eval_count` 564 on ministral and 13 on nemo. The cause
  is not the vision tower: ministral's chat template injects a long default Mistral identity block
  **only when no system prompt is supplied** (`{{- if not $hasSystemPrompt }}[SYSTEM_PROMPT]You are
  Ministral-3-14B-Instruct-2512...`). The benchmark sent none. Every junction sets
  `GenerateOptions.system`, so the block is never injected in production — measured directly: the
  same prompt **with** a system prompt reports `prompt_eval_count` **36**, not 564.
  **Lesson for any future model benchmark here: always send a system prompt, or you are measuring
  the template's fallback rather than the model.**
  What remains true is that `ministral-3:14b` advertises a `vision` capability we never use
  (`mistral3.vision.block_count=24`). That costs loaded weights, not prompt tokens — worth a look
  when sizing VRAM, not a per-call saving.

So nemo's measured advantage is the **1.20× decode and the 2 GB it frees for permits** — not more.

Run gates with `scripts/hosting/model-gate.sh <task> <model>` — it waits for nothing and assumes you
called it inside a rest window, which is the hour every three when the Mac is idle by design and a
gate neither competes with the drain nor is slowed by it.

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

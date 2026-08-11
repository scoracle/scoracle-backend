# Handoff — trace the two-machine plumbing, find the friction

**Scope: PLUMBING ONLY.** Voice, character and prompt tuning are a separate session and are
explicitly out of scope here. If a fixture goes red on wording, note it and move on. What this
session cares about is whether work *flows* correctly across two machines: claimed, routed,
generated, persisted, completed, and handed to the next stage without stalling, starving or
silently dropping.

Both machines are reachable from the Mac now, so for the first time the whole path can be watched
end to end from one place.

---

## The two machines

| | **Mac mini** (`192.168.1.77`) | **Archbox** (`192.168.1.92`) |
|---|---|---|
| role | character host | **production** |
| model | `ministral-3:14b`, 100% GPU, 16384 ctx | `gemma3:4b` |
| runs | ollama only | Postgres, `scoracle-cognition`, `scoracle-api`, ollama |
| serves | the six character voices, over the LAN | The Reader, graph, SQL, multilang |
| concurrency | `max_concurrent=1` (16 GB holds one KV alloc) | `max_concurrent=4` |
| repo | `/Users/scotty/scoracle/scoracle-backend` | `/home/sheneveld/scoracle/scoracle-backend` |

Routing as the service reports it at startup:

```
article-reader, emotional-news, multilang, sql   -> gemma3:4b      @ localhost      (Archbox)
momentum, narrative, oracle, stats, transfer, vibe -> ministral-3:14b @ 192.168.1.77  (Mac)
```

**Access from the Mac (new, set up 2026-07-26):**

```sh
ssh archbox 'bash -lc "<cmd>"'        # login shell — non-interactive PATH lacks cargo/psql/hostname
psql -U scoracle -d scoracle -h localhost -c "..."   # on Archbox; verified working
```

`~/.ssh/id_archbox_claude` is a dedicated key on its own `authorized_keys` line — revoke by deleting
that one line. `Host archbox` alias lives in the Mac's `~/.ssh/config`.

---

## Verified working — do not re-litigate

- Deploy is live and correct: Archbox runs `commit="5b471f752441"`, built `2026-07-27T00:55:55Z`.
- Both units active: `scoracle-cognition`, `scoracle-api`.
- The Oracle completion barrier is compiled in (the *post-complete* worker version, not the earlier
  racy handler version).
- **The conviction ladder is calibrated and needs no change.** Measured over 7,272 entities:
  conviction 0 ≈ 20%, ±1 ≈ 37%, ±2 ≈ 18%, ±3 ≈ 15%, ±4 ≈ 9%, ±5 ≈ 2%. The feared failure mode
  (mass under 20 so the 3/4/5 bands never fire) did **not** materialise. Item closed.
- Mac ↔ Archbox LAN path works; ollama on the Mac is reachable and the model stays resident 24h.

---

## Friction point 1 — stale-lease recovery is starved by a long drain (**start here**)

**Symptom.** 11 `pipeline_work` rows have been `status='running'` for 43+ minutes, even though
`COGNITION_STALE_LEASE_SECONDS` defaults to **1800s (30 min)**. Meanwhile 6 other rows were claimed
normally in the last 5 minutes, so the worker is alive.

**Mechanism (read `worker.rs::tick`).** One cycle is:

```rust
requeue_stale(...).await     // once
drain_all(...).await         // "drain every registered stage to EMPTY"
```

`requeue_stale` runs once per tick, and the tick does not come round again until `drain_all`
returns — which it only does when the queue is empty. With ~550 pending items funnelling through
one serialized 12 tok/s GPU, that drain runs for **hours**. So the 30-minute lease never gets
evaluated, and any row orphaned during the cycle stays `running` for the whole drain.

Backlogs are not exotic: any prompt-version bump that sits inside `input_hash` creates one. This
one came from `momentum-s13`.

**Why it matters more than it looks — it interacts with the Oracle barrier.** The barrier treats
`status='failed'` as settled but `status='running'` as outstanding. A stale lease is therefore an
*indefinite* block on that entity's reading, not a delayed one. Those 11 entities cannot be crowned
until the entire backlog drains.

**Candidate fixes to weigh (do not just pick the first):**
- Run `requeue_stale` on its own interval/task rather than once per tick.
- Bound `drain_all` (drain a slice, return, re-tick) so recovery interleaves.
- Have the barrier treat a lease older than `stale_lease` as settled — narrower, but it papers over
  the starvation rather than fixing it.

---

## Friction point 2 — handler timeouts are accumulating

`COGNITION_HANDLER_TIMEOUT_SECONDS = 1200` (20 min). Already dead-lettering:

```
transfers 3468  attempts=1  handler exceeded COGNITION_HANDLER_TIMEOUT_SECONDS (1200s)
transfers 83    attempts=1  handler exceeded COGNITION_HANDLER_TIMEOUT_SECONDS (1200s)
sigil 9         attempts=1  model generate: ollama request: error sending request ...
peak 1881       attempts=1  model generate: ollama request: error sending request ...
```

**Suspected mechanism.** The concurrent drain keeps many items in flight, but all six character
stages share the Mac's **single** permit. An item's 20-minute clock starts when its handler starts,
not when it reaches the GPU — so under backlog an item can burn the whole timeout *queueing*. The
concurrent drain (`4ae26b9`) and the 1-permit remote host are individually correct and jointly
produce this.

Worth checking whether the timeout should measure generation rather than wall-clock-including-wait,
or whether Mac-routed stages need a smaller in-flight cap so items are not admitted before there is
any chance of a permit.

---

## Friction point 3 — the queue is barely moving, and the Mac looks idle

Two snapshots ~10 minutes apart were essentially unchanged (momentum 277→280 total, narratives 178,
sigil 393). At the same time the Mac's `llama-server` sampled **0.1–2.4% CPU** — idle — while 550+
items are routed to it.

That combination is the core mystery for this session: **work is queued for the Mac, the Mac is
idle, and the queue is not draining.** Friction 1 and 2 are probably contributors, but confirm
rather than assume. Specifically worth separating:

- Are the 6 recently-claimed items Archbox-local (gemma3) rather than Mac-routed?
- Is `drain_concurrency` admitting Mac-routed items that then block on the semaphore?
- Is anything actually in flight to `192.168.1.77:11434` right now?

The Mac's *current* ollama logs are NOT `~/.ollama/logs/server.log` — that file belongs to the
retired app and stops at 12:52. ollama runs as a launchd daemon (`/usr/local/bin/ollama serve`,
pid 1 parent) whose stdout goes elsewhere; finding where is itself a small plumbing task worth
doing, because without it there is no request-level view of the character host.

---

## Friction point 4 — the barrier is still unverified in production

It is compiled in and it is the correct post-complete design, but nothing has *observed* it fire.

- Its messages are `debug!`; the service runs at `INFO`, so zero barrier lines is **absence of
  evidence, not evidence of absence**.
- `pipeline_work` has **no `created_at`**, and `updated_at` moves on claim as well as enqueue, so
  sigil-row timestamps cannot separate "the barrier enqueued this" from "the worker claimed this".

Cheapest decisive test: raise the cognition service to `debug` for one sweep, grep for
`oracle barrier`, then revert. Both log lines exist and say which way it went:

```
oracle barrier: pillars still outstanding; not enqueuing
oracle barrier: last pillar settled; enqueued sigil
```

The barrier's SQL has never been executed against Postgres by anyone — it was written and shipped
from the Mac, which has no database.

---

## Friction point 5 — pre-existing, lower priority

`article_read` has two rows dead-lettered at `attempts=5` (`parse article evidence (raw=...)`).
Unrelated to this branch and unrelated to the two-machine split, but they are permanently stuck and
should either be re-armed or have the parse fixed.

---

## Traps that cost time today

- **`!` prefix.** It means "run in this session" only at the Claude Code prompt on the Mac. Typed
  into a shell it is logical-NOT: it inverts exit codes, and `! ssh archbox` typed *on Archbox*
  loops back to Archbox's own `127.0.1.1`.
- **`git fetch` before trusting `main`.** Local `main` was 5 commits behind origin, then 12 more
  landed mid-session. The handoff recipe's `merge --ff-only` assumes they are in sync; twice today
  they were not. A careless resolution would have reverted a production parser hotfix.
- **The deploy path is `rust/bin/`, and `cargo build` runs from `rust/`.** Corrected in
  `HANDOFF-junctions.md` this session; the old snippet `cp ... bin/scoracle-cognition` from the repo
  root targets a directory that holds only `scoracle-api`.
- **SSH non-interactive PATH is minimal** — no `cargo`, `psql`, or even `hostname`. Wrap in
  `bash -lc`.
- **Gate before deploying a parser change.** This is the standing lesson from `190a83a`, whose own
  commit message says it: a model swap on a junction whose parser is not JSON needs its fixture gate
  run first.

---

## Useful commands

```sh
# queue shape
ssh archbox "psql -U scoracle -d scoracle -h localhost -c \"SELECT stage,status,count(*) FROM pipeline_work GROUP BY 1,2 ORDER BY 1,2;\""

# stale leases — the friction-1 signal
ssh archbox "psql -U scoracle -d scoracle -h localhost -c \"SELECT stage,entity_id,now()-updated_at AS held_for FROM pipeline_work WHERE status='running' ORDER BY updated_at;\""

# dead letters and why
ssh archbox "psql -U scoracle -d scoracle -h localhost -c \"SELECT stage,entity_id,attempts,left(last_error,80) FROM pipeline_work WHERE status='failed' ORDER BY attempts DESC;\""

# service log + deployed commit
ssh archbox "journalctl --user -u scoracle-cognition -n 40 --no-pager"

# is the Mac actually generating?
ps -o %cpu= -p $(pgrep -f llama-server | head -1); ollama ps
lsof -nP -iTCP:11434 | grep 192.168.1.92
```

Fixture gates (Mac, needs the GPU — check Archbox is not mid-generation first):

```sh
cd /Users/scotty/scoracle/scoracle-backend/rust
export DATABASE_URL="postgres://unused/unused" OLLAMA_BASE_URL="http://127.0.0.1:11434" OLLAMA_TIMEOUT_SECONDS=600
COGNITION_ROUTE_MOMENTUM_LOGIC=ministral-3:14b ./target/debug/eval --task momentum --fixtures
COGNITION_ROUTE_ORACLE_LOGIC=ministral-3:14b   ./target/debug/eval --task oracle   --fixtures
```

`OLLAMA_TIMEOUT_SECONDS` is **not** optional — the default is 60s (`config.rs`) and two fixtures
time out without it.

---

## Current gate baselines (for reference only — prompt work is out of scope)

- momentum `s13`: **36/37**. The one red is a genuine `steady band` leak.
- oracle `or8`: **94/98**. All four reds are `reading_max_peers` — the model names 2–4 peers where
  the contract allows one. Promotion to a prominent rule block has fixed five rules out of six
  across both seats; this is the one that does not respond to instruction, so it is a decision about
  the Oracle's voice rather than a bug. Left visibly red on purpose.
- `cargo test --lib`: 269 passed.

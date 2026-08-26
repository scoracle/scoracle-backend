# The serving system — and how a new machine joins the drain

**What this is:** how work gets done in Scoracle, and the exact steps to add a machine that
pulls from the same pool. Written 2026-08-24 against `main` @ `304b2d5`.

Companions in this folder: [`RUNBOOK.md`](RUNBOOK.md) (release/rollback, backup/restore),
[`ENDPOINTS.md`](ENDPOINTS.md) (the product API), [`DEVELOPMENT.md`](DEVELOPMENT.md).

---

## 1. The shape of it

There is **one queue table** (`pipeline_work`) and **N workers**. A worker is one
`scoracle-cognition` process. Every worker runs the same code, reads the same queue, and
does whatever it manages to claim.

```
                    ┌─────────────────────────┐
   Go: pipeline ───►│  pipeline_work (queue)  │◄─── SQL triggers (enqueue_*)
   (RSS ingest)     │   Postgres, on archbox  │◄─── Rust enqueuers
                    └───────────┬─────────────┘
                                │  FOR UPDATE SKIP LOCKED
                 ┌──────────────┼──────────────┐
                 ▼              ▼              ▼
          scoracle-cognition  ...worker 2   ...worker N
           (archbox + GPU)    (Mac + GPU)    (any host + GPU)
```

**Nothing coordinates the workers.** There is no leader, no shard assignment, no partitioning.
Postgres does the whole job through row locks, which is why adding a machine is a deploy
rather than a design change.

## 2. The claim protocol

`work::claim` is the entire contract:

```sql
SELECT ... FROM pipeline_work
 WHERE stage = $1
   AND status IN ('pending', 'failed')
   AND available_at <= NOW()
 ORDER BY <per-stage priority>, available_at
   FOR UPDATE SKIP LOCKED
 LIMIT $2                      -- CLAIM_BATCH = 10
```
then flips the claimed rows to `status = 'running'`.

Four consequences worth understanding before you add a machine:

- **`SKIP LOCKED` makes double-processing impossible.** A row locked by one worker is stepped
  over by every other. This is the whole safety mechanism.
- **Ordering is NEAR-order, not strict FIFO.** Because workers step over locked rows, the
  global order is approximate. That is fine here — mig 225's "FIFO preservation" is about not
  restamping `available_at` when a pending row is re-enqueued, not about serialising the drain
  — but do not describe the system as strictly ordered.
- **`failed` rows are re-claimed**, not dead. They simply wait for `available_at`. A retry with
  an earlier stamp is claimed *before* newer pending work, which is why a queue can look
  "stuck" at a constant pending count while the failed count drains first. That is correct
  behaviour, not a stall.
- **Priority is per stage.** Most stages order by `available_at` alone; the news stages order
  by `feed_rank` first, and some entity stages put `team` before `player`.

### Leases and crashed workers

A worker that dies mid-item leaves rows in `running`. The **stale-lease recovery loop** (its
own task, independent of the drain) returns them:

| knob | value |
|---|---|
| loop interval | 60s |
| lease | 1800s (30 min) |

So the cost of losing a machine is bounded: its in-flight items (≤ `CLAIM_BATCH` per stage)
come back within half an hour and any other worker picks them up. **You do not need to do
anything when a worker dies.** Do not "clean up" `running` rows by hand.

### Wakeups

Workers `LISTEN` on the Postgres channel `pipeline_work_ready`, so an enqueue wakes an idle
worker immediately rather than waiting for a poll.

## 3. Failure semantics — read this before adding a machine

`work::fail` increments `attempts` **regardless of cause** and backs the row off:

| prior failures | next attempt in |
|---|---|
| 0 | 30s |
| 1 | 2m |
| 2 | 10m |
| 3+ | 30m |

At `MAX_ATTEMPTS = 5` the row is parked at `NOW() + INTERVAL '100 years'` — a **dead letter**,
invisible unless you query for it:

```sql
SELECT stage, entity_type, entity_id, attempts, last_error
  FROM pipeline_work WHERE available_at > now() + interval '50 years';
```

**The sharp edge:** an unreachable backend fails in milliseconds, so a dead dependency burns
items as fast as the ramp allows. Roughly **43 minutes of downtime on something a stage depends
on will permanently dead-letter that stage's whole queue.** This was measured on 2026-08-24:
202 narratives items failed in one burst when archbox briefly could not reach the Mac's ollama;
they survived only because the outage was short.

This is the single strongest argument for the topology in §5.

## 4. Stages, roles, and the GPU governor

**Stages** are set per host by `COGNITION_STAGES` in `.env.local`. Current production list:

```
graph, editor, investigate_entity, narratives, vibe, rating, transfers, momentum, sigil
```

`fixture_boxscore` is deliberately ABSENT — no source is registered, so enabling it drains
fixtures to `no_source`. Do not add it without reading
`planning_docs/PLAN-availability-and-boxscores.md`.

**Roles → models.** Stage code never names a model. It asks for a `Role`, and
`COGNITION_ROUTE_<ROLE>` maps that role to a model. `COGNITION_ROUTE_<ROLE>_BASE_URL`
optionally sends that role to a *different host* — this is the two-host split, and §5 explains
why a new machine should not use it.

**The governor** is per host, keyed by `base_url`, sized from
`COGNITION_BACKEND_CONCURRENCY`. One semaphore per distinct machine, so two hosts drain
concurrently instead of taking turns.

**Slot groups** are a second, per-process limit: `ARCHBOX_SLOTS = ("archbox-3b", 4)` and
`MAC_SLOTS = ("mac-3b", 4)`. A stage declaring a slot group shares that budget with every
other stage in it, which is how the Editor's drain is stopped from starving the voices.

> ⚠️ **The slot-group names are historical.** They name hosts, but the semaphore is
> **per process**. A second worker gets its own `archbox-3b` semaphore that has nothing to do
> with archbox. The mechanism still does the right thing (bound concurrency per process); only
> the name misleads. Do not try to make slot groups cluster-wide — that would need a
> distributed semaphore, and the queue already handles distribution.

## 5. Two topologies — pick the second

**A. One worker, remote model calls** (what production ran until 2026-08-24)

```
archbox: worker ──HTTP──► Mac ollama     (narratives, vibe)
         worker ──local─► own ollama     (everything else)
```

Every cross-machine call is a per-item network dependency. If the Mac blinks, the *item* is
blamed, `attempts` is spent, and §3's 43-minute window starts running. Nothing on the Mac is
doing work; it is only a GPU on the end of a wire.

**B. N workers, local model calls** (recommended)

```
archbox: worker ──local─► own ollama  ─┐
Mac:     worker ──local─► own ollama  ─┼──► same pipeline_work
host N:  worker ──local─► own ollama  ─┘
```

Each worker only ever talks to its own GPU. A machine going away costs its leases (auto
recovered in ≤30 min) and nothing else. No machine idles waiting on another. Adding capacity
is config.

Topology B also **removes the need** for a per-backend circuit breaker or cross-host failover:
there is no cross-machine model call left to fail.

The one thing B does not fix: a worker's **local** ollama can still die and still burn item
attempts. That is worth addressing on its own (do not spend an item's lives on infrastructure
failure), independent of topology.

---

## 6. Adding a machine — the checklist

### Prerequisites

1. **The machine has a GPU and its own ollama**, serving the same model tag the roles name
   (currently `granite4.2:3b` since 2026-08-25 — see
   `run_docs/2026-08-25_resident-model-switch-granite.md`; check
   `COGNITION_ROUTE_*` on an existing host, and note every role MUST carry an
   explicit `COGNITION_ROUTE_<ROLE>_THINK` on granite).
   Verify: `curl -s localhost:11434/api/tags`.
2. **Postgres accepts connections from it.** On the DB host (`archbox`):

   ```bash
   # postgresql.conf — listen beyond loopback.
   # Use '*' rather than pinning an IP: both machines are on DHCP, and a pinned
   # address that changes makes Postgres FAIL TO START.
   listen_addresses = '*'

   # pg_hba.conf — one line per worker, scoped to /32. This is the real access control.
   host    scoracle    scoracle    <worker-ip>/32    scram-sha-256
   ```

   `listen_addresses` needs a **restart**; `pg_hba.conf` alone needs only a reload.
   Config lives at `/mnt/data/postgres/data/` on archbox — **not** the distro default.

3. **Open the firewall.** archbox runs **nftables with `policy drop`** on input — only 22,
   loopback, ICMP and established traffic are allowed. Opening `listen_addresses` and
   `pg_hba.conf` is not enough; the packet never reaches Postgres.

   **Edit the FILE, then reload. Do not add the rule at runtime.** `/etc/nftables.conf` opens
   with `destroy table inet filter`, so `nft -f` tears the table down and rebuilds it from the
   file — silently discarding anything added with `nft add rule`. Editing the file first makes
   the reload apply the rule instead of erasing it, and it is persistent in the same step.

   ```bash
   sudo cp /etc/nftables.conf /etc/nftables.conf.bak
   sudo sed -i '/tcp dport ssh accept/a\    ip saddr <worker-ip> tcp dport 5432 accept comment "postgres from worker"' /etc/nftables.conf
   grep -n -B1 -A1 5432 /etc/nftables.conf   # confirm it sits just after the sshd line
   sudo nft -f /etc/nftables.conf
   ```

   Placement matters: the rule must precede the chain's `reject`, which is why it is anchored to
   the sshd line rather than appended.

   > Three traps, all hit on 2026-08-24.
   >
   > **`systemctl is-active nftables` reports `inactive` even when rules are loaded** — it is a
   > oneshot that loads at boot and exits. Never conclude "no firewall" from the service state;
   > read `sudo nft list ruleset`.
   >
   > **`nft add rule` APPENDS**, landing the rule after the `reject` where it can never match.
   >
   > **`nft -f` on the stock config DESTROYS runtime additions.** Adding a rule live and then
   > "verifying the file parses" undoes the rule you just added — which looks exactly like the
   > rule never worked.
   >
   > Archbox has no `vi`, so `sudoedit` fails; `nano` is the only editor present. The `sed`
   > above avoids needing one.

   The chain's `reject with icmpx admin-prohibited` means a blocked port surfaces as
   **`ConnectionRefused`, not a timeout** — so "connection refused" here does not mean
   "nothing is listening".

4. **Confirm the source IP.** `pg_hba` and the firewall rule both match on source address, and
   a machine with several interfaces will use whichever one routes to the DB host:

   ```bash
   route -n get <db-host-ip>              # macOS — read "interface:"
   ssh <db-host> 'echo $SSH_CONNECTION'   # prints "<client-ip> <port> <server-ip> <port>"
   ```

### Install

5. Clone the repo and build:
   ```bash
   scripts/hosting/release.sh --build-only    # verifies the build without touching services
   ```
6. Write `.env.local` on the new worker. **The critical differences from a single-host setup:**

   ```bash
   DATABASE_URL=postgresql://scoracle:<pw>@<db-host>:5432/scoracle?sslmode=require

   # Point at the LOCAL GPU. This is the whole point of topology B.
   OLLAMA_BASE_URL=http://localhost:11434

   # Same stage list as the other workers — every worker is interchangeable.
   COGNITION_STAGES=graph,editor,investigate_entity,narratives,vibe,rating,transfers,momentum,sigil

   # Size this host's governor for ITS card.
   COGNITION_BACKEND_CONCURRENCY="http://localhost:11434=4"
   ```

   **Do NOT set any `COGNITION_ROUTE_<ROLE>_BASE_URL`.** That is what creates the remote model
   call topology B exists to remove.

7. Install a service so it survives reboot. On Linux, a systemd **user** unit with lingering
   (`loginctl enable-linger <user>`) — without lingering it will not start unattended, which is
   easy to miss because the unit looks fine when you are logged in. On macOS, a launchd plist.

8. Once the new worker is drawing, **remove the `COGNITION_ROUTE_*_BASE_URL` lines from the
   other hosts** so nobody makes remote model calls any more, and restart them.

### Verify

```sql
-- Work is moving (run a minute apart; the number should fall)
SELECT stage, status, count(*) FROM pipeline_work GROUP BY 1,2 ORDER BY 1,2;

-- Nothing is dead-lettering
SELECT count(*) FROM pipeline_work WHERE available_at > now() + interval '50 years';
```

```bash
# The new worker registered the stages you expect
journalctl --user -u scoracle-cognition --since "5 min ago" | grep 'registered stage handlers'

# Both GPUs are actually busy — if one sits idle, a role is still routed off-box
nvidia-smi --query-gpu=utilization.gpu --format=csv    # Linux
```

### Security notes

- `sslmode=disable` is fine on loopback and **not** fine across a LAN — the password crosses
  in clear text. Use `sslmode=require` once a second machine connects.
- Scope every `pg_hba` line to a `/32`. Never `0.0.0.0/0`.
- Set **DHCP reservations** for every machine. IPs appear in `pg_hba.conf`,
  `COGNITION_BACKEND_CONCURRENCY`, and any `_BASE_URL` — a moved lease breaks all of them, and
  the failure looks like a model outage rather than a network change.

## 7. Things that are NOT how you scale this

- **Do not shard the queue by stage or entity.** `SKIP LOCKED` already distributes; a manual
  split creates idle workers the moment the mix changes.
- **Do not run two workers against one GPU** to "use it harder". The governor already caps
  concurrency for that card; a second process just doubles the KV cache and risks spilling to
  host RAM, which on a discrete GPU is a cliff, not a slope. (Measured 2026-08-24: an 8B model
  needing 8.54 GB on an 8 GB card ran at 18.8 tok/s against the 3B's 67.4 — 3.6× slower.)
- **Do not add a stage to one worker only** unless you mean it. Stage lists are per host, so an
  asymmetric list silently makes one machine the single point of failure for that stage.
- **Do not "fix" a stuck-looking queue by resetting `running` rows.** The lease loop owns that;
  see §2.

## 8. Model consistency is a provenance question

Every card records `model_version` and `prompt_version`. With one model everywhere, which
worker claimed an item does not matter. **The moment two workers run different models, the
machine that won the race decides which model wrote that card** — and the row will say so,
which is exactly why the columns exist.

That is allowed, but it must be deliberate: keep the tags identical across workers unless you
are running a measured comparison, and if you do split, expect the provenance columns to be the
only way to tell the halves apart.

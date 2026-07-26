# Handoff — the junction pass is DONE; open items below

Repo `/home/sheneveld/scoracle/scoracle-backend`, branch `main`, tree clean.
Fetch/pull both repos first; parallel sessions push to origin.
**This Archbox IS production.** Prod actions need Scott's named approval.

> **Tasks 1 and 2 completed 2026-07-25** (commits `d965459`, `3beb75c`, `a277c9a`, `d4b55d1`).
> See "What landed" below before reading the rest — several sections of this doc describe the
> old layout. The binary in `bin/` is UNCHANGED; the refactor is not deployed.

Read `scoracle-wiki/progress_docs/2026-07-25_relevance-root-cause-and-teardown.md` — the plan of
record, plus the evening ADDENDUM which supersedes several of its phases. Do not re-derive the
diagnosis; it is measured and settled.

## State — all deployed and running

The news rail was rebuilt today: Google casts the net → Go records facts → **The Reader judges** →
The Journalist builds memory. The scrub GPU relevance gate, `resolve.rs`, `bin/relevance_bands.rs`
and all BGE/candle use outside `narratives`/`threads` are **deleted**. Novelty is pure
`token_jaccard >= 0.90`. Headline passthrough, Google `feed_rank` (mig 194) and a per-entity read
budget are live. The Reader runs `gemma3:4b`.

## What landed — Tasks 1 and 2, 2026-07-25

Every model-calling seat is one directory under `rust/src/junctions/`, named for its **character**,
holding exactly three files: `mod.rs` (stage machinery), `prompt.rs` (system prompt +
`*_PROMPT_VERSION` + format schema + builder, nothing else), `tests.rs`.

| junction | was | now | contract |
|---|---|---|---|
| The Reader | `reader.rs` | `junctions/reader/` | `ar3` |
| The Journalist | `narratives.rs` | `junctions/journalist/` | `n13` |
| The Oracle | `sigil.rs` | `junctions/oracle/` | `or5` |
| The Insider | `transfer.rs` | `junctions/insider/` | `t11` + `is1` + identity-adjudication-v2 |
| The Influencer | `vibe.rs` | `junctions/influencer/` | `v14` |
| The Analyst | `momentum.rs` | `junctions/analyst/` | `momentum-s7` |
| The Scout | `rating.rs` | `junctions/scout/` | `s14` |
| *(not a character)* | `graph.rs` | `junctions/graph/` | `g3` |

`src/prompts/` was **deleted** — one home per junction, not two. This deviates from the plan above,
which wanted a parallel `src/prompts/` tree; Scott chose one-dir-per-junction, so the already-shipped
`prompts/reader.rs` folded into `junctions/reader/prompt.rs`. `judge.rs` is eval tooling, not a live
junction, and was left alone.

Also done: 2,982 lines of test module moved out of the stage files into `tests.rs` (no file in
`src/junctions/` exceeds 2,000 lines now, down from four); six stale header claims corrected
(`route(SynthesisLogic)` → `OracleLogic`, four wrong `route(EmotionalNews)` claims, prompt versions
in prose reading n5/s9/t5). Verified byte-identical prompt bodies by diffing every line against the
parent commit — the only differences are `use` statements and five deliberate visibility widenings.
`cargo test --lib` stayed at **230 passed** through all four commits; zero build warnings.

**Traps for the next session.** Queue-stage identifiers were deliberately NOT renamed —
`pipeline_work` rows and `COGNITION_STAGES` still say `narratives`, `sigil`, `vibe`, `transfers`,
`peak`, `momentum`, because those name rows in a table, not seats. And `examples/graph_probe.rs`
joins `transfer_t10_fixtures.rs` as pre-existing-broken: it references `Harness.resolve`, deleted
with `resolve.rs` in the teardown.

**Not deployed.** `bin/scoracle-cognition` is still the pre-refactor binary; prod runs unchanged.

## Open items, in priority order

1. **Watch The Reader's `irrelevant` rate — and disambiguate it.** gemma3:4b sat at **0.0% across
   its first 24 readings** against mistral's **14.4%** baseline. At n=24 that is p≈2.4% under the
   baseline rate, so it is no longer dismissible as small-sample noise. It is now the *sole*
   relevance judge; if it never rejects, it is doing half its job.

   **But there is a strong confound, and it is the more likely explanation.** The 14.4% baseline was
   measured on a FIFO queue of everything; gemma reads only top-ranked articles under the new budget,
   and Google's top hits are genuinely more often about the entity. A lower rejection rate is what a
   working ranking system *should* produce. The two are distinguishable only with a rank-matched
   comparison, which was impossible on 07-25 because `feed_rank` had just started populating.

   With a day of data, compare like with like — if the rate stays ~0% even on poorly-ranked
   articles, the judge is the problem; if it rises as rank worsens, the ranking is working:
   ```sql
   SELECT CASE WHEN a.feed_rank IS NULL THEN 'unranked'
               WHEN a.feed_rank < 3 THEN 'top3' ELSE 'rest' END AS band,
          r.model_version, count(*),
          round(100.0*count(*) FILTER (WHERE r.status='irrelevant')
                /NULLIF(count(*) FILTER (WHERE r.status IN ('success','irrelevant')),0),1) AS irrelevant_pct
     FROM news_article_readings r JOIN news_articles a ON a.id=r.article_id
    WHERE r.updated_at > NOW()-INTERVAL '24 hours' GROUP BY 1,2 ORDER BY 1,2;
   ```
   A cheaper direct check that needs no rank data: hand-read 20 of gemma's `success` verdicts and
   look for articles that plainly are not about the entity.
   ```sql
   SELECT t.name AS entity, left(a.title,70) AS title, left(r.evidence_blurb,90) AS blurb
     FROM news_article_readings r
     JOIN news_articles a ON a.id = r.article_id
     JOIN news_article_entities nae ON nae.article_id = a.id AND nae.vetted IS TRUE
     JOIN teams t ON t.id = nae.entity_id AND t.sport = nae.sport   -- sport! see Traps below
    WHERE r.model_version = 'gemma3:4b' AND r.status = 'success'
    ORDER BY r.updated_at DESC LIMIT 20;
   ```
2. **Then raise the read budget to `COGNITION_ARTICLE_READ_TOP_K=8`** in `.env.local` + restart.
   Measured: K=4 → 701 reads/day (16.4% of ingest), K=8 → 1,058/day (24.7%) — Scott's 25% target.
   gemma sustains ~3,150/day (131.4/hr vs mistral 53.7/hr, 2.45x), so K=8 is affordable.
3. **Phase 2.3 — delete the panic guards** (`-rss-limit`, `short_code` solo lanes,
   `newsMaxTeamAliasRSSQueries`, risky-solo-term lists), then re-run
   `./scripts/ops/news_ingest_funnel.sh`. The funnel shows `-rss-limit` discarding **3,401 of 5,267**
   articles per sweep *after* they were fetched, matched and deduped. Do this last — it raises volume.
   Also make truncation rank-aware or it keeps discarding Google's top hits by construction
   (it truncates *after* `sortArticlesByDate`).
4. **Phase 3 of the plan — thread identity to The Journalist.** Build the `narrative_threads` merge
   path first (4,457 singletons), add `continues_thread` to the output contract, fix E7
   (`threads.rs:131` has `FOR UPDATE` with no `ORDER BY`). This is what finally deletes
   `Harness.embedder` — `narratives` clustering and `threads.rs` centroid are its last two consumers.
5. **~~Ollama is thrashing the GPU~~ — RESOLVED.** The topology split took it 101 → 22
   reloads/hour, and matching graph's `num_ctx` to the Reader's 8192 addresses the remainder
   (see "Phase 2 as shipped"). Gemma is now pinned with `OLLAMA_KEEP_ALIVE=-1` on a box that
   holds one model. The original diagnosis is kept below for its measurements.

   **Ollama is thrashing the GPU — measured 07-25, undecided.** **702 `llama runner started`
   events in 6 hours** (~1 reload/min). `mistral:7b` (5.1 GB, every character) plus `gemma3:4b`
   (~3.3 GB, The Reader) is ~8.4 GB against the 1070 Ti's 8192 MiB, and `OLLAMA_KEEP_ALIVE=30m`
   has both trying to stay resident, so an evict-and-reload fires on nearly every alternation
   between The Reader and a character stage. Note this interacts with item 2: raising `TOP_K`
   raises Reader volume, which raises the alternation rate.

   Scott asked about `OLLAMA_FLASH_ATTENTION=1` + `OLLAMA_KV_CACHE_TYPE=q8_0`. q8_0 KV
   **requires** FA (hard dependency), and this card is **compute capability 6.1 (Pascal)** — the
   bad case: GP104 runs fp16 at 1/64 of fp32 and llama.cpp's tensor-core FA kernels need cc ≥ 7.5,
   so it falls back to vec kernels. Expect flat-to-slower tok/s. The win is *headroom* (~240 MiB
   off mistral's KV at 4096 ctx, less on gemma3's sliding-window attention) — roughly half the
   ~400 MiB gap, so it is a coin flip alone. `OLLAMA_MAX_LOADED_MODELS=1` or a shorter keep-alive
   on the Reader route attacks the thrash directly. Judge by the reload count, never tok/s:
   `journalctl -u ollama --since "1 hour ago" | grep -c "llama runner started"`.

   **Trap:** ollama is a **system** unit (`/etc/systemd/system/ollama.service`, `User=ollama`),
   NOT `systemctl --user` like the scoracle units. Restarting it drops every loaded model.
6. `topic_heat_embeddings` is orphaned — nothing reads or writes it. Drop in a later migration.
7. ~~`examples/transfer_t10_fixtures.rs` and `examples/graph_probe.rs` do not compile~~ — **FIXED
   2026-07-26.** `graph_probe` set `resolve:` on `Harness` (gone with `resolve.rs` in the
   teardown); `transfer_t10_fixtures` passed a 4th `best_weight` arg to
   `TransferEvidence::from_news`, which lost that field. `cargo build --examples` is clean.
   The graph gate this unblocked is recorded under "The graph-on-gemma gate" below.

## The Archbox/Mac split — rollout (Phase 1 code shipped `dfbf78a`, NOT yet configured)

One model per machine: **gemma3:4b alone on Archbox** (The Reader, the gatekeeper) and **the six
characters on the M4 mac mini** (16 GB unified). This ends the VRAM thrashing (item 5) by removing
the thing that causes it — two models on one 8 GB card — rather than tuning around it.

**Networking: no hardware needed.** What crosses the wire is a prompt and a completion — tens of KB
per call, at roughly one call per 30–60s. Any existing LAN is orders of magnitude more than enough;
Wi-Fi is fine, wired is nice only for stability. LAN latency (~1 ms) against a ~55 s generation is
0.002% overhead, so a Tailscale tunnel is equally fine and is the better default: it gives a stable
MagicDNS name instead of a DHCP-assignable IP, and **the ollama API has no authentication**, so it
must never be exposed to the internet. LAN or tailnet only.

**On the Mac:**
```sh
brew install ollama && brew services start ollama
launchctl setenv OLLAMA_HOST "0.0.0.0:11434"   # default binds localhost only
sudo pmset -a sleep 0 disablesleep 1            # THE failure mode; also "Wake for network access"
ollama pull mistral-nemo:12b                    # 12B @ Q4_K_M ≈ 7.1 GB — see below
ollama run --verbose mistral-nemo:12b "..."     # record real tok/s before committing
```
Prefer a **12B at Q4_K_M over an 8B at Q8**: at a fixed memory budget parameter count beats
quantization precision, and Q8 costs ~half the speed (M4 ≈ 120 GB/s ⇒ 8B@Q8 ≈ 10 tok/s vs
12B@Q4 ≈ 13 tok/s) to buy back well under 1% perplexity. Avoid thinking models here — a reasoning
trace multiplies output length 3–10x, and every character is on the critical path to the sigil card.

**On Archbox** — append to `.env.local`, then restart the unit:
```sh
COGNITION_BACKEND_CONCURRENCY="http://localhost:11434=3,http://mac-mini:11434=1"
for R in NARRATIVE_LOGIC ORACLE_LOGIC TRANSFER_LOGIC VIBE_LOGIC MOMENTUM_LOGIC \
         STATS_LOGIC EMOTIONAL_NEWS MULTILANG SQL; do
  COGNITION_ROUTE_${R}_BASE_URL="http://mac-mini:11434"
  COGNITION_ROUTE_${R}="mistral-nemo:12b"
done   # ARTICLE_READER is deliberately absent — it stays local on gemma3:4b
```
`OLLAMA_KEEP_ALIVE=30m` becomes harmless once each host holds one model, and the flash-attention
drop-in from item 5 should be REMOVED if it was ever installed (pure downside on Pascal once gemma
runs alone).

**Verify before flipping** — reads no database, safe while the service is up:
```sh
set -a && source .env.local && set +a && cargo run --example topology_probe -- --ping
```
Expect two hosts, `article-reader` alone on localhost, and no "distinct models will contend"
warning. Boot logs then carry `resolved model topology` and one `ollama reachable` per host.

**Roll back** by deleting those env lines and restarting — the code is inert unconfigured, and
single-host behaviour is byte-identical (test: `single_host_deploys_build_exactly_one_governor`).

**Then reconsider the read budget.** `COGNITION_ARTICLE_READ_TOP_K` exists because reads were
scarce and had to be rationed to Google's best-ranked hits. With gemma alone on the card at 3
concurrent, scarcity likely ends — and if you read everything, `feed_rank` goes back to being an
ordering rather than a gate, which dissolves the rank confound in item 1 for free.


## DECISIONS — 2026-07-26 afternoon, before the compositor restart

Scott's calls, made with the measured numbers in hand. These SUPERSEDE anything below that
conflicts.

> **All four are now EXECUTED** (1, 2, 3 in the session after the restart; 4 earlier that day).
> See "Phase 2 as shipped" at the end of this doc for what landed and what to watch.

1. **Archbox comes off the sequencing approach entirely.** It should work items as they arrive,
   not one per rotation. The rotation existed to stop a single GPU being oversubscribed; the
   per-host semaphores (`dfbf78a`) now do that job properly, so **the governor becomes the
   scheduler** and the rotation constraint can go. Reader and graph pull continuously up to the
   host's slot count.
2. **The Mac keeps sequencing** — `COGNITION_BACKEND_CONCURRENCY` pins it at `=1`, because 16 GB
   will not hold two KV allocations for a 14B and `OLLAMA_NUM_PARALLEL=1` there means a second
   request would queue inside ollama with its timeout clock running.
3. **4 concurrent slots on Archbox**, after the compositor restart frees ~1.8 GB. Set
   `OLLAMA_NUM_PARALLEL=4` and `COGNITION_BACKEND_CONCURRENCY=http://localhost:11434=4,...`.
   The two numbers must match: slots the harness will use vs slots ollama actually allocates.
4. **Ingest moved 6h -> 12h** (`0 */12` in crontab, DONE 2026-07-26 12:40; previous crontab backed
   up to `~/.cache/crontab/crontab.bak` and to this session's scratchpad). Deliberate while the
   backlog drains and the tuning settles. **Known cost, accepted:** Google News RSS is a capped
   rolling window, so half the sweeps means roughly half the articles ever seen — not deferred,
   gone. It also sharpens the existing `-rss-limit` bias, which truncates AFTER `sortArticlesByDate`
   and so drops the older half of a longer window by construction. Revert with `0 */6` when tuning
   is done. `cron-narrative-links.sh` deliberately stays at 6h — different, cheap job.

The intended end state: **Archbox never backlogs.** Reader and graph chew through work as it
arrives on 4 slots, and the only thing anyone waits on is the Mac — which is the correct place for
the constraint to live, since that is where the expensive character voices are.

## PHASE 2 — SHIPPED 2026-07-26 afternoon (all four items below are DONE)

> **Status: code complete, `cargo test --lib` 251 passed / 0 failed, zero build warnings.**
> What changed, and what to watch, is recorded at the end of this section under
> "Phase 2 as shipped". The plan text below is kept because its measurements are the
> baseline the next window compares against.

Written 2026-07-26 12:15 after a measured 30-minute window. Execute in a fresh context.

**Where we are.** The split is live and correct: gemma3:4b alone on Archbox (Reader + graph),
ministral-3:14b on the Mac (six character voices). Thrashing went from **101 reloads/hour to 22**,
and those 22 have a known cause (item 1). Throughput is **255 calls/hour against a 310 baseline,
-18%** — the Mac runs at ~89% utilisation while Archbox idles waiting on it, because the drain is
sequential. Nothing is broken; the machine is just half-used.

Measured 30-min window: graph 1446->1437 pending (draining at ~96/hr against ~78/hr arrivals),
article_read 233->205 then re-injected to 453 by an ingest sweep, sigil flat at 426 (5 drained,
~5 arrived — equilibrium, not a stall), Reader 134->95 reads/hr (starved by slow rotations).

### 1. graph's num_ctx — one line, removes the last of the thrashing

`reader/mod.rs:315` sends `num_ctx: 8192`; `graph/mod.rs:70` sends `num_ctx: 0`, which the client
omits, so ollama falls back to its server default. **Two context sizes on one model force a runner
reload**, and they alternate constantly. Set graph to `ARTICLE_NUM_CTX` (8192) — the 8192 runner is
already what the Reader makes us pay for, so this costs no extra VRAM and should take reloads to
~0. **Confirmed, not theorised:** reloads arrive in PAIRS 12-17s apart every 6-7 minutes
(11:59:48/12:00:05, 12:05:48/12:06:01, 12:09:32/12:09:44, ...) -- one rotation loading 8192 for
the Reader then the default for graph. Verify after the fix:
`journalctl -u ollama --since "1 hour ago" | grep -c "llama runner started"`.

### 2. Phase 2 proper — concurrent drain

`worker.rs:drain_all` is `for handler { for item { handle().await } }`. Make stages run
concurrently; the per-host semaphores from `dfbf78a` already bound it correctly and need no change.

**Prerequisite, not optional: fix E7 first.** `threads.rs:131` has `FOR UPDATE` with no `ORDER BY`.
Two transactions locking the same rows in different orders is a textbook deadlock. It is harmless
today ONLY because the drain is sequential — Phase 2 is precisely what makes it reachable.

Watch for: two stages claiming the same item (the lease is the guard — verify it holds under
concurrency), `Harness.embedder` shared across tasks (narratives is its last consumer), and the
handler timeout now measuring wall time that includes waiting for a busy host.

Expected result: the Reader recovers toward 134+/hr and graph runs flat out, because neither has to
wait behind ~5 minutes of Mac work per rotation.

### 3. OLLAMA_NUM_PARALLEL=2 on Archbox — after Phase 2, not before

Batched inference reads the weights ONCE per batch, so on a bandwidth-bound card two slots approach
2x throughput rather than 1.3x. Archbox has NO `OLLAMA_NUM_PARALLEL` set today, so it serves one at
a time no matter what the harness sends — which is why this only pays off after Phase 2.

**Use 3.** An earlier draft of this plan said 2, from a generic-4B estimate of ~1.1 GB per slot.
That is wrong for this model: **gemma3 uses sliding-window attention on 5 of every 6 layers**, so
those layers cap KV at a 1024-token window rather than the full 8192 and the cache is a fraction of
a standard 4B's. Measured: ollama holds 3,880 MiB total for weights (~3.3 GB) plus KV plus compute
buffers, so one slot's KV is ~350-500 MB. Against 1,835 MiB free, 3 slots (~+800 MiB) fits
comfortably and 4 probably would. Set `localhost=3` in COGNITION_BACKEND_CONCURRENCY to match --
it is already 3, so that line needs no change after all.

**The 1.8 GB of 'missing' VRAM is the desktop, not a leak in our stack:** `cosmic-comp` holds
1,872 MiB (plus ~150 MiB of panel/portal/Xwayland/ghostty). It has been up 22 days driving a
3840x1600 ultrawide, whose framebuffers should cost ~300 MB -- so most of that is likely
accumulation and a logout/login would reclaim it. NOT required for 3 slots; it is the lever if you
ever want 5-6, or if the box goes headless.

**And correct a live misconfiguration:** `COGNITION_BACKEND_CONCURRENCY` currently says
`localhost=3`. With 2 slots the third request queues INSIDE ollama with its timeout clock running.
Set `localhost=2` to match. It is a system unit: `/etc/systemd/system/ollama.service.d/`.

### 4. Close the fixture-gate gap that let a bug into production

The Markdown break (`**SCORE: -1**` rejected by a bare-prefix parser, fixed in `190a83a`) should
never have shipped. `bin/eval` with `COGNITION_ROUTE_<ROLE>_CANDIDATE` exists to catch exactly this
and was skipped. Two jobs: run the character fixtures for ministral-3:14b vs the mistral incumbent,
and repair `examples/graph_probe.rs` (broken since `resolve.rs` was deleted) so the graph-on-gemma
swap shipped today can be gated retroactively. `fixtures/graph/` is empty; regenerate via
`examples/graph_fixture_gen.rs`.

### 5. Then revisit capacity honestly

The Mac at ~89% is the binding constraint, and 426 sigil items are not draining. Options once Phase
2 lands: `mistral-nemo:12b` (7.1 GB vs 9.1 GB — faster, and leaves room for NUM_PARALLEL=2 on the
Mac too), or move the highest-volume voice (momentum, 786/day) back to local gemma. Decide with
numbers, after concurrency, not before.

**Also still open from the original list:** the read budget may no longer need to exist now that
reads are not scarce (dissolves item 1's rank confound), and `topic_heat_embeddings` is orphaned.

### Phase 2 as shipped — 2026-07-26 afternoon

**The 1.87 GB was NOT a leak in our stack, and it is now reclaimed.** A cosmic-comp restart took
the compositor from **1,872 MiB to 67 MiB**; total desktop GPU use is ~212 MiB across
comp/panel/Xwayland/portal/ghostty/chrome. Free VRAM went **1,835 MiB → 3,916 MiB** against
ollama's steady 3,880 MiB. It was 22 days of accumulation on a 3840x1600 ultrawide whose
framebuffers only justify ~300 MB. **If it creeps back, the remedy is a logout/login, not a hunt
through our code** — nothing in the harness allocates compositor memory.

1. **graph's `num_ctx` — DONE.** `graph/mod.rs` now sends `reader::ARTICLE_NUM_CTX` (8192)
   instead of `0`. `ARTICLE_NUM_CTX` became `pub(crate)` with a comment on both sides saying the
   two must change together; the value is no longer duplicated, because drift between them is
   precisely what caused the paired reloads.
2. **Concurrent drain — DONE.** `worker.rs::drain_all` was `for handler { for item { await } }`.
   It now keeps N claimed items in flight in a `FuturesUnordered`, topping up after each
   completion. Per decision 1 **the governor is the scheduler**: the drain only keeps work
   offered, and the per-host semaphores decide what runs.
   - Concurrency is **intra-task** — the futures are polled by the one drain task and never
     spawned, so the 07-15 incident property holds (stage futures, embedder and GPU stay off the
     supervisor's task; nothing a handler does can pin the LISTEN socket).
   - **The control is per-stage, not global.** `StageHandler::max_in_flight()` (new; default 1)
     caps how many items of one stage may run at once. graph and The Reader are **2** each —
     they are the only two stages on the local gemma3:4b, so between them they fill Archbox's 4
     slots. Every Mac stage stays at 1: the Mac runs one request at a time by design, so extra
     in-flight items there would only queue on its semaphore, holding leases and burning their
     handler-timeout clock for no throughput.
   - `max_in_flight` is NOT `rotation_batch`. The latter is how many rows to claim in one SQL
     round trip; the former is how many slots the stage may hold. scrub wants 256 rows per trip
     (its items are microseconds) but must hold ONE slot, because every slot it holds is a slot
     the GPU cannot use.
   - `COGNITION_DRAIN_CONCURRENCY` is now an optional **throttle**, not the primary knob. Unset
     (the normal case) the worker derives the ceiling as the sum of the stages' caps, so it can
     never bind. `=1` restores the old strictly-sequential drain — that is the rollback.
   - **Sizing the ceiling to total GPU permits (5) was tried and is wrong** — recorded because it
     is the obvious idea. The drain claims in DAG order, so scrub(1) + graph(2) + article_read(2)
     fills all 5 and the six Mac stages get NOTHING until the local backlog (2,580 items) is
     empty. The Mac would idle for hours while Archbox worked — the exact inversion of the goal.
     `resolve_drain_concurrency` carries this note and a test asserts the ceiling cannot starve.
   - E7 was the hard prerequisite and is fixed: `threads.rs` open-thread `SELECT … FOR UPDATE`
     now has `ORDER BY id`.
3. **`OLLAMA_NUM_PARALLEL=4` — DONE**, in a new drop-in
   `/etc/systemd/system/ollama.service.d/concurrency.conf`, with `COGNITION_BACKEND_CONCURRENCY`
   moved `localhost=3 → 4` to match. The drop-in also sets **`OLLAMA_KEEP_ALIVE=-1`** (pin gemma
   in VRAM; Scott: "we won't be switching") and **`OLLAMA_MAX_LOADED_MODELS=1`** as the guard
   that keeps the pin true. Note the old handoff claim that `OLLAMA_KEEP_ALIVE=30m` was set was
   **wrong** — the unit had no `Environment=` lines at all and ollama was running the 5m default,
   so an idle gap silently unloaded gemma and the next request paid a cold load.
   Flash-attention/q8_0 KV stay deliberately absent (Pascal cc 6.1; their only win was headroom,
   which the reclaim made moot).
4. **Config defaults realigned to what actually runs.** `OLLAMA_TIMEOUT_SECONDS` default
   **60 → 600** and `COGNITION_HANDLER_TIMEOUT_SECONDS` default **900 → 1200**. Both had been
   overridden in `.env.local` by every real deploy, so reading `config.rs` misdescribed the
   system — it cost this session a round-trip. Live values are unchanged.
5. **`COGNITION_DB_MAX_CONNS` 5 → 25 — a real bug the concurrency exposed, caught in
   production within two minutes of the deploy.** A pool of 5 was sized for a drain that ran ONE
   item at a time. With up to 11 handlers in flight, each holding a connection and some holding a
   transaction plus a query, the pool starved:
   `narratives … debounce check news_summaries player/322: pool timed out while waiting for an
   open connection`. **Anything that raises `max_in_flight` or adds a stage must re-check this
   number** — it has to stay comfortably above the sum of the stage caps. Postgres here allows
   100 with ~22 in use, and a pool max is a ceiling, not a preallocation.

**The one thing to watch — the handler timeout now measures queueing.** An item's
`COGNITION_HANDLER_TIMEOUT_SECONDS` (1200s) clock starts at claim, so it now covers time spent
waiting on a busy host's semaphore, not just generation. The pathological case is several slow
Mac items in flight at once: a narratives item has been observed at 4–7 min, and 4 of those
queued ahead of a fifth would blow the budget.

With every Mac stage capped at 1 in flight, the exposure is bounded at **six Mac items, five of
them queued** on that one permit. At the ~55s calls measured on ministral-3:14b that is a ~275s
wait — comfortable. At the 4–7 min a narratives item has been observed to take, the last in line
would exceed 1200s and time out.

Why that is survivable, and the lever if it bites:
  * A timeout is fail-closed and self-healing: the item fails with backoff and retries. Nothing
    is persisted for it and nothing is corrupted. It costs throughput, not correctness.
  * Total throughput barely moves either way — the Mac runs one request at a time regardless, so
    a queued item was never going to run sooner.
  * **If it does bite**, set `COGNITION_DRAIN_CONCURRENCY` to ~6 rather than raising the timeout
    (which must stay under `COGNITION_STALE_LEASE_SECONDS=1800`). At 6 the local stages take 5
    (scrub 1 + graph 2 + article_read 2) and exactly one Mac item is in flight, so the Mac queue
    disappears while local work exists. It returns when Archbox goes idle — unavoidable without
    host-awareness.
  * **The real fix, if it ever earns the work:** make the drain host-aware so it never claims
    more for a host than that host can run. That needs a stage→host map the worker does not
    have — `StageHandler` does not declare its role, and some stages use several.

**Also watch:** `Pulse` is shared, so a busy drain now beats for whichever item moved last — the
watchdog can no longer see one wedged item behind others making progress. That is acceptable
because the per-item handler timeout, not the watchdog, is the guard for a hung handler; the
watchdog stays the backstop for a drain where *everything* stopped.

**Deploys are slower now, and that is expected — do not go looking for a hang.** On shutdown the
drain lets in-flight items finish their own bookkeeping rather than abandoning their leases, so a
restart waits for the slowest of up to 11 items instead of 1. In practice it runs to the
supervisor's 75s `SHUTDOWN_GRACE`, which then aborts whatever is left (nothing is persisted for an
aborted item; its lease recovers via `requeue_stale`). Expect `systemctl --user` to sit in
`stop-sigterm` for up to ~75s after every binary deploy.

### MEASURED AFTER THE CUTOVER — 2026-07-26 13:44

All of it deployed, verified live, and holding.

| | before | after |
|---|---|---|
| local generate calls | 255/hr (both hosts, 12:15 window) | **~1,940/hr local alone** (97 in 3 min) |
| runner reloads ("switches") | 23/hr, in pairs every 4–5 min | **0** since ollama's restart, under full load |
| `cosmic-comp` VRAM | 1,872 MiB | **67 MiB** |
| free VRAM | 1,835 MiB | **3,039 MiB** (with 4 slots allocated) |
| `OLLAMA_NUM_PARALLEL` | 1 | **4** |
| `OLLAMA_KEEP_ALIVE` | 5m (default) | **pinned** (`-1`) |
| DB pool | 5 | **25** |

**The per-stage caps are provably holding in production.** Filtering `pipeline_work` to rows
claimed after the restart: article_read 2/2, graph 2/2, and momentum/narratives/peak/sigil/vibe
1/1 each — 9 in flight against a ceiling of 11, so the caps govern and the ceiling does not bind.
The 4 local slots are exactly graph(2) + article_read(2), which is the whole design.

**Reading `status='running'` needs care after a restart.** Aborted in-flight items keep their
`running` row until `requeue_stale` reclaims them 30 min later, so a naive count shows stages
over their cap. Filter by `updated_at > <service start>` before concluding anything is wrong —
three such orphans (momentum 3, narratives 1030, transfers 68) were present and harmless here.

**Verify a switch is a switch.** A `llama runner started` line immediately after
`Started Ollama Service` is the pinned model's cold load, not an eviction. Only a reload with no
preceding restart is a real switch, and with one model pinned on the card there should be none:
`journalctl -u ollama --since "1 hour ago" | grep -c "llama runner started"`.

### The graph-on-gemma gate — run retroactively 2026-07-26 (Phase 2 item 4)

Both broken examples are repaired (open item 7), which unblocked the gate that item 4 wanted.

**The fixtures were stale and would have gated the wrong contract.** `fixtures/graph/` was NOT
empty — the earlier note here was wrong; it held four fixtures frozen at **`g2`** while the live
builder is **`g3`**. The g2→g3 delta is not cosmetic: g3 adds the entire **Language handling**
paragraph (read the source language, emit English keys/enums, never drop relations because the
article is non-English), plus em-dash→`--` and a trailing-space fix. Regenerated via
`examples/graph_fixture_gen.rs`; only `prompt_version` and `system` changed, with `user_prompt`,
`expect` and the curated `note` preserved.

**Result: gemma3:4b passes 10/12 property checks.**

| fixture | verdict |
|---|---|
| object-attachment-two-suitors | **✓ all 5** — attaches to the true counterparty, finds the coach |
| person-discovery-manager-named | **✓ all 4** — finds both coach and agent |
| no-relation-quiet-mention | **✗** over-extracts: 1 relation where a clean empty was required |
| unary-injury-no-counterparty | **✗** misses the unary injury entirely: 0 relations, wanted `1:injury:-` |

Read that against what the set was built for. All four fixtures pin residuals **measured on the
g1/g2 probes, before the gemma swap** — so the two gemma passes are the two failures the set was
authored from, and the two it fails are different ones. The swap is not a regression on this
evidence; it moved the residual. The open half is **over-extraction and unary relations**, which
is the sharpest available statement of what to fix in the graph prompt next.

**Do not A/B this against mistral on Archbox.** Loading a second model is exactly what
`OLLAMA_MAX_LOADED_MODELS=1` and the `-1` keep-alive now forbid, and it would reintroduce the
evict-and-reload thrash this whole day removed. Run a challenger on the Mac, or accept 10/12 as
the recorded gemma baseline.

## Operational

- Env: `set -a && source .env.local && set +a`. `.env.local` is **gitignored** — the gemma route
  lives only on this box, with its rationale and rollback in a comment beside it.
- Deploy is atomic rename, never `cp` (ETXTBSY), never `pkill`:
  `cargo build --bin scoracle-cognition && cp target/debug/scoracle-cognition bin/.new && chmod 700 bin/.new && mv -f bin/.new bin/scoracle-cognition`
  The systemd **user** path unit auto-restarts. Go binary is `go/bin/pipeline`, same pattern.
- `COGNITION_STAGES` in `.env.local` **overrides** the systemd unit. Units are `systemctl --user`.
- `sql/schema/schema.sql` is a snapshot after mig 183; migs 184–194 live only in their files.

## Traps that cost real time today

- **`teams.id` is per-sport, not globally unique** — 204 rows, 157 distinct ids. Any join from
  `news_article_entities.entity_id` to `teams.id` **must** include `sport`, or ~47 teams are scored
  against another club's name. This silently turned a 100% result into 70%.
- **A stage with no model call must not be paced like one.** `StageHandler::rotation_batch()`
  exists because scrub, once model-free, was still draining one item per multi-minute rotation —
  a 7,165-item backlog would have taken weeks.
- **Verify a plan's claims against the code before implementing.** Phase 1.1 of the plan of record
  had already shipped hours before the plan was written; implementing it would have been a no-op.

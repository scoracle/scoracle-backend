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
5. **Ollama is thrashing the GPU — measured 07-25, undecided.** **702 `llama runner started`
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
7. `examples/transfer_t10_fixtures.rs` and `examples/graph_probe.rs` do not compile (pre-existing).

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

## PLAN — Phase 2: make the two machines actually work at the same time

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

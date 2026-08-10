# PLAN — Character Tuning (session notes)

> **COMPANION FILE — `PLAN-one-rail.md`.** These two are one document split by *kind*, not by topic,
> and a session usually needs both:
> * **That file is the RAIL** — phases, plumbing, migrations, cutover. **Its §0 working rules bind
>   every tuning session too** (build to `target/debug`, never `rust/bin/`; one change one
>   measurement; deploys are explicit; DB access from archbox; STOP on surprise). **Its Appendix D is
>   the LEDGER — the index of every D-T number.**
> * **This file is the DIAGNOSIS** — the numbers behind each D-T, plus **Appendix S** (the schema
>   inbox) and the per-session handoff fences.
>
> **The rule that keeps them from drifting: a finding is written in BOTH — one line in Appendix D,
> the detail here — in the same commit.** D-T20→D-T31 were indexed there only on 2026-08-08, twelve
> entries late; the ledger had stopped at D-T19 while this file had run on to D-T31.

**Founded 2026-08-05 by Scott's ruling: "this is a tuning issue… this session's goal is to
get the new rail built. Then we focus on tuning the LLM junctions to really ratchet up the
speed of the flow."**

**What this file is.** The working notes for post-rail **Character tuning sessions** — the
diagnosis detail behind Appendix D of `PLAN-one-rail.md`. The convention (written into the
Appendix D preamble):

- **Appendix D stays the ledger** — every junction quality/efficiency finding gets a D-T
  number there with its one-line measured baseline. A rail phase may cite the ledger; it may
  never halt on it.
- **This file carries the diagnosis** — numbers, code pointers, candidate knobs, and the
  measurement that would settle each knob. Add to it whenever rail work surfaces a finding;
  never fix mid-rail (the §4 law: plumbing gates phases, tuning is follow-up).
- **Tuning sessions run AFTER the rail stands** (post Phase 8 cutover, or in idle capacity on
  Scott's word), one knob at a time, one measurement per change (the ar4/ar5 lesson).

The laws still bind during tuning: describe-then-derive (T2); exact+discriminator or refuse;
a contract_version is a cache key — bumping one reopens ALL its work; stage wire names never
rename.

**SESSION ORDER (Scott, 2026-08-06): the VOICE session runs next — D-T23 (one article, many
tags) → D-T24 (the heat index moves to the character) → D-T25 (the Scout listens). A dedicated
SCHEMA session follows it, and works APPENDIX S at the end of this file.**

---

## ⭐ THE TWO STANDING TARGETS (Scott, 2026-08-08) — carry these through ALL tuning and rail work

**These are not queue items to be scheduled. They are the direction every knob is judged against,
and they outrank local tidiness.**

### TARGET 3 — **CLEAR THE DAILY WORK IN 4–5 HOURS** (Scott, 2026-08-08, after D-T34)

> *Scott: "oMLX is a big win here in speed. 2.3x means we can churn through our work that much
> faster. And when ctx windows are optimized, that makes it that much faster as well. We should, I
> believe, get to a place where we can get through our daily work in 4-5 hours on both machines."*

**ADOPTED AS THE THROUGHPUT OBJECTIVE.** MLX is the engine for the Mac voice tier (D-T34: **2.13×**
aggregate at 4 concurrent, measured). What follows is the arithmetic that says how far that gets us,
because the objective deserves a number rather than a feeling.

##### THE MEASURED BASELINE

| | |
|---|---|
| voice calls completed, best full day (Aug 6) | **1,771** *(Aug 7: 1,515)* |
| 7-day average | 1,108/day *(dragged by an Aug 4 outage: 0 calls)* |
| harness availability | ~**16 h/day** (runs 2 h, rests 1 h) |
| **effective rate today** | **≈111 calls/hour** |
| backlog | **8,954 pending** (D-T32 §voice queue) |

*(111/hour against a theoretical 174/hour at 2 concurrent × 41.4 s — the gap is real prompt variance;
`narratives`/`vibe` are far larger than the 2,372-token benchmark prompt.)*

##### WHAT THE OBJECTIVE REQUIRES, AND WHAT IS ACTUALLY IN HAND

| scenario | rate | time to clear 1,771 calls |
|---|---|---|
| today (llama.cpp, 2 slots) | 111/hr | **16.0 h** |
| **+ MLX at 2.13×** | ~236/hr | **~7.5 h** |
| **the 4–5 h objective** | **354–443/hr** | 4–5 h |

**MLX alone gets roughly 60% of the way there in time terms. It does not, by itself, reach 4–5 h.**
The objective needs **3.2–4.0×** over today; D-T34 measured **2.13×**. **Recording the gap rather
than rounding it away, because the remaining factor has to come from somewhere identified.**

##### ⛔ THREE CORRECTIONS THE ARITHMETIC FORCES — each changes what work to do

1. **"BOTH MACHINES" — MLX IS APPLE-ONLY. ARCHBOX GETS NOTHING FROM IT.** MLX is Apple-Silicon
   Metal; the 1070 Ti is Pascal/CUDA and keeps llama.cpp via ollama. **The Editor's throughput
   problem on archbox is not the engine — it is D-T32's cap** (which withholds 81% of arrivals) and
   the Editor already reads 100% of what it is asked for. **Two machines, two entirely different
   bottlenecks; do not apply the 2.13× to archbox.**
2. **⛔ FIXING D-T35 COSTS THROUGHPUT — the ctx work is not purely a speed win.** `narratives` today
   prefills only **2,051 tokens** because the rest is silently discarded. A *correct* prompt inside
   the ~3,540-token budget is **~73% MORE prefill work than it does now.** **So the correctness
   repair makes those two voices SLOWER, and part of MLX's 2.13× pays for it rather than banking it.**
   *(This is the sharpest reason not to promise ctx work as speed: for the two voices that most need
   fixing, correctness and speed point in opposite directions.)*
3. **LOWER ctx STILL DOES NOT SPEED A CALL** (`92a63d6`) — it buys slots. **On llama.cpp more slots
   measured WORSE (D-T30: 16.5 → 11.4). On MLX more concurrency measured BETTER (22.2 → 35.2).** So
   the chain `lower ctx → more slots → throughput` is **live on MLX and dead on llama.cpp here.**
   That is a second, independent argument for the engine change.

##### THE CHEAPEST UNMEASURED THING THAT COULD CLOSE THE GAP

**MLX was only tested to 4 concurrent, and it was still scaling** (13.1 → 22.2 → 35.2, not yet
flattening). **Measuring MLX at 6 and 8 concurrent is one afternoon in a rest window and could
supply most of the missing 1.5–1.9×.** **Also raise `COGNITION_BACKEND_CONCURRENCY` for the Mac
(currently `…1.77=3`) — the client cannot use slots it never fills.** ⚠ Both are memory-bound on 16 GB
and must be measured, not assumed — that is exactly the mistake D-T30 made.

**HONEST STATUS: 4–5 h is a plausible objective, NOT a projection the measurements support yet.**
The path is MLX (2.13×, in hand) + higher MLX concurrency (untested) + the D-T35 trim (a throughput
COST, a correctness necessity). **Do not report progress against 4–5 h until the concurrency scaling
above 4 is measured.**

---

### TARGET 1 — **`ministral-3:3b` IS the 1070 Ti model. Tune to it, not to `gemma3:4b`.**

> *Scott: "I want to tune to the better model and Ministral beat out Gemma significantly with
> prompts designed for Gemma. And it keeps us in the Mistral family of models which I align with
> over Google. No sense in tuning for models other than the one we're using."*

**Two reasons, and the second is the one that compounds:**
1. **It won on the incumbent's home turf — 52/53 vs 47/53 on prompts written FOR gemma.** The
   margin is therefore an UNDER-statement of the real gap; nothing has been tuned in its favour yet.
2. **Family alignment.** The six voices already run `ministral-3:14b`. One family across the rail
   means prompt formatting, chat templating and tokenizer behaviour stop being per-host variables.

**The operative consequence: every prompt/window/fixture measurement taken against `gemma3:4b` is
now provisional.** Do not spend effort tuning gemma-specific behaviour. When a knob is measured,
measure it on ministral or say plainly that the number is a gemma number pending re-measurement.

### TARGET 2 — **DRIVE CONTEXT WINDOWS DOWN. 4096 IS THE CEILING, NOT THE GOAL.**

> *Scott: "4096 should be the highest, but some of the characters will genuinely need much less.
> Lower ctx when it doesn't detract from the output is a win for throughput."*

**Per-character floors, not one global number.** D-T29 made 4096 uniform per HOST for a real reason
(one runner, no reloads), and that stays — but the target is now each character sized to what it
actually needs, and several are far under 4096 (`rating` ≈723 tok, `transfers` ≈1,119, `sigil`
≈1,897 on gemma's tokenizer).

**⚠ THE MECHANISM MATTERS, OR THE WIN WILL NOT APPEAR — measured, and already corrected once in this
repo (`92a63d6`).** `num_ctx` governs **MEMORY, not per-token compute**: attention cost scales with
the ACTUAL sequence length, so a 2,049-token prompt costs the same at 8192 as at 4096. **Lowering
`num_ctx` does NOT make a call faster.**

**The throughput win is real but INDIRECT, and it is a two-step:**

> **lower `num_ctx` → smaller KV → free VRAM → MORE CONCURRENT SLOTS → throughput**

**Step two is not optional. Headroom that is never spent on slots buys nothing measurable** — which
is exactly why D-T29's deploy is expected to read FLAT. **So every ctx reduction should be paired
with the concurrency raise that spends it, or booked as headroom banked, not as a win taken.**

> ### ⛔ TWO MEASURED CONSTRAINTS ON TARGET 2, BOTH FOUND 2026-08-08. READ BEFORE LOWERING ANY WINDOW.
>
> **1. TRIM THE PROMPTS FIRST. LOWERING `num_ctx` BEFORE THAT IS A SILENT QUALITY CUT (D-T35).**
> `narratives` and `vibe` already overflow 4096 and are **losing 71.5% of their prompt with their
> instructions evicted and no error raised** — the model fabricates rather than fails. **A ctx
> reduction applied before the trim evicts MORE and would look like a throughput win while corrupting
> output.** The usable content budget inside 4096 is **~3,540 tokens**, not 4,096 (≈554 tok of chat
> template). **Order: trim → verify nothing overflows → then lower.**
>
> **2. "MORE SLOTS" IS NOT AUTOMATICALLY MORE THROUGHPUT — MEASURED AND FALSIFIED (D-T30).**
> Raising the Mac to `-np 4` at 4096 made **every** figure worse (aggregate @4: 16.5 → 11.4 tok/s).
> The slots fit; the extra KV bandwidth cost more than they returned. **So the chain is
> `lower ctx → more slots → throughput` ONLY where memory bandwidth allows — verify on the host, do
> not assume it.** On the Mac the concurrency win came from a different engine entirely (D-T34: MLX
> at 2.13×), not from more llama.cpp slots.

### ⚠ THE TWO TARGETS PULL AGAINST EACH OTHER — MEASURED, NOT PREDICTED

**`ministral-3:3b` tokenizes the SAME TEXT into 32% MORE TOKENS than gemma3:4b** (2,705 vs 2,049 on
a byte-identical prompt). So **adopting ministral makes every window effectively ~32% smaller**, and
4096 headroom falls from 28% to **12%**.

**The rule that follows, and it is the expensive mistake to avoid:** **every per-character ctx floor
MUST be computed on ministral's tokenizer.** The voice numbers on record (`narratives` ≈7,574,
`vibe` ≈6,437, and the small ones above) are **scaled from gemma's 4.75 chars/token ratio and are
flagged in §D-T29 as indicative, not tokenizer-exact.** Setting a floor from a gemma number under a
ministral runtime is the **silent system-prompt eviction** failure — no error, no dead-letter, just
quietly worse output. **Re-measure first, then floor.**

*(Consequence already live: `ARTICLE_MAX_MODEL_CHARS = 9_000` is load-bearing for the 4096 window
under ministral and must not be raised without redoing that arithmetic.)*

**→ WHILE DOING VOICE WORK, LOG EVERY SCHEMA OBSERVATION TO APPENDIX S §S2. Do not act on it
unless it is COUPLED** — the test is *does the voice change make this safe, or is it merely near
it?* Only coupled changes ride inside a voice migration; everything else is logged and handed on.
The order is a dependency, not a preference: most of the schema debt is only droppable *because*
of something the voice work does, and D-T22 spent a whole session proving what happens when you
delete ahead of the code.

---

## Handoff — D-T19, the instrument (fresh context window; Scott's request 2026-08-06)

*Everything in the fence is measured. Facts gathered 2026-08-06 ~16:30 EDT while writing it — the
next session should verify, not re-derive.*

```
Work D-T19 in scoracle-backend: MAKE THE EDITOR FIXTURE GATE DETERMINISTIC. Nothing else.

This is an INSTRUMENT session, not a tuning session. The gate scored 47/53 then 43/53 on two
consecutive runs — same binary, same fixtures, same gemma3:4b. Until that spread is gone, every
editor number in PLAN-character-tuning.md is being read through a ±4 instrument and no knob you
turn afterwards can be scored. Fix the gauge. Do NOT fix a prompt in the same session.

READ FIRST: PLAN-character-tuning.md §0 (the register — start at §0c), then §6a (D-T19 itself),
then §1/§2 (D-T11/D-T12, the Editor findings the gate exists to score). Do not read the whole
repo. PLAN-one-rail.md is DONE for your purposes except its §0 working rules, which still bind.

STATE. RAIL=packet is live and the rail is clean. 2026-08-06 shipped four deletions in one day:
8.8 (the relevance regex — the entire Go tree now has three regexes, all RSS parsers), 8.9 (the
query builder — one Google query per name we know an entity by, no scoring/allowlists), 8.10 +
8.11 (news_article_entities went from 9 columns and two writers to 5 columns and ONE writer, the
Editor; `vetted`/`match_confidence`/`scrubbed_at`/`title_pos` dropped in mig 214). Deployed
@ 6fbf798, schema snapshot 9986cab. 9 stages, six voices on ministral-3:14b at the Mac (4096, 3
concurrent); the EDITOR runs gemma3:4b on ARCHBOX's own Ollama (localhost:11434, max_concurrent 4).

WHAT IS ALREADY MEASURED — do not spend the session re-deriving it:
  * The gate is `cargo run --bin eval -- --task editor --fixtures` on archbox. It is NOT in
    rust/bin; build to target/debug (§0 rule 6 — placing binaries in rust/bin or go/bin trips the
    .path watchers and restarts live services).
  * 12 fixtures in rust/fixtures/editor/. EVERY ONE pins "temperature": 0.0. So the eval is
    already greedy, and "it isn't really temp 0" is a DEAD lead — check it once and move on.
    (Production is temperature 0.2 via editor_opts(), editor/mod.rs:52. That asymmetry is
    deliberate and documented there; it is not the bug.)
  * The fixture loop is SEQUENTIAL (eval.rs:332, `for case in cases`). Nothing inside the eval
    runs concurrently, so intra-eval batching is also a dead lead.
  * GenerateOptions (ollama.rs, ~:41) has NO seed field. Nothing pins sampling on any call in
    this codebase. Adding one is a candidate fix, not a diagnosis.
  * The 12 fixture files declare 60 expect-keys between them, but the gate reports out of 53.
    RECONCILE THAT. A denominator you cannot derive from the files is its own instrument problem
    and may be hiding skipped checks.

THE LEADING HYPOTHESIS, and it is cheap to settle first: THE COGNITION DAEMON IS RUNNING WHILE YOU
EVAL. scoracle-cognition drains the editor stage against the SAME archbox Ollama at
max_concurrent 4, so an eval's requests are batched alongside live traffic, and batched inference
changes floating-point reduction order — which moves greedy output. Nothing in the eval is
concurrent, but the SERVER is.
  EXPERIMENT 1 (do this before any code change):
    systemctl --user stop scoracle-cognition
    run the gate 5x, record all five scores
    systemctl --user start scoracle-cognition
    run the gate 5x again, record all five
  If the quiet runs collapse to one number and the noisy ones spread, the instrument is
  contention and the FIX IS A METHOD RULE, not code: the gate is only valid with the daemon
  stopped. Write that into §6a and the gate's own doc comment so nobody scores a knob against a
  busy GPU again. If BOTH sets still spread, add `seed` to GenerateOptions, thread it through
  editor_opts, and repeat — one change, one measurement (§0 rule 4).

WHEN THE GATE HOLDS STILL — and only then — the two failure shapes are in §6a and are NOT this
session's to fix: names[] drops the coach/manager class (Shanahan, Moyes, Arteta, Bellingham,
Rangers-as-club), and register[outrage] reads neutral on the fan-protest fixture. Record the
STABLE baseline score, then stop. Both fixes are prompt changes and prompt changes belong in
PLAN-one-rail 7.11, which is deliberately ONE re-earn event: a *_PROMPT_VERSION bump is a cache
key and reopens ALL that stage's work fleet-wide. Same reason `momentum`'s markdown-instead-of-
contract failures (§0b) ride 7.11 rather than getting their own bump.

LAWS THAT STILL BIND: describe-then-derive (T2) — a model never renders a verdict as a bare
field; one change, one measurement; contradictions are preserved, never summarized away (T3);
stage wire names never rename; DB access is from archbox, not the Mac (§0 rule 8).

DO NOT TOUCH THIS SESSION: any prompt or *_PROMPT_VERSION (that is 7.11). Phase 9 (the Rust
demolition — article_reader/, scrub.rs, Role::ArticleReader, the embedder) and the 30,224 parked
article_read rows. PHASE 6 IS STILL OPEN on 6.7 alone — its window closes ~Aug 8 22:08 EDT; run
scripts/rail-6.7-bands.sh only after that, and never close phase 6 on an INTERIM banner. The
ingest path shipped today is settled: Google does the relevancy, the Editor is the valve — do not
re-open it.

TWO LESSONS FROM 2026-08-06 THAT APPLY DIRECTLY TO THIS WORK:
  1. A WARN that says "continuing" hides its own frequency. journalctl retention said 5 articles;
     the DATA said 9. Measure blast radius from the data, never from the log.
  2. SQL functions are code. compute_transfer_heat is LIVE and still carried a proximity gate for
     six hours after we recorded it as deleted from Rust. When you check whether something is
     gone, check pg_get_functiondef too.

FINISH RITUAL: update §6a with the spread you measured and what settled it, close or re-scope
D-T19 in Appendix D of PLAN-one-rail.md, commit as `tuning: D-T19 — <what made the gate hold
still>`, and print the next session's handoff (7.11, the voice diet) as the last thing you say.
```

---

## Handoff — D-T22, the schema audit (fresh context window; Scott's request 2026-08-06)

*Everything in the fence is MEASURED on live prod 2026-08-06 16:40–18:00 EDT. The next session
should verify what it acts on, but should not re-derive it from scratch — the sweep is done.*

```
Work D-T22 in scoracle-backend: FINISH THE SCHEMA AUDIT, AND SETTLE THE STALLED METADATA FEATURE.

Scott's framing, verbatim, because it is the whole brief: "We're running into issues where the
pipeline is clean, but we're attempting to force it to work with an outdated schema. We want both
to be optimized for the new approach of Google doing the relevancy work, and then empowering our
models for everything downstream. No most restrictive regex or fancy workarounds. Simple and
durable beats clever and fragile. We had to be clever before because we had no AIs working in our
stream. Now, we empower them to do the work and then update the SQL."

READ FIRST: PLAN-character-tuning.md 8b (both passes — the findings below are its summary), then
PLAN-one-rail.md 0 working rules, which still bind. Do NOT read the whole repo. The sweep is
already done; your job is the verdicts, the migrations, and the metadata decision.

STATE. RAIL=packet is live. D-T19 is CLOSED (the editor gate is deterministic with the cognition
daemon stopped; baseline 47/53). D-T21 IS LIVE AND ARMED as of 2026-08-06 17:39 —
EDITOR_MAX_READS_PER_ENTITY_DAY=10 in archbox .env.local, first real bite at the 02:00 cron. That
matters to you for ONE reason: read volumes changed on 2026-08-07, so do not read a drop in any
news/editor table as schema rot. It is the cap.
82 tables, 116 functions. DB is on archbox; access from archbox, never the Mac:
  ssh archbox 'cd ~/scoracle/scoracle-backend && set -a; . ./.env.local; set +a; \
    psql "${DATABASE_PRIVATE_URL:-$DATABASE_URL}" -c "select 1"'

=====================================================================================
THE METHOD. Four legs. Use all four or you will delete something load-bearing.
=====================================================================================
 1. Is the column/table still WRITTEN?  Split the count pre/post-flip (2026-08-06 10:55 EDT).
    A column written 19,862 times last week and 0 times since the flip is the whole finding.
 2. pg_get_functiondef across ALL 116 functions for the name, plus pg_get_viewdef across views.
    SQL FUNCTIONS ARE CODE. This is how compute_transfer_heat kept a deleted gate alive for six
    hours after we recorded it gone.
 3. Which of those functions has a live Rust/Go caller? A function nothing calls is not evidence.
 4. SHELL SCRIPTS THAT CALL SQL FUNCTIONS FROM A psql HEREDOC. This is the leg that was missing
    and it is the one that matters most here: scripts/hosting/recompute-tiers.sh (cron, Mondays
    02:00) runs `SELECT recompute_entity_tiers(...)` directly. That path is invisible to a Rust
    grep, a Go grep, AND a repo grep for the table name. Three of four legs miss it.

FOUR CONTAMINATIONS. Every one of these produced a wrong answer before it was corrected.
 A. THE 04:00 pg_dump SEQ-SCANS EVERY TABLE. Postgres 18 gives last_seq_scan/last_idx_scan, which
    is the best liveness signal available — but uncorrected, all 82 tables report "last read today
    04:02:37" and everything looks live. The discriminator is a read AFTER 04:02:38.
 B. WEEKLY CRONS. recompute-tiers.sh and football-meta run Mondays. On a Thursday, "not read in
    13 hours" is the EXPECTED state for their tables, not death.
 C. THE AUDIT CONTAMINATES ITSELF. topic_heat_embeddings reported a read at 16:35:30 — that was
    the audit's own SELECT. Snapshot pg_stat_user_tables BEFORE you query the candidates.
 D. TWO TABLES CAN SHARE A COLUMN NAME. enqueue_voices_on_packet (the live trigger on packets)
    names routing_tags and looks like the legacy column surviving in the hottest path. It is
    packets.routing_tags, doing real work. CONFIRM THE OWNER, NOT THE NAME.

=====================================================================================
FINDING 1 — THE STALLED METADATA FEATURE. Do this one FIRST; it needs a product decision.
=====================================================================================
It is NOT stale rot. It is DORMANT AND ARMED, and it will resume filling when the season starts.

  metadata_refresh_queue    42,782 rows   0 processed, EVER   last write 2026-06-17
  player_team_history       42,782 rows   (same count — populated by the same trigger)
  metadata_sync_log              0 rows   never written — the completion log of a job that
                                          has never once run
  event_box_scores       1,025,731 rows   last insert 2026-06-17 (FOOTBALL; NBA/NFL 2026-05-30)

The chain, measured: trg_detect_team_change is a LIVE, ENABLED trigger on event_box_scores
(tgenabled='O'). On each box-score insert detect_team_change maintains player_team_history and
enqueues into metadata_refresh_queue. The consumer side EXISTS as SQL — get_metadata_queue_batch,
get_metadata_queue_status, mark_metadata_processed — and NOTHING in Rust or Go calls any of them.
So the producer is live, the consumer was never built or was removed, and processed_at is NULL on
all 42,782 rows.

The queue stopped growing on 2026-06-17 for one reason only: that is the day box scores stopped
arriving (off-season). PHASE 4 IS PARKED WAITING FOR EXACTLY THAT SEASON. So when fixtures resume,
this trigger resumes writing into a queue with no drain.

SCOTT'S DECISION, and the audit cannot make it: is player metadata refresh a feature we want?
  (a) WANT IT  -> build the consumer (the SQL side is already there) and drain the 42,782.
                  Check first whether the backlog is still meaningful or should be truncated —
                  it was queued before the season ended and may be answering a stale question.
  (b) DON'T    -> drop the trigger FIRST, then the three tables. Dropping tables while an ENABLED
                  trigger writes to them breaks event_box_scores inserts at season start, which
                  is the worst possible time to find out.
Either way the trigger and the tables move TOGETHER, and the window is BEFORE the season starts.

=====================================================================================
FINDING 2 — THE SQL-ONLY CLASS. Eight live tables invisible to every code search we own.
=====================================================================================
This is compute_transfer_heat's category generalised, and it is the audit's most useful result.
Not one is a deletion candidate. Every one is a place where the next person greps, finds nothing,
and concludes the table is dead.

  metadata_refresh_queue  <- detect_team_change, get_metadata_queue_batch,
                             get_metadata_queue_status, mark_metadata_processed
  player_team_history     <- detect_team_change
  provider_seasons        <- resolve_provider_season_id
  rating_thresholds       <- _compute_rating_bundle
  source_tiers            <- backfill_narrative_episodes
  news_article_readings   <- collapse_exact_title_duplicates  (LIVE — runs during every ingest)
  entity_aliases          <- entity_aliases_no_update  (a trigger guard)
  source_performance      <- source_reliability_for_pair (2 Rust callers) /
                             refresh_source_performance (NO Rust or Go caller, yet the table was
                             written 2026-08-06 12:45 — find and name what drives it)

VERDICT: KEEP, and write the driver into each table's COMMENT ON TABLE. The fix is a comment, not
a migration. Do that before anything else in this session — it is cheap and it is the thing that
stops this whole class of mistake recurring.

=====================================================================================
FINDING 3 — TRUE ORPHANS. No Rust, no Go, no function, no view.
=====================================================================================
  metadata_sync_log         0 rows      never written    -> DROP (but see Finding 1: it is part
                                                            of the metadata cluster; move it with
                                                            that decision, not separately)
  season_recompute_needed   0 rows      never written    -> DROP, unambiguous
  provider_entity_map       26,210 rows last 2026-08-01  -> INVESTIGATE BEFORE DROPPING. 436,729
                                                            lifetime updates and nothing names it
                                                            now. Something stopped on Aug 1 —
                                                            find out what before deleting 26k rows.
  topic_heat_embeddings     8,924 rows  last 2026-07-25  -> DROP WITH PHASE 9 (the embedder's
                                        (15 MB)             table; Phase 9 already owns it — do
                                                            not front-run the demolition)

=====================================================================================
FINDING 4 — news_articles COLUMNS. The flip left live SQL reading dead columns.
=====================================================================================
Post-flip = the 448 articles ingested after 2026-08-06 10:55; pre-flip = the prior 7 days (56,439).

  bucket         0 of 448 post-flip  (19,862 pre-flip)  READ BY 3 LIVE FUNCTIONS:
                                                        compute_transfer_heat (3 Rust callers —
                                                        the Insider calls it PER PAIR),
                                                        refresh_typed_links (1), and
                                                        seal_narrative_threads (1).
                 Even pre-flip only 1 of 24,673 recent articles was bucket='transfer'. So three
                 live functions branch on a column that was already 99.996% NULL and is now 100%.
                 VERDICT NARROW: strip the bucket branch from all three functions FIRST, deploy,
                 measure; drop the column SECOND. NEVER the reverse — dropping first breaks all
                 three. SCOTT'S CALL 2026-08-06: batch this with the rest of the audit's SQL
                 changes rather than doing it piecemeal. That is THIS session.
  routing_tags   0 of 448 post-flip  (19,862 pre-flip)  Nothing reads news_articles.routing_tags.
                 See contamination D — packets.routing_tags is a DIFFERENT, live column.
  topic_heat     0 in the entire 7-day window (12,317 all-time)  -> dead
  duplicate_of   31 of 448 post-flip   read by collapse_exact_title_duplicates -> live, KEEP
  feed_rank      all rows              read by collapse_exact_title_duplicates -> live, KEEP
  full_text      364 of 448 post-flip  -> LIVE, KEEP

=====================================================================================
WHAT THE SWEEP DID NOT COVER. Not a clean bill of health.
=====================================================================================
 * EVERY INDEX. None were examined. Unused indexes on momentum_scores (12.7M rows) are very
   likely the single biggest storage win available and nobody has looked.
 * 106 of the 116 functions were never read individually — only grepped for table names.
 * The sweep answered "IS THIS TABLE USED". It did NOT answer "IS THIS TABLE THE RIGHT SHAPE",
   which is the other half of what Scott actually asked for. The right-shape question is where
   "the schema should support empowering the models" actually lives, and it is still open.

=====================================================================================
DELIVERABLE
=====================================================================================
 1. COMMENT ON TABLE for all 8 SQL-only tables naming their driver. Do this first.
 2. Scott's answer on the metadata feature, then execute (a) or (b) — trigger and tables together.
 3. ONE migration (number from 215; template sql/migration_template.sql; apply with sql/migrate.sh;
    then scripts/hosting/snapshot-schema.sh and COMMIT THE MIGRATION AND SNAPSHOT TOGETHER)
    carrying the settled DROPs and the bucket-branch NARROW.
 4. For each DROP, the query that proves nothing reads it, pasted into the plan Log.
 5. Then start the index pass and the right-shape question.

LAWS: describe-then-derive (T2); ONE CHANGE, ONE MEASUREMENT; contradictions preserved (T3); stage
wire names never rename; data writes rehearsed in a ROLLED-BACK transaction with the invariant
asserted inside it; deploys are explicit, and note that go/bin and rust/bin .path watchers are
DIRECTORY-scoped, so building any one binary there restarts that service.

DO NOT TOUCH: any prompt or *_PROMPT_VERSION (that is 7.11). Phase 9's demolition set
(article_reader/, scrub.rs, Role::ArticleReader, the embedder) and the 30,224 parked article_read
rows. The ingest path: Google does the relevancy, the Editor is the valve.
```

---

## Handoff — TURBOFIELDFARE / MoE-on-SSD for the voice tier (fresh context window; Scott's request 2026-08-08)

> ## ✅ CLOSED — NOT ADOPTED. **Scott's call, 2026-08-08 ~01:15 EDT: "forget we discussed the MoE."**
> **DO NOT EXECUTE THIS HANDOFF.** Nothing was cloned, built, installed or benchmarked; the Mac was
> never touched. Kept only for the measurements in it and so the question is not re-opened from
> scratch. **The reasoning that closed it, briefly, because it is the reusable part:**
> 1. **PREFILL, not decode, disqualifies it for the voice tier.** Prefill routes across ~all experts
>    (decode touches only the ~4B active slice), so it is disk-bound and **structural, not tunable**.
>    Even granting this M4 3.6× the M2's measured 27.7 tok/s, draining the queued ~3,500 `narratives`
>    + ~3,100 `vibe` costs **5.4–19 days of prefill alone**, before one output token. Today's Editor
>    baseline is ~200 tok/s prefill (derived from §7b's 87%-decode figure).
> 2. **The memory it frees is the memory it needs.** Half of decode is SSD expert reads with a WARM
>    page cache; the cache is the 14.3 GB expert file. Running it beside `ministral-3:14b` (8.8 GB
>    resident, already 3.47M pageouts) starves that cache. **So it never traded quality for
>    concurrency — it lost on both axes**, and D-T30's 1→4 slots is the competing claim on the same
>    16 GB, measured and free.
> 3. **"26B-A4B beats a 14B" was the load-bearing premise and it does not hold as stated.** Scott's
>    pushback — *experts are dense in their field* — is the right question and the answer is that
>    **learned routing is not domain routing**: it is per-token and per-layer, and published analysis
>    (Mixtral) found specialization tracking **syntax and token identity, not topic**. TurboFieldfare's
>    own slow prefill *is* that evidence — domain-specialized experts would give a sports prompt a
>    small cacheable working set. Expected quality vs a dense 14B: **roughly a wash.**
> 4. **THE POINT THAT GENERALIZES BEYOND THIS RUNTIME, and the reason this is worth keeping:**
>    every open defect on the board — `momentum` answering in markdown, `sigil`'s NBA crowns (D-T28b),
>    the `names[]` coach/manager class, `register[outrage]` reading neutral — is a **contract-adherence
>    or labeling failure, not a knowledge failure.** Density buys knowledge. **These sit on the
>    JUDGMENT axis of T2 CLARIFIED**, where gemma3:4b wrote the correct descriptor while mislabeling
>    the role, seven iterations running. **`ministral-3:3b` beating `gemma3:4b` 52/53 to 47/53 is the
>    same lesson with a number on it: the smaller model won.** Reach for post-training fit and prompt
>    contracts before parameter count.

**Read this whole section, then `route.rs`'s `VOICE_NUM_CTX` doc, then D-T29/D-T30/D-T31 below.
Do NOT read the rest of the repo.** The question is narrow: *can an SSD-streamed MoE replace or
augment `ministral-3:14b` on the Mac voice tier?*

### THE FIND

**`https://github.com/drumih/turbo-fieldfare`** — *"Gemma 4 26B-A4B inference in ~2 GB of RAM on any
M-series MacBook."* Swift 6.2 + Metal 4, macOS 26+, **Apache 2.0**. Not llama.cpp — a custom runtime.
Model is **Gemma 4 26B-A4B** (26B total / 4B active), **14.3 GB on disk**. It keeps the **1.35 GB
shared core + FP16 KV cache resident** and **streams only the experts each token needs from SSD**.
Docs worth reading in order: `docs/SYSTEM_DESIGN.md`, `docs/BENCHMARKS.md`, `docs/OPENAI_SERVER.md`
(**the integration path — an OpenAI-compatible local server**), `docs/OPTIMIZATION_JOURNEY.md`.

*(Scott found this via a video and referred to it as "turbo-fieldflare". It is **fieldfare** — the
bird. That spelling is why the first GitHub search returned nothing.)*

### HARDWARE REALITY — THE DEMO IS NOT THIS MAC

| host | decode | memory |
|---|---:|---:|
| 8 GB M2 Air, TurboFieldfare | **5.10–6.30 tok/s** | ~1.9–2.1 GB |
| **24 GB M5 Pro, TurboFieldfare** | **31–35 tok/s** | ~2.1 GB |
| 24 GB M5 Pro, **mlx-lm** (same box) | **76–82 tok/s** | 8.3–9.8 GB RSS / 14.7–15.3 GB GPU |

**The 35 tok/s Scott saw is a 24 GB M5 Pro.** This machine is **`Mac16,10`, Apple M4 base, 10-core,
16 GB, macOS 26.4, Swift 6.3.2** — it satisfies the build requirements but sits between those two
rows, much nearer the M2. **Do not carry 35 tok/s into a plan for this box.**
Baseline to beat: **`ministral-3:14b` decodes 12.3 tok/s here today at 8.8 GB resident.**

### ⚠ THE FINDING THAT PROBABLY DECIDES IT: **PREFILL, NOT DECODE**

From `BENCHMARKS.md`, 8 GB M2: a **1,017-token prompt took 36,729 ms of prefill** (TTFT 37.7 s).
That is **~27.7 tok/s PREFILL**. For comparison the 1070 Ti prefills at **~1,200 tok/s**.

**Scoracle's voice prompts are large** (measured, D-T29): `narratives` **7,574** · `vibe` **6,437** ·
`momentum` 2,535 · `sigil` 1,897 · `transfers` 1,119 · `rating` 723 tokens. Even granting the M4
several times the M2's prefill rate, a `narratives` call plausibly spends **minutes** in prefill
alone. **The voice tier is prompt-heavy and queue-deep (~3,500 `narratives` + ~3,100 `vibe`
pending), which is the worst possible shape for this runtime.** Decode is only 87% of model time
when prefill is cheap (§7b) — that assumption does not survive here.

**FIRST THING THE NEXT SESSION SHOULD MEASURE: prefill tok/s on THIS M4 at a realistic 3,000-token
prompt.** If it lands near the M2's ~28 tok/s, the voice tier is disqualified and the rest is moot.

### THE OTHER NUMBER THAT MATTERS

On the **same** M5 Pro, **mlx-lm ran 2.4× faster (76–82 vs 31–35 tok/s) using ~7× more memory.**
**TurboFieldfare buys MEMORY, not SPEED.** So the honest framing is: it is for boxes that cannot
hold the model, and this box already holds a 14B comfortably. Half of `decode` is SSD (`expert
reads 83.1 ms` of a 162.8 ms step), so **concurrent streams contend for SSD** — expect worse than
linear degradation. That directly undercuts the "1.35 GB resident so we can run more channels" plan:
**the freed memory is wanted by the page cache that makes expert reads fast.**

### WHERE IT COULD STILL WIN — the honest case for testing it anyway

A **26B model beats a 14B on quality**, and low-volume/high-value junctions have SHORT prompts:
**`sigil` 1,897 tok · `rating` 723 tok.** The Oracle/Sigil seat is exactly where a smarter, slower
model earns its keep — and `sigil` has an open, undiagnosed defect (**D-T28b**, NBA-team crowns
failing ~86%). **Scope the experiment to the Oracle, not the voice tier.**

### DO NOT DO THESE

* **Do not install or benchmark before Saturday 2026-08-08 10:55 EDT** — 8.7's watch closes then and
  already carries three confounds.
* **Do not point production at it** until prefill is measured on this box.
* **Note `llama.cpp` issue #19825 — "Managed SSD offloading for MoE to prevent macOS kernel
  panics."** Different runtime, same technique class. This is Scott's working Mac AND the voice
  host; a panic takes production with it.
* The Mac is **already paging (3.47M pageouts, ~14 GB)** at 16 GB with the 14B resident.

### SUGGESTED ORDER

1. Clone + `swift build -c release` (build only — harmless, no production contact).
2. **Measure prefill and decode on this M4** at 700 / 1,900 / 3,000 / 7,500-token prompts.
   Compare against `ministral-3:14b`'s 12.3 tok/s decode and the 1070's ~1,200 tok/s prefill.
3. Only if prefill survives: stand up `docs/OPENAI_SERVER.md` and A/B **the Oracle** through
   `COGNITION_ROUTE_ORACLE_LOGIC_CANDIDATE` + `eval --task oracle` — the same discipline that gave
   D-T31 its 52/53 (**adoption is a human editing the route, never an auto-promote**).
4. Watch memory pressure and `pageouts` throughout; abort on any kernel instability.

### SESSION STATE THIS HANDOFF INHERITS

`RAIL=packet` live. **Two changes STAGED AND NOT DEPLOYED, both held until after 8.7 closes:**
**D-T29** (`ARTICLE_NUM_CTX`/`EDITOR_NUM_CTX` 8192→4096, committed, 400 tests pass) and **D-T31**
(Editor → `ministral-3:3b`, which scored **52/53 vs gemma3:4b's 47/53**). **ORDER IS NOT OPTIONAL:
deploy the 4096 binary FIRST, then switch the model** — ministral-3:3b at 8192 is ~7.65 GB on an
8 GB card and spills. Saturday: 8.7 at ~10:55 → `rail-cutover-check.sh` (no `DAY`) = **8.2 day 1**
→ `rail-6.7-bands.sh` after 22:10.

---

## Handoff — D-T29 + D-T31, THE SATURDAY DEPLOY (fresh context window; written 2026-08-08 ~01:30 EDT)

> ## ⛔ EXECUTED AND CLOSED 2026-08-08 16:10 EDT. **DO NOT RUN THIS FENCE.** The current handoff is
> **"THE 22:10 BANDS + THE CAP RULING"**, immediately below it.
>
> **Steps 1, 2 done. Step 3 HELD by Scott. Step 4 partial. Step 5 NOT run — still ~6 h out.**
> Two of its premises were measured FALSE during execution and are corrected in §D-T29 and §D-T30:
> the Editor was **not** already at 4096 (the runner had reloaded to 8192/5.3 GB ~13 h earlier, so
> `ollama ps` CAN verify and the deploy was a real window change), and there is **no archbox mirror**
> of D-T30 (the client sends 4 locally; `OLLAMA_MAX_CONCURRENT` is inert). Its step 1 returned a
> structural FAIL — **D-T32**.

*This is the CURRENT handoff. The TurboFieldfare fence above it is CLOSED and must not be executed.*

```
Work the SATURDAY DEPLOY in scoracle-backend (Scoracle, /Users/scotty/scoracle/scoracle-backend).
Two staged changes go out, IN ORDER, and then get measured. Nothing else.

READ FIRST: PLAN-one-rail.md STATE block (top) + §0 working rules, then PLAN-character-tuning.md
§D-T29 and §D-T31 including the "ALREADY RUNNING AT 4096 BY ACCIDENT" subsection. Do NOT read the
rest of the repo. The TurboFieldfare handoff is CLOSED — skip it.

TIME GATE. 8.7's watch closes ~10:55 EDT. Nothing deploys before that. Check the clock first.

STATE. RAIL=packet live. Deployed binary 6fbf798 (built 2026-08-06 19:32Z). Tree clean at 6f306f9.
Both changes committed, neither deployed. ministral-3:3b already pulled on archbox (3.0 GB).
D-T21's cap is ARMED (EDITOR_MAX_READS_PER_ENTITY_DAY=10, verified 2026-08-08).

ORDER — NOT OPTIONAL:
  1. 8.7 closes ~10:55 -> scripts/rail-cutover-check.sh with NO DAY override (= 8.2 day 1).
  2. DEPLOY the D-T29 4096 binary. This trips the .path watcher and restarts scoracle-cognition;
     that is intended here and only here.
  3. THEN edit COGNITION_ROUTE_EDITOR=gemma3:4b -> ministral-3:3b in archbox .env.local, restart.
  4. Confirm resident ~6.0 GB and gemma3:4b evicted (MAX_LOADED_MODELS=1).
  5. scripts/rail-6.7-bands.sh only AFTER 22:10 EDT. Never close phase 6 on an INTERIM banner.
Flipping the model while the deployed binary still asks 8192 puts ministral at ~7.65 GB on an 8 GB
card and spills it to CPU. That is the one mistake that breaks production.

TWO THINGS ALREADY KNOWN THAT WILL MISLEAD YOU IF YOU FORGET THEM:
  * THE DEPLOY WILL SHOW NO SPEEDUP. The Editor is already running at 4096 because
    OLLAMA_KEEP_ALIVE=-1 pinned the runner the Aug 7 eval loaded from target/debug. FLAT IS THE
    EXPECTED RESULT. Do not go hunting for a second knob to explain it. The deploy is still
    required: any reload restores 8192, and under ministral that is the spill.
  * `ollama ps` CANNOT VERIFY THE DEPLOY — it reads 4096 either side. Verify from the journal:
    `scoracle-cognition starting ... built=` must postdate d4c80a0 (2026-08-07 10:03 EDT).

WHAT TO MEASURE AFTER THE SWAP — THE TAG DISTRIBUTION, NOT THE GATE SCORE. story_type and register
differ between the two models on fixtures that BOTH PASSED, and routing_tags derives from them, so
the swap can shift which voices wake with the gate registering nothing. Before-picture (D-T22 pass
3): fixture 4,693 / charged 2,237 / roster 2,203 / transfer 1,831 / performance 1,737 / general
1,655 / injury 349. WATCH `injury` HARDEST — nothing subscribes to it (D-T25), so a change is silent.
Also observe real Editor eval_count: the speed question is OPEN (decode +13%, tokenizer 32% denser
= net wash). Do not promise a speed win.

DO NOT:
  * Do not deploy before 10:55. Do not run rail-6.7-bands.sh before 22:10.
  * Do not change OLLAMA_MAX_CONCURRENT on archbox this session (see below) — one change, one
    measurement, and the swap owns this window.
  * Do not touch any prompt or *_PROMPT_VERSION (that is 7.11 — a bump is a cache key and reopens
    ALL that stage's work fleet-wide).
  * Do not raise ARTICLE_MAX_MODEL_CHARS (9_000). It is now load-bearing for the 4096 window under
    ministral's denser tokenizer — headroom is 12%, not 28%.

NEXT AFTER THIS, ALREADY QUEUED (do not start them here):
  * D-T30 — Mac OLLAMA_NUM_PARALLEL 1 -> 2, measure, then consider 4. Largest throughput change
    available; 4 slots already fit at 9.74 GB.
  * The archbox mirror of it — server NUM_PARALLEL=4 while the client's OLLAMA_MAX_CONCURRENT=1,
    on the Editor, the stage running at ~96% of ingest with no headroom (§0a). Settle the
    disagreement with D-T19's handoff (which recorded 4) before changing it.
  * The VOICE session proper: D-T23 -> D-T24 -> D-T25, logging schema observations to Appendix S.

LAWS: describe-then-derive (T2 — the axis is OBSERVATION vs JUDGMENT, see PLAN-one-rail §0);
ONE CHANGE, ONE MEASUREMENT; STOP on surprise and write it down rather than improvising; build to
target/debug for tests, never rust/bin (except step 2); DB access from archbox, not the Mac.

FINISH RITUAL: fill in the measured numbers, update BOTH files in the same commit (one line in
PLAN-one-rail Appendix D, the detail in PLAN-character-tuning), update the STATE block, commit, and
print the next handoff last.
```

---

## Handoff — THE 22:10 BANDS + THE CAP RULING (fresh context window; written 2026-08-08 ~16:10 EDT)

*This is the CURRENT handoff. Both fences above are CLOSED and must not be executed.*

```
Work scoracle-backend (Scoracle, /Users/scotty/scoracle/scoracle-backend). Saturday's deploy is
DONE; what is left is one gated measurement and one ruling. Nothing else.

READ FIRST: PLAN-one-rail.md STATE block (top) + §0 working rules, then PLAN-character-tuning.md
§D-T32. Do NOT read the rest of the repo. Both older handoff fences are CLOSED — skip them.

CHECK THE CLOCK FIRST. This block was written 16:10 EDT Sat 2026-08-08 and its one timed item is
22:10 THAT NIGHT. If you are reading this on a later date, that window has PASSED: run the bands
for the CURRENT day and say so, do not pretend to close a window you were not present for.

STATE. RAIL=packet live. DEPLOYED 39db36ee9d45 (built 2026-08-08T20:01:56Z, live 16:04:18 EDT) =
D-T29's 4096 binary. Tree clean, both plan files committed together. D-T31 is HELD, NOT flipped —
the Editor is still gemma3:4b. D-T21's cap stays ARMED at 10/day by Scott's ruling.

DO THIS:
  1. AFTER 22:10 EDT ONLY: scripts/rail-6.7-bands.sh. Never close phase 6 on an INTERIM banner.
     This is the last thing phase 6 is waiting on.
  2. At/after the 02:00 drain: confirm the 4096 window actually landed. `ollama ps` should read
     gemma3:4b CONTEXT 4096 at ~4.99 GB, DOWN from the 5.3 GB @ 8192 measured at deploy time.
     `ollama ps` IS trustworthy again (the old "blind verify" note died with the accident).
     Wall-clock is expected FLAT — num_ctx is memory, not per-token compute. Do not hunt a knob.
  3. Re-run scripts/rail-cutover-check.sh for 8.2 day 2 and expect clause 1 to FAIL again at ~19%.
     That is D-T32 and it is EXPECTED. Do not roll the rail back for it.

THE ONE THING THAT NEEDS A DECISION (D-T32) — do not improvise it, it is Scott's:
  D-T21's cap withholds at ENQUEUE; §2 clause 1 counts ARRIVALS. While the cap is 10/day, clause 1
  is unreachable and 8.2's 7-day window can NEVER start. The Editor is not broken — it read 921 of
  the 921 it was asked for (100%), and read+withheld=arrivals EXACTLY on two separate days. Three
  candidates, none chosen: redefine clause 1 against the QUEUE / reshape the cap / accept the
  coverage as a product decision. Bring Scott the choice, not a fix.

BEFORE D-T31 CAN EVER BE FLIPPED: re-bank the tag before-picture POST-CAP. The banked one (fixture
4,693 / charged 2,237 / roster 2,203 / transfer 1,831 / performance 1,737 / general 1,655 / injury
349) is PRE-CAP and the sample is now 81% smaller, so a swap measured against it proves nothing.
WATCH injury HARDEST — nothing subscribes to it (D-T25), so a change there is silent.

DO NOT:
  * Do not run rail-6.7-bands.sh before 22:10.
  * Do not flip COGNITION_ROUTE_EDITOR. It is HELD by decision, not by oversight.
  * Do not change the cap, or any second knob, without Scott — one change, one measurement.
  * Do not touch any prompt or *_PROMPT_VERSION (that is 7.11 — a bump is a cache key and reopens
    ALL that stage's work fleet-wide).
  * Do not raise ARTICLE_MAX_MODEL_CHARS (9_000) — load-bearing for the 4096 window.
  * Do not chase "the archbox mirror of D-T30". It was measured FALSE and struck; the client
    already sends 4 locally. OLLAMA_MAX_CONCURRENT is inert for this client.

STILL QUEUED (do not start them here):
  * D-T30 — Mac OLLAMA_NUM_PARALLEL 1 -> 2, measure, then consider 4. Largest throughput change
    available; 4 slots already fit at 9.74 GB. Unaffected by tonight.
  * The VOICE session proper: D-T23 -> D-T24 -> D-T25, logging schema observations to Appendix S.
  * Two live dead-letter streams still burning voice capacity hourly (D-T28: momentum answers in
    markdown; sigil crown parse). narratives now shows the same markdown failure — worth a look.

LAWS: describe-then-derive (T2 — the axis is OBSERVATION vs JUDGMENT, see PLAN-one-rail §0);
ONE CHANGE, ONE MEASUREMENT; STOP on surprise and write it down rather than improvising; build to
target/debug, never rust/bin (no deploy is authorised in this block); DB access from archbox, not
the Mac. DEPLOY NOTE if one is ever authorised: a plain `cp` over the running binary fails ETXTBSY —
stage outside rust/bin/ and `mv` in, which is also what keeps the .path watcher to one trigger.

FINISH RITUAL: fill in the measured numbers, update BOTH files in the same commit (one line in
PLAN-one-rail Appendix D, the detail in PLAN-character-tuning), update the STATE block, commit, and
print the next handoff last.
```

---

## 0 · THE REGISTER — every friction point, roadblock and concern going into the session

*Assembled 2026-08-06 ~13:15 EDT, after 8.8, on Scott's instruction: "I want all the friction
points, the roadblocks, the concerns listed." Everything here is MEASURED on live post-flip
production, not inferred. Sections below carry the detail; this is the one place that lists all of
it. **Ordered by what would hurt most if ignored, not by how easy it is to fix.***

### 0a · The one that governs everything else: the model layer is the throughput ceiling

**Measured today.** The Editor sustains **430–490 reads/hour while the daemon is up**, and the
daemon is deliberately down **8 hours a day** (harness rest windows at 00/03/06/09/12/15/18/21:00,
+1h each — §0 rule 6 of `PLAN-one-rail.md`; the 12:00 window is visible in the journal as a clean
stop at 12:02 and restart at 13:00). That works out to:

| | |
|---|---|
| Editor reads/day, actual | Aug 3 **7,041** · Aug 4 **7,560** · Aug 5 **8,063** |
| Articles arriving/day | Aug 3 **6,960** · Aug 4 **8,027** · Aug 5 **8,401** |
| Editor backlog right now | **4,682 pending**, oldest stamped **Aug 5 02:01** (~35h latency) |

**The Editor runs at parity with ingest — about 96% — which means it never catches up.** It is not
falling behind, but it has no headroom to burn down a backlog, absorb a re-read, or take a prompt
that costs 20% more. Every knob in this file that makes a model call longer spends from this
budget. **This is the number to protect.** Coverage reads 97.3% on a complete day precisely
*because* throughput ≈ inflow; those are the same fact seen twice.

**And the Editor is not the deepest queue.** Full pending state at 13:07 EDT:

| stage | pending | oldest pending | note |
|---|---|---|---|
| `investigate_entity` | **8,624** | Aug 4 04:07 (**~57h**) | the deepest starvation on the rail; D-T10 |
| `editor` | 4,682 | Aug 5 02:01 (~35h) | at parity, permanent backlog |
| `narratives` | 2,059 | **Aug 6 10:55:52** | the flip's trigger burst, undrained 2h later |
| `vibe` | 1,912 | **Aug 6 10:55:52** | same burst, same story |
| `momentum` | 1,269 | Aug 2 04:38 | includes the failures in 0c |
| `sigil` | 474 | Aug 3 10:43 | |
| `peak` | 237 | Aug 4 03:00 | |
| `transfers` | 136 | Aug 6 11:23 | the Insider, post-gate-removal |
| `fixture_boxscore` | 72 | Aug 4 04:02 | Phase 4 is parked |
| `article_read` | 30,222 | Aug 3 02:01 | **PARKED BY DESIGN** — rollback surface, 0 compute, dies in Phase 9 |

**The structural tension to name out loud, because it is the thing tuning must actually resolve:**
six AI layers enriching every article is the product, and it is also the cost. The Mac holds
**3 concurrent permits for 6 voices on one GPU**. Adding quality by adding a layer, a re-read, or a
longer prompt is not free here — it comes directly out of coverage. **"Empower the models" and
"read everything" are in tension, and the session should decide which one wins where, per junction,
rather than letting the queue decide by starving whichever stage sorts last.**

### 0b · Defects — fix these, do not tune them

- ~~**`editor::write_links` loses an article's ENTIRE link set on a duplicate resolve.**~~
  **FIXED AND REPAIRED 2026-08-06 @ `6c67a68` — PLAN-one-rail 8.10.** `DISTINCT ON (entity_type,
  entity_id)`, the idiom `storyline.rs`'s sibling write already carried. **The real blast radius
  was 18× the journal's 5:** the DATA said **9 of 754 post-flip reads (1.2%), 47 links** — 2 lost
  every vetted row and 7 kept stale pre-flip legacy rows, so they looked adjudicated while the
  Editor's verdict was gone. All 9 repaired from `editor_reads.resolved` (never by re-reading —
  a re-fetch could overwrite the very read being recovered). **Lesson worth carrying into the
  session: a WARN that says "continuing" hides its own frequency, and `journalctl` retention set
  the number everyone believed. Measure blast radius from the data.**
- **`momentum` answers in markdown instead of its contract.** 11 pending failures + 7
  dead-lettered, `momentum: invalid response (raw="**Momentum Read: …")`, spanning **Aug 2 →
  Aug 6 11:38**. The voice is writing a beautiful essay into a field that wants a structure. First
  candidate: it is the contract prompt, not the model, since the same model answers five other
  voices correctly.

### 0c · Instrument problems — you cannot tune against a gauge that moves

- ~~**D-T19 (§6a): the editor fixture gate is not deterministic**~~ — **SETTLED 2026-08-06. The
  instrument was GPU CONTENTION, and the fix is a method rule, not code: the gate is valid only
  with `scoracle-cognition` STOPPED.** Ten runs: daemon stopped → **47/53 ×5 with a single
  identical output hash**; daemon running → 47,47,47,47,48 and **five different hashes**. Greedy
  decode is not deterministic on a busy GPU (batching moves the floating-point reduction order),
  and a `seed` would not have helped — at temp 0 the RNG is never consulted. **The stable baseline
  is 47/53, REQUIRED 32/33.** Two further traps recorded in §6a and now guarded: the summary line
  can hold still while checks flip underneath it (diff the per-check table, never the score), and
  an unparseable fixture used to shrink the denominator silently (fixed + unit-tested).
- **§2's clause 3 link sample is emitted and UNSCORED.** Precision on the rail's links has never
  actually been measured — only sampled. The 0.90 Editor links are now the majority producer
  (0b, 0e), so this is scoring the new rail, not the old one.
- **Clause 4b is FAIL (43–47/53) and Scott waived it explicitly** for the flip (D-T19). The waiver
  is logged so it is auditable; it has not been retired.

### 0d · Architecture concerns — the hand-rolled complexity that SURVIVED 8.8

**This is the honest answer to "did we just take the fastest path and port a bunch of old Go?"**
The relevance regex is gone (0e). What remains, and it is the largest hand-rolled judgment left
anywhere on the rail:

- ~~**~350 lines in `news.go` decide WHAT WE ASK GOOGLE**~~ — **CLOSED 2026-08-06 by
  PLAN-one-rail step 8.9, on Scott's call, hours after this register was written.** The alias
  scoring, the 18-word risky-club list, the four trusted literals, the short-alias allowlist, the
  per-term suffix branching and the lane cap are all deleted (393 net lines). What runs now: one
  query per name we know the entity by, sport term on every lane, every lane runs, cap on results.
  **Two things survived deletion because measurement said to keep them, and the session should not
  re-open either without new numbers:** the sport suffix (bare "Nice" returns the NHS institute and
  Formula 1; "Nice soccer football" returns the club) and alias lanes (20–30% marginal unique
  recall — Spurs 18/47, Barça 29/102, PSG 16/47). Cost: ~44% more Google calls per sweep.
- ~~**`title_pos`'s only remaining readers are SQL functions**~~ / ~~the legacy link schema~~ —
  **CLOSED 2026-08-06 by 8.11.** `news_article_entities` is five columns and one writer; `vetted`,
  `match_confidence`, `scrubbed_at` and `title_pos` are dropped, and with them the three
  reconciliation arms that made 8.10's bug possible. **The find that justified the whole rip:
  `compute_transfer_heat` is LIVE (the Insider calls it per pair) and still carried the mig-033
  proximity gate 8.8 removed from Rust — so the gate was half-alive in SQL for six hours after we
  believed it deleted. Check both sides of a gate: SQL functions are code too.**
- **`fetch.rs::clean_html` is a naive strip-all-tags** (§1, D-T11) — nav menus and footers reach
  the prompt, and **34.3% of editor prompts hit the 9,000-char truncation cap.** Hand-rolled
  extraction is spending the model's window on page furniture.
- **`title_pos`'s only remaining readers are SQL functions** (`refresh_co_mention_links` and
  friends). Phase 9 owns them; noted here so nobody re-adds a writer.

### 0e · What is genuinely settled — do NOT re-litigate these in the session

Verified 2026-08-06 after 8.8, with numbers, so the session starts from fact rather than memory:

- **The relevance regex is gone.** The entire Go tree contains three `MustCompile` calls, all three
  RSS *parsers* (`<[^>]+>`, the entity decoder, whitespace). **The Rust tree contains no regex at
  all — the `regex` crate is not even a dependency.**
- **Google is the relevance source.** The primary link is the query hypothesis at 0.95; ingest
  applies no relevance filter; the funnel's only drops are window, dedup and limit, and it
  balances (residual 0 on a live sweep).
- **The Editor is the safety valve, and it fires.** **78 of 507 reads since the flip returned
  `irrelevant` (15.4%)**, and an irrelevant read retracts every vetted row for that article.
- **The Editor replaced the regex as a link GENERATOR and beat it.** Since the flip: **645 player
  links + 87 team + 5 person at 0.90, from 507 reads = 1.27 player links per read**, against the
  deleted regex loop's 3,589 player links per ~8,400 articles = **0.43 per article**. Reading the
  body finds roughly **3× the players** that substring-matching a headline did.
- **The resolver is not fuzzy matching wearing a new coat.** `editor::derive::resolve_names` is an
  EXACT match on `public.nrm()`-normalized surfaces in `entity_name_surfaces`, sport-scoped,
  kind-gated by the model's own `kind_hint` + `descriptor`; two candidates **refuse** rather than
  coin-flip; zero candidates go to the Investigator as discovery. That is describe-then-derive (T2)
  working as designed, and it is why deleting the regex cost nothing.

### 0f · Open decisions the session inherits (detail in the numbered sections)

| id | question | where |
|---|---|---|
| ~~D-T19~~ | ~~stabilize the fixture gate before scoring anything~~ — **CLOSED 2026-08-06: stop the daemon; baseline 47/53** | §6a |
| D-T20 | knob (a) DONE (proximity clause deleted @ `28fcf45`); does `entity_roles` replace it? | §7a |
| D-T18 | syndication doubles facts in a packet — never dedupe across sources (T3) | §6b |
| D-T11/12 | Editor input hygiene + output dominance | §1, §2 |
| D-T10 | the Investigator's starvation — now 8,624 deep | §3 |
| D-T6/7/8 | Investigator evidence-class gaps | §4 |
| D-T9 | parked ops — **ONLY on Scott's go** | §4 |
| 7.11/7.15 | the voice diet + its eval dry-run — one re-earn event, one fleet-wide regen | §7b |
| **D-T21** | **cap the reader at 5 articles per entity** — Scott, 2026-08-06: buy the Investigator and the graph real headroom out of the Editor's parity budget (§0a) | §8 |
| **D-T22** | **the schema audit** — Scott, 2026-08-06: the code is being contorted to fit a schema built for the pre-Google, pre-AI pipeline; update the SQL to the new approach instead | §8 |

**Watch while tuning:** `transfer_rumors` **70/24h** against a **68/24h** pre-flip baseline (the
proximity gate came out at 11:38 today — if pair volume climbs, that is the Insider eating the
Mac's permits and D-T20 knob (b) is the answer).

---

## 1 · The Editor — input hygiene (D-T11; measured 2026-08-04/05, 4,774 ledgered calls)

**Finding:** `fetch.rs::clean_html` (fetch.rs:261) is a naive strip-all-tags — it keeps every
visible string on the page: nav menus, footers, related-link rails. **34.3% of all editor
prompts hit the 9,000-char truncation cap** (`EDITOR_MAX_MODEL_CHARS`, editor/prompt.rs:96).
`sports.yahoo.com` — the #1 domain, 584 calls in the sample window — runs **95% at cap**, and
its "Article text:" begins with the site's entire chrome ("News Today's news US Politics …
Horoscopes Shopping Food Travel Autos …") before any article prose. On capped pages the chrome
eats the front of the window and **real article text is truncated off the tail** — feeding
D-T1's under-fill miss class (late-article names the model never sees). Capped domains beyond
Yahoo: si.com 77%, nytimes.com 65%, nbcsports.com 67%, cbssports.com 46%.

Bonus defect: `decode_entities` (fetch.rs:328) handles `&#39;` but not hex `&#x27;` (or numeric
entities generally) — quote-heavy articles carry six wasted chars per apostrophe inside the
budget, and the model reads `&#x27;` as literal text.

**Candidate knobs** (quality-first; modest wall-time win):
- (a) Main-content extraction before truncation: prefer the `<article>` element when present;
  else strip `<nav>/<header>/<footer>/<aside>` blocks — the existing `strip_element_blocks`
  machinery extends to this in-idiom. No contract change.
- (b) Decode numeric/hex entities in `decode_entities`.

**Measurement:** re-run the D-T1 per-name 2×2 on a capped-domain sample (Yahoo/SI) before/after;
watch pct-at-cap and extracted_words shift; the 5.7-style fixture set should include a captured
Yahoo page (real prompts are in `cognition_ledger.built_prompt`).

## 2 · The Editor — output dominance and capacity (D-T12; same sample)

**Finding: output generation, not prompt eval, is the wall.** Wall by prompt-size bucket:
~1.4k chars → 16.9s avg / 195 out-tok; ~5k → 30.6s / 387; capped ~8.9k → 38.8s / 476. Of the
~22s small→capped delta, ~19s is the extra 281 output tokens (~14 tok/s/slot at 4-parallel on
the 1070 Ti) and only ~3s is prompt eval. Longer input → more names/facts/evidence emitted →
generation time.

**Capacity is fully subscribed:** ~490 reads/hr active, rest windows pause 8h/day (every 3rd
hour +1h) → ~7,800 reads/day capacity vs arrivals grown to ~8,000–8,400/day (Aug-4: 7,985;
Aug-5: 8,358; ingest is one daily 02:00 EDT batch). Slot utilization measured 77% model-call
wall; concurrency verified real at both layers (worker `ARCHBOX_GEMMA_SLOTS`=4,
`OLLAMA_NUM_PARALLEL=4`, 100% GPU). Within-24h coverage still 100.0% post-deploy — but there is
no headroom left for arrival growth.

**Candidate knobs, by leverage:**
- Output-side (the real seconds): D-T4's `num_predict` 900→750 clips only the p95 tail (avg
  output 420). A real cut means a tighter ep1 envelope (bounded `key_facts[]`, shorter
  `evidence_blurb`) — **an ep2 contract bump that reopens all editor work; never casual.**
- Input-side: D-T11 above (secondary for time, primary for quality).
- Rest windows: 8h/day of wall (33%) — hardware-stress policy, **Scott's call only**.
- Model/quant swap on the same card: a Character decision, needs the D-T1 yardstick replayed.

## 3 · The Investigator — starvation and volume (D-T10; day-2 verdict 2026-08-05)

**The design works but the arithmetic doesn't:** the investigator caught exactly the idle the
design predicted — 70 runs in the 01:52–02:00 EDT window before the daily batch re-buried the
card (Aug-5). Decisions honest: 8 accepted / 23 ambiguous / 20 not_sport / 19 insufficient
(11.4% acceptance). But steady-state nominations are ~3k persons/day (day-2 pace matched
day-1 — NOT a corpus flush), so the queue grows ~2.7k/day against a ~70/day drain (6,670
pending at day 2). Even fully unblocked, the 4.2 budget (2s Wikimedia spacing) caps drain at
~900/day.

**Candidate knobs, in leverage order** (from the D-T10 ledger entry): (a) the v1 investigator
makes ZERO model calls — holding an `ARCHBOX_GEMMA_SLOTS` card slot for pure HTTP work is the
structural mismatch; a separate slot group frees it entirely; (b) tighten the 5.2 enqueue rule
(descriptor-on-first-sight admits ~100% of person names; the 2-mention floor is near-dead
letter); (c) run the investigator through GPU rest windows (the card rests; HTTP doesn't need
it) — interacts with the pause-timer design; (d) raise `max_in_flight` only after (a).

✅ **(a) SHIPPED 2026-08-09** (the ep6 session, on Scott's "we move on to the Investigator"):
`slot_group()` → `None` in `entity.rs` — the stage no longer queues behind the Editor for a card
it never touches, and it keeps running through the Editor's drain. `max_in_flight` stays 1 ON
PURPOSE: the binding constraint is now the polite 2s Wikimedia spacing (~900/day ceiling), so
(d) needs a faster evidence source, not a bigger slot count. The route pin means membership
stays wrong even when 5.4's prose arm lands — `Role::Investigator` is the 14B on the OTHER
host. Watch the drain rate against the ~70/day starvation baseline.

**The compounding upside already measured:** the 8 overnight accepts collected 102 resolver
links onto `persons` rows within the same day (Xabi Alonso 59, Andoni Iraola 23) — every
accepted person immediately stops being an unresolved name. Drain rate is the direct multiplier
on this loop.

## 4 · The Investigator — evidence-class gaps (D-T6/7/8) and parked ops (D-T9)

- **D-T6** enrichment refusals leave no durable trace (log-only) — review surface can't count
  them. Candidate: census row or `players.meta` note on refusal.
- **D-T7** initials in `nrm()` ("A.J. Green" → `a j green` vs Wikidata `aj green`) — honest
  refusal, missed enrichment. Measure the class size across rosters before touching the one
  normalizer (mig-198 caution doubly applies).
- **D-T8** legal-name vs known-name ("Airious" vs "Ace" Bailey) — the designed answer is the
  deferred 5.4 prose arm: Wikipedia REST search + gemma **describes** the page, code decides.
  Build when the class proves big enough; this is also the first real model-call load for
  `Role::Investigator` (interacts with D-T10's slot question).
- **D-T9** the meta-gathering RUN (FULL NBA seed ~603 players → 20-row hand-check → widen to
  FOOTBALL rosters at season start) — **ops on Scott's go**, machinery ready. Box-score target
  URLs themselves stay parked with Phase 4 until a season provides them (pulselive_pl seed
  one-liner still awaits Scott).

## 5 · Older ledger items carried (see Appendix D for baselines)

- **D-T1** names[] under-fill — the miss class (16.7% of successful name-reads missed the
  player in the replay). Knobs: quoted-people re-scan for title principals; the Investigator
  nomination backstop now live structurally catches what the prompt drops. D-T11's truncation
  fix plausibly shrinks this class — measure them together.
- **D-T2** register `outrage` reads neutral under phrase-first order (declined reorder; needs a
  fixture set before revisiting).
- **D-T3** parse_failed 2.6% vs legacy 0.1% — diagnose format_schema violations vs truncation
  (interplay with D-T4/num_predict).
- **D-T4** editor call cost / num_predict 900→750 — superseded in part by §2's decomposition:
  the knob only clips the tail.
- **D-T5** descriptor leakage ("team 277" — an internal id in a text-copy field). Count
  instances before caring.

---

## 6 · Found during the Phase 8 build (2026-08-06; the first shadow compile + the first §2 reading)

Both are RECORDED, NOT FIXED, per Scott's ruling that session ("no tuning as we go — we'll tune
the weekend"). Both have D-numbers in Appendix D; the diagnosis is here.

### 6a · D-T19 — the editor fixture gate — **SETTLED 2026-08-06. THE INSTRUMENT WAS CONTENTION.**

**RESOLVED. The gate is deterministic with the cognition daemon stopped, and only then. No model-
path code changed; the fix is a METHOD RULE.** The rule, which is now also printed by the gate on
every run and carried in `run_fixtures`' doc comment:

```
systemctl --user stop scoracle-cognition
cd rust && cargo build --bin eval && ./target/debug/eval --task editor --fixtures
systemctl --user start scoracle-cognition
```

**The experiment (archbox, 2026-08-06, ten runs — same binary, same fixtures, same `gemma3:4b`,
every fixture pinned `"temperature": 0.0`):**

| daemon | scores | model output over the 5 runs | wall/run |
|---|---|---|---|
| **STOPPED** | **47/53 ×5** | **ONE hash — all 53 checks identical, every run** | **96s** |
| running | 47, 47, 47, 47, 48 | **FIVE hashes — every run differed** | ~290s |

**Why.** Nothing inside the eval is concurrent — the fixture loop is sequential (`eval.rs:332`) and
the Router is built with one permit — but **the SERVER is**. `scoracle-cognition` drains the editor
stage against the same archbox Ollama at `OLLAMA_NUM_PARALLEL=4`, so the gate's calls are batched
alongside live traffic; batching changes the floating-point reduction order, and a changed reduction
order moves the argmax on near-ties. **Greedy decode is not deterministic on a busy GPU.** Under
load the `fan-protest-register-outrage` fixture emitted **2 names on one run and 5 on the next** off
a byte-identical prompt. Load was measured **from the data, not the journal** (0b's lesson): the
live Editor completed **5–12 reads per minute** in `editor_reads` throughout every noisy run.

**Both listed dead leads are dead, as briefed, and one candidate fix was rejected on the mechanism:**
temp=0 is genuinely pinned in all 12 fixtures; the loop is genuinely sequential; and **a `seed` was
NOT added to `GenerateOptions`.** At temperature 0 the sampler is greedy and never consults the RNG
— the divergence is upstream of sampling, in the kernels — so a seed would have pinned nothing while
looking like a fix. The only lever that pins this is an idle server.

**The trap this leaves behind, and the reason the rule is worth its own paragraph: the summary line
hides its own movement.** Under load the tally read 47/53 four times running while two checks on ONE
fixture flipped in OPPOSITE directions and cancelled — `name_found[Moyes]` and `name_absent[Gwladys]`
are the same coin, because a longer `names[]` catches the manager and the stand together. **A stable
total is not a stable gate. When comparing two runs, diff the per-check table, never the score.**

**The denominator, reconciled (it was asked for, and it was hiding something).** The 12 files declare
**60** expect-keys; the gate scores **53**. The gap is exactly the keys that are prompt/resolver
INPUTS rather than assertions — 12 × `reader_vetted` and 6 × `resolver_surfaces` — after which the
remaining 42 keys expand list-wise to 53 checks. **But `expected_property_count` (the count used when
a reply is UNPARSEABLE) knew every voice axis and NOT ONE of the Editor's**, so an unparseable editor
fixture contributed `0/0` and *vanished from the tally* instead of scoring `0/N`. The gate would have
printed a quietly smaller denominator with no warning — `47/53` and `47/46` read as the same kind of
success, and the second is a fixture that died. **Fixed**, with the editor, graph and three stray
voice axes added, and a unit test (`editor_fixture_denominator_is_derivable_from_the_files`) that
walks the real fixture dir and pins 12 fixtures / 53 checks. The denominator now moves when the FILES
move and never when a reply does. *(It never fired in the ten runs above — both observed scores were
out of a true 53 — so it explains none of the historical spread. It was a loaded gun, not the shot.)*

**THE STABLE BASELINE, and it is the number every editor knob is scored against from now on:**

> **47/53 — REQUIRED 32/33, WAIVED 15/20** (the 3.7 re-scoped bar, PLAN-one-rail §Phase 3 Log).
> Six reds, identical in all five quiet runs:
> `name_absent[Gwladys]` **(the only RED on a REQUIRED axis)**, `register[outrage]`,
> `name_found[Arteta]`, `name_found[Rangers]`, `name_kind[Rangers=club]`, `name_found[Bellingham]`.

**Do not read 47 as a regression from iteration 13's `48/53 ×2, 33/33 required` (2026-08-01).** That
reading's daemon state was never recorded, and this fixture is precisely the one that flaps, so the
two are not comparable — 48/53 (Moyes ✓ *and* Gwladys ✓) is simply the third face of the same coin.
**47/53 quiet is the first baseline taken on a still instrument; it is the reference, and the earlier
numbers in this file are superseded rather than contradicted.**

**One observation for 7.11, recorded and deliberately NOT acted on here.** The Gwladys red is the
model emitting `Gwladys Street<other "Goodison Park stand">` — kind_hint `other`, descriptor
"Goodison Park stand" — while the frozen prompt already carries iteration 13's exclusion ("never
stadiums, stands, streets"). **So the model DESCRIBED the stand correctly and the check scores the
raw `names[]` list regardless of kind.** Under T2 that is arguably the check being stricter than the
contract: the resolver is kind-gated, so an `other` carrying the descriptor "stand" cannot link to a
club, and this is not the same defect as inventing one. **Whether the fix is the prompt or the check
is 7.11's call, not the instrument session's.**

**The failure shapes**, now that the instrument is trustworthy:

1. **`names[]` drops the coach/manager class.** Kyle Shanahan (`coach-discovery-kyle-shanahan` —
   all four of its checks fail together: name absent, so kind, descriptor and the resolver's
   unresolved-record all fall with it), Moyes (`fan-protest-register-outrage`), Arteta
   (`injury-report-accept-no-invention`), Bellingham (`result-line-verbatim-score`), Rangers as a
   club (`opponent-only-mention`). This is **D-T1's under-fill with a specific shape**: the model
   lists the CLUBS and drops the PEOPLE attached to them. That is exactly the channel §1a leans on
   for discovery — `names[]` is how the Investigator learns a person exists — so the miss costs the
   living database, not just the fixture. Knob: the ep1 prompt's names[] ask, which currently
   treats people and clubs as one list; consider naming the coach/manager role explicitly in the
   ask. Measure against D-T1's 16.7% baseline and D-T11's truncation fix together — all three are
   the same class seen from different angles.
2. **`register[outrage]` reads `neutral`** on the fan-protest fixture. This is **D-T2 reproducing**
   under the phrase-before-label order, which was supposed to help. D-T2 says it needs a fixture
   set before revisiting; it now HAS one. Fold them.

### 6b · D-T18 — syndication doubles facts inside one packet

**Finding:** packet 2 (storyline 7471, the first shadow compile) carries 15 claims that are closer
to 8 facts — "Celtic have wrapped up an 11 million pound deal for Kasper Hoog" beside "Celtic have
signed Kasper Hoog"; "Bayern Munich's sporting director denied rumours linking Michael Olise with a
move to Real Madrid" beside "Bayern Munich denies Michael Olise will be leaving".

**The compiler is not at fault, and this is the important part.** The two members are articles
186800 and 186793 — both Goal.com transfer roundups from the same hour, correctly clustered by the
Desk. The packet faithfully carries both, which is the right default: T3 says two outlets asserting
a thing is evidence, and silently suppressing a restatement is precisely how a preserved
contradiction gets dropped. But **two lanes of ONE outlet is syndication, not corroboration**, and
it spends the 2,000-token render budget twice on one fact.

**Do not tune this before measuring what it costs.** On a 3-member packet it is noise. The exact-
title dedup sweep already catches the byte-identical case, so what is left is near-duplicates from
one source. The measurement: over a day of packets, what fraction of render budget goes to claims
sharing a source AND high text similarity? Knobs, in increasing order of how much they can break:
(a) collapse same-source near-duplicate claims at compile, keeping the longer; (b) prefer one member
per (source, hour) at assembly — cheaper, but it discards an article, so it owes an A5 exclusion
line naming what it dropped. **Never dedupe across DIFFERENT sources** — that is the T3 line, and
crossing it turns the contradiction-preserving property of a packet into a summarizer.

---

## 7 · Carried out of the flip (2026-08-06) — read these before tuning the Insider

### 7a · D-T20 — the Insider's proximity gate went inert at the cutover, and nobody chose that

**Measured on live post-flip data:** of 170 `news_article_entities` rows created since
`RAIL=packet` went live, **0 carry a `title_pos`**. Every one is an Editor 8.5 insert, and the
Editor does not compute that column.

**Why it matters here rather than in the rail plan.** `insider::load_candidates`
(`insider/mod.rs:318`) picks the (team, player) pairs the Insider will spend model calls on, and it
is deliberately NOT rail-gated — 7.5 ruled that the packet replaces what articles SAY, never which
articles they ARE. Its thinning clause is:

```sql
AND (te.title_pos IS NULL OR pe.title_pos IS NULL
     OR abs(te.title_pos - pe.title_pos) <= $5)
```

NULL passes, by design. So at 10:55 EDT on 2026-08-06 the gate stopped thinning anything, and the
Insider's candidate set widened — not by a decision, but as a side effect of 8.5 not writing a
column 8.4 never mentioned.

**This is probably the right outcome, which is exactly why it needs deciding rather than
inheriting.** Headline proximity was a proxy for "is this co-mention real?" back when co-mentions
came from a regex scanning a title. On the packet rail they come from the Editor having READ the
body and resolved a name. Proximity is a crutch for noise that no longer exists.

**But measure before you keep it or cut it.** The baseline: `transfer_rumors` ran **68 per 24h**
pre-flip. If post-flip pair volume climbs sharply, the widened candidate set is spending Mac
throughput on pairs the gate used to drop, and that is Insider tuning — it competes directly with
the other five voices for the Mac's single permit. Knobs, in order: (a) delete the clause outright
and let `HAVING count(DISTINCT te.article_id) >= $3` do the thinning (it is a better filter — it
asks for corroboration across articles rather than adjacency in one headline); (b) replace
proximity with the Editor's own `entity_roles` — a `passing_mention` pair is exactly what the
gate was trying to drop, and now we have the model's word for it instead of a character offset.
Knob (b) is the describe-then-derive version and is the one to reach for if (a) proves too loose.

Tied to **8.8** in `PLAN-one-rail.md` (the regex excision session), which lists this as its one
judgment call among otherwise straightforward deletions.

**UPDATE 2026-08-06 ~11:50 EDT — knob (a) IS DONE. 8.8 removed the clause, deliberately and with
the decision logged.** Both sites (`load_candidates` and `load_stale_pair_news_ids`) and
`COMENTION_PROXIMITY_CHARS` are gone as of `28fcf45`; `HAVING count(DISTINCT te.article_id) >= $3`
is now the only thinning, exactly as (a) describes. Re-measured before the cut: **0 of the 271 rows
created since the flip carried a `title_pos`** (170/0 four hours earlier — the finding held as the
sample grew), so it is a no-op for new data and a real change only for the pre-flip tail
(310,705 rows carry a position).

**What is left for this session is knob (b) and the measurement that decides it.** Baseline to beat:
`transfer_rumors` was **68/24h pre-flip** and read **70/24h** just before the cut — no explosion in
the first hour, but one hour is not a reading. If pair volume climbs and the Insider starts eating
the Mac's single permit, (b) is the answer: replace proximity with the Editor's `entity_roles`
(`passing_mention` is exactly what the gate was reaching for), which is the describe-then-derive
version and needs no character offsets at all.

### 7b · Prompt fat is the weekend's main event — the inventory is already written

Scott, at the flip: *"We're going to be able to trim a LOT of fat from the legacy prompts that we
copied over to the new rail."* The measurements that scope that work already exist and should not
be re-derived:

- **§2 of this file (D-T12)** — the Editor's output dominance and capacity numbers.
- **PLAN-one-rail 7.2's window budget** — the per-voice 4096 envelope (system ≤550 tok, memory
  ≤700, packet render ≤2,000, `num_predict` ≤800, prompt p99 ≤3,300). That table is the target
  the trimming aims at, and `eval_count` telemetry is how you assert you hit it.
- **7.11 is the step that owns the RAIL-scoped diet prompts** and is still open — it is where the
  trimmed versions land, and its `s17` bump spends one fleet-wide regen, so batch every prompt
  change into it rather than bumping twice.

**SCOTT, 2026-08-07 — THIS IS PROMOTED TO A HEADLINE WORKSTREAM OF THE TUNING SESSION:** *"A big
part of the tuning session will be trimming the prompts. They're messy now, and we'll be able to
get everything under the 4096 window size, I'm sure of it."*

**The gap is now MEASURED against 7.2's budget rather than asserted** (D-T29, 72h of
`cognition_ledger`). 7.2 sets **prompt p99 ≤ 3,300 tokens**. Four of six voices already clear it.
Two do not, and they are not close:

| voice | max prompt | ≈ tokens | vs the 3,300 budget |
|---|---|---|---|
| `narratives` | 35,975 chars | **≈ 7,574** | **~2.3× over** |
| `vibe` | 30,576 chars | **≈ 6,437** | **~1.9× over** |
| `momentum` | 12,040 chars | ≈ 2,535 | within |
| `sigil` | 9,010 chars | ≈ 1,897 | within |
| `transfers` | 5,314 chars | ≈ 1,119 | within |
| `rating` | 3,433 chars | ≈ 723 | within |

**So the diet is not six jobs — it is two, and they are large.** Scott's confidence looks well
placed: the four already inside the envelope prove the budget is achievable on this rail, so the
question for `narratives` and `vibe` is *which block grew*, not whether 4096 is realistic.
7.2 already decomposes the envelope (system ≤550 · memory ≤700 · packet render ≤2,000 ·
`num_predict` ≤800), so **the first move is to attribute each voice's tokens to those four buckets
and find the one that is over — do not trim by eye.**

**Why it is worth doing beyond speed:** over-budget prompts fail SILENTLY. When prompt +
`num_predict` exceeds the window the system prompt is evicted mid-generation — no error, no
dead-letter, just a voice quietly operating without its instructions. That is very likely a
contributor to the `momentum` markdown dead-letters (D-T28a), and it would never show up in a
failure count.

---

##### ⚠ MEASURED 2026-08-07 — **THE PROMPT IS NOT WHERE THE TIME GOES. THE OUTPUT IS.**

Recorded because it inverts the intuition this section was built on, and it should change what the
diet optimises for. Measured directly against the live gemma3:4b on the 1070 Ti (the Editor's own
largest prompt, replayed through ollama's `/api/generate`, reading `prompt_eval_duration` and
`eval_duration` rather than inferring from wall clock):

| phase | measured | rate |
|---|---|---|
| **prefill** (reading the prompt) | 2,049 tok in **1.70 s** | **~1,205 tok/s** |
| **decode** (writing the answer) | 400 tok in **7.62 s** | **~52.5 tok/s** |

**Prefill is ~23× cheaper per token than decode.** Decomposing a typical Editor call
(1,476 prompt in, 419 out): **≈1.2 s reading, ≈8.0 s writing — 87% of model time is OUTPUT.**

**Consequences, in the order that matters:**

1. **Trimming the Editor's PROMPT saves ~6%** (≈0.6 s of a ≈9.2 s call). Worth doing — but for
   VRAM and slot count, **not for wall-clock**. Do not expect speed from it.
2. **Trimming the OUTPUT is the speed lever.** The Editor averages **419 output tokens against a
   900 reserve**. Moving the ep1 envelope toward ~200 tokens takes ~4 s off a ~9 s call — **~40%,
   against prompt-trimming's 6%.** `eval_count` telemetry (already named above) is the instrument.
3. **Observed latency is mostly QUEUE, not compute.** `wall_ms` averages **33.6 s** while real model
   time is **≈9.2 s**. The card does **≈12 GPU-hours of work in a 24 h day (~50% utilised)** — so
   the strain Scott wants removed is not silicon saturation, it is scheduling.
4. **Concurrency is free headroom and is NOT being used.** The local backend is configured for 4
   in-flight calls (`COGNITION_BACKEND_CONCURRENCY=localhost=4`) and does not saturate them between
   the 02:00 bursts. **KV scales as `num_ctx × slots`, so D-T29's 8192→4096 buys the same memory at
   twice the slots** — the ceiling doubles even if today's load does not need it.

**THE THREE LEVERS, RANKED BY MEASURED EFFECT — deliberately the opposite of the intuitive order:**
**(1) output tokens · (2) concurrency · (3) prompt length.**

**None of this weakens the prompt diet — it re-aims it.** The two jobs stay, for two DIFFERENT
reasons, and they must not be conflated:
* **`narratives` and `vibe` (≈7,574 / ≈6,437 tok) must shrink for CORRECTNESS**, not speed. They
  exceed the 4096 window, and the failure is the silent system-prompt eviction above. That is
  mandatory regardless of what it does to the clock.
* **The Editor's prompt (max 2,048 tok) is already inside its window.** Its diet is optional and
  buys slots. If the target is a **2048** window, note the arithmetic is prompt + `num_predict`:
  at the current 900 reserve the prompt must land **≤1,148 tokens**, so the output contract has to
  move first — which is lever 1 again.

**Caveat on the rate, stated so it is not over-read:** 52.5 tok/s was measured on an otherwise idle
card. Under 4-way contention each stream is slower while aggregate throughput rises. Treat it as the
**per-stream ceiling, not the production rate.**

**And there is no coverage left to win by any of this:** a settled day reads 6,038 articles against
6,036 ingested with **0 backlog**. These levers buy drain SPEED and future headroom, not coverage.

---

## 8 · Carried in from Scott mid-session (2026-08-06, during the D-T19 instrument run)

**Recorded here the moment they were said, NOT started.** The instrument session's charter was
D-T19 and nothing else, and both of these are behaviour changes that deserve their own measurement
(§0 rule 4). They are the next session's material, in this order.

### 8a · D-T21 — cap the reader per entity · **BUILT AND INERT; SCOTT CHOSE 10/day; NOT DEPLOYED**

**STATUS 2026-08-06 ~17:00 EDT.** The cap is written, unit-tested and green (Go builds, `go vet`
clean, all packages pass), and it is **INERT: `EDITOR_MAX_READS_PER_ENTITY_DAY` defaults to 0, which
means NO CAP, so shipping the binary changes nothing.** Arming it is env + restart:

```
# on archbox, in .env.local
EDITOR_MAX_READS_PER_ENTITY_DAY=10
systemctl --user restart <the ingest unit>     # then watch "editor read cap reached"
```

**Scott's call on the size, made against the measurement below: 10/day (~82% cut), not 5.**

**What it does.** `persistArticles` counts this entity's articles already ingested today from the
provenance already on the rows (`raw->>'query_team_id'`, same transaction, one extra query — no new
state, nothing to backfill), and enqueues at most `cap - already` of this sweep's fresh inserts.
**The article row is still INSERTED and keeps its provenance; only the Editor's read is withheld**,
so nothing is lost and a backfill can enqueue the remainder later. The withheld count is **logged
per sweep** (`editor read cap reached … enqueued=… withheld=…`) rather than dropped, because a cap
whose bite is invisible cannot be tuned (§0b).
**Which articles survive is Google's call, not ours:** the fresh list is kept in Google result order
and the cap keeps its front. `needEditor` had to become a slice for that — it was a map, and Go
randomizes map iteration, so the old code had no order to cap. A test pins the ordering precisely so
a ranking heuristic cannot grow back where 8.9 removed one.
**Only team sweeps are capped**, because only they carry `query_team_id` — a stated limit of the
rule, not an oversight, and it leaves 31% of arrivals uncapped (see below).

**Still owed before this is called done:** the deploy is Scott's explicit act (§0 rule 6), and after
24h armed the before/after numbers must be read — `investigate_entity` drain (9,049 pending at
~57h), links per read (1.27 player links/read) and the `irrelevant` rate (15.4%).

*(The original framing and the measurement that sized it follow.)*

#### The ask, and the numbers that sized it

**Scott's words:** *"I think we should limit the reader to 5 articles per entity. That will free up
enough headroom for the Investigator to get meaningful work in, and the graph work as well."*

**Why it lands where §0a says the pain is.** The Editor runs at ~96% of ingest — parity, permanently
— so it has no headroom to burn down a backlog, and everything downstream is queued behind it:
`investigate_entity` **8,624 pending at ~57h**, `narratives`/`vibe` undrained bursts, `momentum`
1,269. A per-entity cap is the first knob in this file that BUYS capacity rather than spending it,
which is why it is worth doing before any prompt work: 7.11 makes calls better, this makes room for
them. **The cap is a VOLUME decision and it does not re-open the relevance decision** — Google still
does the relevancy and the Editor is still the valve (§0e); we are choosing how many of one entity's
arrivals are worth a read, not re-introducing a filter over which ones qualify.

**MEASURED FIRST, 2026-08-06 (~16:35 EDT), before touching anything — and the number is much bigger
than "5 per entity" sounds. THIS NEEDS SCOTT'S CONFIRMATION BEFORE IT SHIPS.**

An entity-day currently averages **~50 articles**, not a handful. Articles per (`query_team_id`,
ingest day) over the last 7 days:

| day | entity-days | articles | avg/entity-day | worst | entity-days over 5 | articles above a cap of 5 |
|---|---|---|---|---|---|---|
| Aug 2 | 154 | 7,191 | 46.7 | 232 | 89.6% | 89.7% |
| Aug 3 | 156 | 6,905 | 44.3 | 222 | 83.3% | 89.6% |
| Aug 4 | 151 | 7,985 | 52.9 | 259 | 90.7% | 90.9% |
| Aug 5 | 155 | 8,358 | 53.9 | 240 | 88.4% | 91.1% |
| Aug 6 | 156 | 8,778 | 56.3 | 259 | 89.1% | **91.4%** |

**A cap of 5 per entity per day cuts ~90% of the read stream — roughly 8,000 reads/day down to
~800.** The whole cap ladder, 7-day window: cap 3 → 94.0% cut · **cap 5 → 90.4%** · cap 8 → 85.3% ·
cap 10 → 82.2% · cap 15 → 75.2%. *Even a cap of 15 cuts three quarters.*

**Scott chose 10/day (~82% cut) on these numbers.** §0a says the Editor runs at parity with ingest and starves everything
behind it (`investigate_entity` **9,049 pending**, `narratives` 2,909, `vibe` 2,595), and a 90% cut
is decisive headroom, which is what "so we can start building up downstream examples" asks for.
**But it is a 10× reduction of the reading corpus, not a trim**, and it changes the product: fewer
facts, fewer links (1.27 player links per read today), fewer storyline members, thinner packets. The
size of the cut is Scott's call, not the implementer's.

**One scope fact that must be settled in the same breath: 31% of arrivals are out of the cap's
reach.** Only **39,312 of 56,887** articles in the 7-day window carry `raw->>'query_team_id'` at all
(69.1%) — the cap as described keys on the entity whose sweep landed the article, so the other 17,575
would be uncapped unless the rule says otherwise.

**Before implementing, the questions that decide the shape** — answer them from data, not intuition:
- **Cap per entity per WHAT?** Per day is the obvious reading (arrivals are one 02:00 batch); per
  sweep and per rolling-24h are different knobs with different tails. State the window explicitly.
- **Which 5?** The Editor reads what ingest enqueues; picking 5 requires an ORDER. Newest-first is
  the honest default. Anything cleverer is a ranking heuristic and this rail just deleted 393 lines
  of those (§0d) — **simple and durable beats clever and fragile.**
- **What happens to number 6?** Dropped, deferred, or enqueued-but-deprioritised. Deferred is not
  free: it grows a queue that nothing drains. Dropping is honest but must be COUNTED, not logged
  (0b's lesson — a WARN that says "continuing" hides its own frequency).
- ~~**Measure the class first**~~ — **DONE, above: 90.4% at a cap of 5.** The remaining question is
  not what the cap buys but whether that is the intended size.
- **Where it goes, when the size is settled:** `persistArticles` in `go/internal/thirdparty/news.go`
  (~:253). It already collects `needEditor` — the freshly-INSERTED article ids for ONE primary
  entity — and enqueues one editor item each, in the same transaction. The cap is a bound on that
  set. **Note what capping there does and does not do: the article row is still INSERTED and keeps
  its provenance; only the READ is withheld.** That is the honest shape — nothing is lost, the
  corpus stays, and a later backfill can enqueue the remainder — but it means "dropped" articles are
  invisible unless the skip is COUNTED (0b: a WARN that says "continuing" hides its own frequency).
- **Watch the cost:** an entity's 6th article is sometimes the one carrying the story. Pair the
  cap with a before/after on links-per-read and on `irrelevant` rate (15.4% baseline, §0e).

### 8b · D-T22 — the schema audit

**Scott's words:** *"we need to include a schema audit. We're running into issues where the pipeline
is clean, but we're attempting to force it to work with an outdated schema. We want both to be
optimized for the new approach of Google doing the relevancy work, and then empowering our models
for everything downstream. No most restrictive regex or fancy workarounds. Simple and durable beats
clever and fragile. We had to be clever before because we had no AIs working in our stream. Now, we
empower them to do the work and then update the SQL. The schema should support this as well as the
code and right now we're attempting to force the code to do that while also working with a schema
built for a very different pipeline."*

**This is the same rip 8.10/8.11 performed on `news_article_entities` (9 columns and two writers →
5 columns and ONE writer), applied to the rest of the schema.** The audit's question, per table:
*was this shaped for a pipeline where code guessed relevance, and does anything still read it?*

**The two rules the audit must carry, both earned this week:**
1. **SQL functions are code.** `compute_transfer_heat` was LIVE and still enforcing the mig-033
   proximity gate for six hours after we had recorded that gate as deleted from Rust (§0d). **Check
   `pg_get_functiondef` on every candidate, not just the Rust and Go call sites.**
2. **Measure blast radius from the DATA, never from the log** (§0b): journal retention said 5
   damaged articles; the data said 9.

**FIRST PASS RUN 2026-08-06 (~16:40 EDT), read-only, on live prod.** 82 tables, 116 functions. This
is the ingest/editor/packet path only — the rest of the schema is unaudited and the next session
should widen it. **Nothing was changed; every row below is a proposal with the evidence attached.**

**The headline, and it is exactly the shape Scott described.** `news_articles.bucket` and
`news_articles.routing_tags` **stopped being written at the flip** — 0 of the 448 post-flip articles
carry either, against 19,862 of 56,439 in the pre-flip week — **and `bucket` is still read by three
LIVE SQL functions**, one of which the Insider calls per pair:

| surface | written post-flip? | live SQL reading it | Rust callers of that SQL |
|---|---|---|---|
| `news_articles.bucket` | **0 of 448** | `compute_transfer_heat`, `refresh_typed_links`, `seal_narrative_threads` | 3 / 1 / 1 — **all live** |
| `news_articles.routing_tags` | **0 of 448** | *(none — see the trap below)* | — |
| `news_articles.topic_heat` | 0, and 0 in the whole 7-day window | — | — |
| `news_articles.full_text` | 364 of 448 | — | **LIVE, keep** |
| `news_articles.feed_rank` | all | `collapse_exact_title_duplicates` | 1 — live |

So the Insider's heat is being computed from a column that is NULL for everything ingested since
10:55 today. **The practical effect is small — even pre-flip only 1 of 24,673 recent articles carried
`bucket='transfer'` — and that is the point: three live functions branch on a column that was already
99.996% NULL and is now 100% NULL.** This is `compute_transfer_heat`'s proximity gate again, one
layer down: the code was cleaned, the SQL was not. **Verdict: NARROW — strip the `bucket` branch from
all three functions FIRST (behaviour change, own measurement, its own rehearsal), and only then drop
the column.** Do not drop it first; the functions would break.
**SCOTT'S CALL 2026-08-06: LEAVE IT FOR NOW, and change the SQL once when the audit is complete
rather than piecemeal.** The dead branch keeps running in the meantime — it is inert, not harmful
(it reads NULL and contributes nothing), and the cost of touching three live functions twice is
higher than the cost of leaving an inert condition in place for one more session.

**A trap worth writing down, because it cost a wrong conclusion before it was checked.**
`enqueue_voices_on_packet` — the LIVE trigger on `packets` — also names `routing_tags`, which reads
like the legacy column surviving in the hottest path on the rail. **It is not the same column.** It
is `packets.routing_tags`, the packet rail's own §1c tag join, and it is working (the tag-subscribed
voices all have pending work). **Two tables carrying one column name is how a grep produces a
confident false positive. Confirm the OWNER, not the name.**

**Dead, and Phase 9 already owns them** — recorded so the audit and the demolition agree:
`topic_heat_embeddings` (8,924 rows, **last write 2026-07-25** — the embedder), and
`news_article_readings` (63,798 rows, **last write 2026-08-05 02:00**, stopped at the flip; the
legacy `article_read` output, still the rollback surface for the 30,224 parked rows).

**One live table that no application code names at all:** `source_performance` (1,747 rows). No Rust
or Go file mentions any of its columns; it is read only through `source_reliability_for_pair` (2 Rust
callers, live) and refreshed only by `refresh_source_performance`, which **has no Rust or Go caller**
— yet the table was written today at 12:45, so the refresh is being driven from inside SQL. **Verdict:
KEEP but DOCUMENT** — a live table whose entire read and write path is invisible to a code search is
the next `compute_transfer_heat` waiting to happen. Find and name its driver.

**What this pass did NOT cover, stated so it is not mistaken for a clean bill:** the other 67 tables
(stats, fixtures, momentum, narrative_*, transfer_*), all 116 functions beyond the ten checked, and
every index. `momentum_scores` alone is 12.7M rows and was not looked at.

**Deliverable for the next session: finish the inventory table-by-table with a verdict — KEEP,
NARROW, or DROP — and for each DROP the query that proves nothing reads it.** The method that worked
here, in order: (1) does the column still get WRITTEN, split pre/post-flip; (2) `pg_get_functiondef`
across all functions for the name; (3) which of those functions has a live Rust/Go caller; (4) only
then propose. Steps 1 and 3 are the ones that keep being skipped.

---

#### PASS 2 — all 82 tables swept (2026-08-06 ~17:45 EDT), on Scott's go

**THE METHOD GREW A FOURTH LEG, and it is the one that matters most here: a table's driver can be a
SQL function called from a SHELL SCRIPT.** `recompute-tiers.sh` (cron, Mondays 02:00) runs
`SELECT recompute_entity_tiers(...)` straight from `psql`. That path is invisible to a Rust grep, a
Go grep, AND a repo grep for the table name — three of the four legs miss it. **Any table whose only
caller is a cron'd `psql` heredoc will read as dead to every search we were running.**

**Postgres 18's `last_seq_scan`/`last_idx_scan` make "does anything actually read this" a
MEASUREMENT rather than a guess — but only after three contaminations are removed:**

1. **The 04:00 `pg_dump` seq-scans every table.** Uncorrected, all 82 report "last read today
   04:02:37" and everything looks live. **The discriminator is a read AFTER 04:02:38.**
2. **Weekly crons.** `recompute-tiers.sh` and `football-meta` run Mondays. On a Thursday, "not read
   in 13 hours" is the expected state for their tables, not evidence of death.
3. **THE AUDIT CONTAMINATES ITSELF.** `topic_heat_embeddings` reported a read at 16:35:30 — that was
   *this audit's own `SELECT`*. An auditor's queries are indistinguishable from live traffic in
   `pg_stat_user_tables`. Take the read timestamps BEFORE you start querying the candidates, or
   discount your own.

**Result: 59 of 82 tables were read by something other than the backup today.** The other 23 split
into two groups, and the second is the interesting one.

**GROUP 1 — TRUE ORPHANS: named by no Rust, no Go, no SQL function and no view.** These are the
DROP candidates, and the two empty ones are unambiguous:

| table | rows | last write | verdict |
|---|---|---|---|
| `metadata_sync_log` | **0** | never | **DROP** |
| `season_recompute_needed` | **0** | never | **DROP** |
| `provider_entity_map` | 26,210 | 2026-08-01 | **INVESTIGATE FIRST** — 436k lifetime updates and nothing names it now; find what stopped |
| `topic_heat_embeddings` | 8,924 (15 MB) | 2026-07-25 | **DROP WITH PHASE 9** — the embedder's table, already Phase 9's |

**GROUP 2 — SQL-ONLY: live, and invisible to every code search we own.** This is
`compute_transfer_heat`'s category generalised, and it is the audit's real finding — **eight tables
whose entire read/write lifecycle happens inside the database**:

| table | its only driver |
|---|---|
| `metadata_refresh_queue` | `detect_team_change`, `get_metadata_queue_batch`, `get_metadata_queue_status`, `mark_metadata_processed` |
| `player_team_history` | `detect_team_change` |
| `provider_seasons` | `resolve_provider_season_id` |
| `rating_thresholds` | `_compute_rating_bundle` |
| `source_tiers` | `backfill_narrative_episodes` |
| `news_article_readings` | `collapse_exact_title_duplicates` — **genuinely live, runs during every ingest** |
| `entity_aliases` | `entity_aliases_no_update` (a trigger guard) |
| `source_performance` | `source_reliability_for_pair` / `refresh_source_performance` (pass 1) |

**Verdict for Group 2: KEEP, and DOCUMENT the driver in the table's own COMMENT.** Not one of these
is a deletion candidate; every one of them is a place where the next person greps the codebase,
finds nothing, and concludes the table is dead. **The fix is a comment, not a migration.**

**ONE FINDING THAT IS NEITHER — and it wants Scott's eye.** `metadata_refresh_queue` holds **42,782
rows, last written 2026-06-17 — seven weeks stale** — while four live functions stand ready to serve
it and nothing has drained it. That is not schema rot, it is **a stalled feature**: either 42,782
units of work nobody will ever do (drop the rows) or a pipeline that silently stopped (fix it). It
cannot be settled from the schema; it needs the product answer first.

**STILL NOT COVERED, said plainly:** every INDEX (none were examined, and unused indexes on
`momentum_scores` at 12.7M rows would be the biggest storage win available), and the 106 functions
not individually read. The sweep answered "is this table used"; it did not answer "is this table the
right shape", which is the other half of what Scott asked for.

---

#### PASS 3 — EXECUTED (2026-08-06 ~18:45–19:30 EDT). Migrations 215 + 216 applied.

**Shipped:** migration **215** (metadata cluster settled + 9 driver COMMENTs) and **216** (the
orphan 215 left behind). Snapshot taken and committed with them. Method note: the
`pg_stat_user_tables` snapshot was taken at 18:43 **before** any candidate query, so contamination
C did not recur — and it showed up anyway (`metadata_refresh_queue.idx_scan` ticked to 1 at
18:44:08, which was this audit's own proof query, correctly discounted).

**THE METHOD NEEDED A FIFTH LEG, and it invalidated two of pass 2's four "true orphans."**
The four legs were Rust, Go, SQL functions/views, and cron'd shell heredocs. **The missing leg is
PYTHON — the `seed/` layer, driven by `cron-live-fixtures.sh`.**

| pass 2 said | pass 3 measured | corrected verdict |
|---|---|---|
| `season_recompute_needed` — TRUE ORPHAN, "DROP, unambiguous" | written by `seed/shared/upsert.py` (`mark_season_recompute_needed` / `clear_…`); **0 rows is the HEALTHY state** — it is a durable safety queue for a failure that has not yet occurred | **KEEP.** Dropping it breaks `upsert.py` exactly when a recompute fails. Retire with the Python prune. |
| `provider_entity_map` — TRUE ORPHAN, "investigate before dropping" | written by `seed/shared/upsert.py:upsert_provider_entity_map` from `seed/services/{roster,meta}/cli.py`; 436,729 lifetime updates; last write 2026-08-01 because the **provider feeds are disconnected**, not because the writer died | **KEEP.** Retire with the Python prune. |

**Scott, 2026-08-06, mid-session:** *"Python was the old seeding layer. We cut the cord with our api
providers since the Investigator will be doing this role of fetching data. We're going to prune
Python in the future."* So these two, plus **`provider_seasons`** (driven by
`resolve_provider_season_id` ← `seed/shared/db.py`), are not orphans and not keepers — they are the
**PYTHON PRUNE SET**, the exact analogue of Phase 9's demolition set. *Do not front-run that prune;
the seed crons are still live in `crontab`.* The `SPORTMONKS_API_TOKEN is required` errors in
`logs/cron-football.log` (weekly `football-meta`, `football-refresh`) are that cut cord, not breakage.

**FINDING 1 — SETTLED. The metadata feature: Scott chose KEEP THE HISTORY, DROP THE QUEUE.**
Migration 215 **narrowed `detect_team_change`** (it still maintains `player_team_history`; the
`metadata_refresh_queue` enqueue block is gone) **in the same transaction** that dropped the queue,
so the trigger was never left pointing at a missing table. Dropped: `metadata_refresh_queue`
(42,782 rows, 0 ever processed), `metadata_sync_log` (0 rows, never once written), the view
**`metadata_queue_status`** (which pass 2 never found — leg 2's view half caught it, and it would
have blocked the drop), and `get_metadata_queue_batch` / `get_metadata_queue_status` /
`mark_metadata_processed`. Kept: `player_team_history`, 42,782 rows, 14,215 distinct players.
Migration **216** dropped `update_sync_log_timestamp()`, orphaned when `metadata_sync_log` went.

**Rehearsed before applying**, per §0 rule 7, in a ROLLED-BACK transaction with the invariant
asserted inside it — the invariant being the one that actually matters: *an `event_box_scores`
INSERT must still succeed and still write history with the queue gone.* Result inside the
transaction: `PASS: box-score INSERT succeeded; player_team_history 42782 -> 42783; queue objects
gone`, then ROLLBACK. Post-apply verification: queue/synclog/view all `GONE`, history 42,782,
`trg_detect_team_change` still `tgenabled='O'`, 0 leftover consumer functions.

**FINDING 2 — DONE, with corrections.** All 9 driver COMMENTs are on. Three of pass 2's "eight
SQL-only tables" were not SQL-only: `entity_aliases` also has the view `investigator_review_accepted`
and `rust/src/junctions/investigator/entity.rs`; `news_article_readings`' driver
`collapse_exact_title_duplicates` is called from `rust/src/worker.rs`; `provider_seasons` is Python.
`source_performance`'s pass-1 open question is **CLOSED** — its driver is
`scripts/hosting/cron-narrative-links.sh` (cron `45 */6 * * *`), a psql heredoc calling
`refresh_source_performance('FOOTBALL'/'NBA'/'NFL')`. Its DELETE-then-INSERT-per-sport refresh is why
`n_tup_del` is huge; that is churn, not data loss.

**Also fixed:** `sql/metadata_system.sql` — a 306-line base file describing the dropped queue system,
applied by *nothing* (`build.sh` clones prod via `pg_dump`; `migrate.sh` only globs
`sql/migrations/*.sql`). It is now reduced to the surviving half and carries a header saying so. It
was itself an instance of the thing Scott is complaining about: an outdated schema artifact left
lying where the next person would trust it.

---

##### FINDING 4 IS WRONG, AND THIS IS THE SESSION'S MOST IMPORTANT RESULT. The bucket NARROW did **NOT** ship.

Pass 2 justified stripping the `bucket` branch from three live functions on the premise that the
column was *"already 99.996% NULL and is now 100% NULL"*, so the branches were **inert**. **Measured
against the window those functions actually read, the premise is false.**

**Correct on the write question** (and this pass confirms it, measured on `fetched_at`, the ingest
clock, not `published_at`): in the last 14 days, pre-flip 129,416 articles / 54,541 bucketed;
**post-flip 1,298 articles / 0 bucketed.** Last bucketed article by ingest: **2026-08-04 02:05** —
the write actually died two days *before* the flip. `topic_heat`: 0 written in either era.

**Wrong on the read question, which is the one that licenses the change.** `compute_transfer_heat`
filters `published_at > NOW() - INTERVAL '14 days'`, and that window is still full of *historical*
bucketed rows:

| bucket | rows in the live 14-day window |
|---|---|
| NULL | 74,364 |
| `non_transfer` | **42,388** |
| `transfer` | **10,572** |

So the three functions are **not** branching on nothing, and the three do **not** share a verdict:

* **`compute_transfer_heat`** — `WHERE a.bucket IS DISTINCT FROM 'non_transfer'`. A *negative*
  filter. It is **excluding 42,319 articles from the Insider's corpus right now.** Stripping it
  today widens the corpus by a third and moves every heat score the Insider computes per pair.
  **Not a no-op. Not inert.**
* **`seal_narrative_threads`** — `JOIN … AND na.bucket = 'transfer'`. A *positive* filter, and the
  opposite hazard. It currently reaches **1,731** open transfer-flavored threads; without the
  predicate the resolved arm reaches **6,080** — a 3.5× expansion into precisely the false sealing
  the function's own comment warns against (*"a player's injury saga must not seal because an
  unrelated move confirmed"*). Stripping it doesn't remove a dead branch, it **turns a safety gate
  off.**
* **`refresh_typed_links`** — **A FALSE POSITIVE.** Its only "bucket" is line 96, a *comment* about
  the ±10-**bucket** trajectory vocabulary. It never references `news_articles` at all. **This is
  contamination D one level deeper: not two tables sharing a column name, but one English word
  meaning two things — and a grep cannot tell prose from code.**

**So: stripping these branches was never a cleanup. It is a behaviour change to the Insider's heat
and to thread sealing, and shipping it blind would have violated ONE CHANGE, ONE MEASUREMENT.**
Scott's "batch it with the audit's SQL changes" instruction was given on the stated premise that the
branches were inert; the premise did not survive measurement, so the instruction was not executed.
**Nothing about `bucket`, `topic_heat` or `routing_tags` was touched.**

**THERE IS A DATED, SILENT DEGRADATION AND IT IS THE REAL FINDING.** The last bucketed article by
publish time is **2026-08-06 15:33**. On **2026-08-20 15:33** `compute_transfer_heat`'s 14-day
window ages past it, its predicate becomes a true no-op, and **the Insider's corpus silently widens
by ~33% with no deploy, no log line and no alarm.** `seal_narrative_threads` decays on a different
clock (no time window; its 1,731 threads bleed out as they seal or fade at 21 days), so its resolved
arm quietly drifts toward zero. **Two live functions change behaviour on a date, driven by data
ageing out, with nobody watching.** That is a far worse failure mode than a dead branch, and it is
what the "inert" reading would have hidden.

**THE RIGHT-SHAPE QUESTION — ANSWERED BY SCOTT, 2026-08-06, and the answer retires the question
rather than solving it.**

**Correction to this section's own first draft, which said "nothing in the packet rail currently
supplies" a transfer-vs-not signal. That was wrong, and it was wrong the same way the rest of
D-T22 was wrong — by grepping instead of reading.** The successor is not only built, it is live and
documented as the successor in `rust/src/bucket.rs`:

* `routing_tags_from_story_type` (bucket.rs) — the **multi-valued** projection of the Editor's
  `story_type`, described in its own doc comment as *"the multi-valued successor to
  `ArticleBucket::from_story_type`"*. Its stated reason for existing is exactly the bucket defect:
  *"`bucket` can say 'transfer' OR 'injury' and never both, so a story could only ever reach one
  voice."*
* `editor::derive::routing_tags(story_type, register)` — the LIVE path, which adds `charged` when
  the register is non-neutral. `routing_tags("transfer","outrage") == ["transfer","charged"]`.
* `stage_routing_subscriptions` — which voice wakes on which tag, **as DATA**. Live rows:
  `transfer`→`transfers` (Insider, team), `charged`→`vibe` (Influencer, player+team),
  `narratives`→`narratives` (Journalist). *"That keeps the routing decision an INSERT rather than a
  code change, and it means this function never has to know the cast."*
* And it is **working**: 2,788 of 9,462 packets carry ≥2 tags — `{charged,fixture,transfer}` 211,
  `{fixture,transfer}` 134, `{charged,transfer}` 117. A charged transfer story already wakes the
  Insider AND the Influencer, neither waiting on the other.

**SCOTT'S RULING, verbatim, 2026-08-06:** *"The bucket system is a legacy one before the tag system.
It was deterministic, versus empowering. The tag system allows the character to be the one with the
authority to interpret something that the Editor has tagged could be relevant to that character.
The bucket system limits the voices."* And on the heat function specifically: *"The heat index thing
for transfers is legacy nonsense. In the tuning session we have planned for the future, we're going
to have the character determine the heat, not the Editor. Let the expert be the expert."*

**So the bucket NARROW is CANCELLED, not deferred, and it was never rewired to the Editor's tags.**
Rewiring `compute_transfer_heat`'s corpus to `story_type` would have been polishing a function
that is itself slated for removal — the Editor would still have been deciding the Insider's heat,
which is the exact inversion of authority Scott is removing. **The measurement that would have
justified the rewire was taken and is recorded below for the tuning session, then not acted on.**

*(Measured 2026-08-06 on the 200 hottest real pairs, for D-T24's benefit: swapping
`bucket IS DISTINCT FROM 'non_transfer'` for the Editor's transfer tag takes the pair corpus from
3,321 articles to 344 and drops 111 of 198 pairs to no heat at all. The decomposition is the
useful part — of 2,119 articles the OLD rule admits, **1,455 were never read by the Editor** (a
transition artifact that closes as post-flip articles fill the 14-day window) and **522 were read
and called something else, 344 of them `fixture`**. The old "transfer heat" was counting more match
reports than transfer stories. That is the number that says the heat index is legacy nonsense, and
it agrees with Scott.)*

**`news_articles.bucket` therefore has no successor to wait for and no reason to be rewired.** It
is dead legacy with two live readers, both of which are themselves legacy. It comes out with D-T24,
not before — dropping it while `compute_transfer_heat` and `seal_narrative_threads` still branch on
it would break both.

---

##### NEW LEDGER ITEMS OUT OF D-T22 — all three are Scott's, all three are for the tuning session

**D-T23 — ONE ARTICLE, MANY TAGS. Scott: *"a single article will need multiple tags, when
applicable. That's a no brainer."*** The tag SET is multi-valued and the packet rail unions tags
across a storyline's articles — but the per-article projection is still **single-valued at the
source**: ep1 emits one `story_type` (an enum, not an array), so one article yields exactly one
topic tag plus an optional `charged`. A piece that is genuinely a transfer AND an injury story
picks one and loses the other voice. Today that is masked at packet grain (a storyline containing a
transfer article and an injury article gets both tags), which is why it has not bitten visibly.

*Scope, so it is not underestimated:* this is an **ep1 contract change** — `story_type` → a
multi-valued field — and the ep1 contract is constrained-decoded with **property order as the
contract**, so it is a version bump (`ep1`→`ep2`, and `contract_version` is a cache key, T1), the
Editor's schema and prompt, `routing_tags_from_story_type`, `derive::routing_tags`, and the packet
compiler's rollup. **NOT DONE THIS SESSION: the standing rule is that no prompt or
`*_PROMPT_VERSION` is touched outside 7.11.** Flagged, scoped, not started.

**D-T24 — THE HEAT INDEX MOVES TO THE CHARACTER.** Scott: *"we're going to have the character
determine the heat, not the Editor. Let the expert be the expert."* `compute_transfer_heat`
(mig 032, SQL, called per pair by the Insider at `rust/src/junctions/insider/mod.rs:385`) is the
legacy deterministic scorer. When it goes, so do its dependents: the `bucket` branch inside it,
`news_articles.bucket` itself, `idx_news_articles_bucket`, and the `bucket` branch in
`seal_narrative_threads`. **Do these as ONE migration when the replacement lands** — the audit's
standing finding is that dropping the column before the readers breaks both functions.

**D-T25 — THE SCOUT IS NOT LISTENING, AND THE INJURY TAG GOES NOWHERE.** Scott, told of it: *"That's
fine if the injury reports aren't being read yet. That's part of the tuning session. Make sure it's
noted in the plan."* **Noted, with the measurement:** the Editor is producing `injury` tags —
**349 packets carry one** — and `stage_routing_subscriptions` has **no `injury` row for any stage**,
so nothing wakes on them. The tag is written and dropped on the floor. Further, the Scout is
presently `Role::StatsLogic` (ratings/PEAK, per `rust/src/eval_tasks.rs`), a **stats** junction that
does not consume packets at all — so this is **not** a one-row INSERT into
`stage_routing_subscriptions`. Making the Scout a packet reader is the actual work; the
subscription row is the last line of it, not the first.

---

##### THE INDEX PASS — started, measured, nothing shipped

Stats window is genuine: `pg_stat_database.stats_reset` is NULL (never reset) and the postmaster has
been up since **2026-07-26**, so idx_scan counters cover **11 days** of real traffic.

**249 indexes, 2,695 MB total, 394 MB never scanned (78 indexes).**

**`momentum_scores` was pass 2's predicted "biggest storage win available." It is not.** Its 1,360 MB
of indexes on a 1,939 MB heap look damning until you read the usage: the 1,066 MB
`idx_momentum_scores_read` has **2,762,239 scans** — it is one of the hottest indexes in the
database. The only unused thing there is `momentum_scores_pkey` (294 MB, 0 scans), and that is a
uniqueness guarantee on a 12.7M-row table, not a performance index. **No win here. The guess was
wrong.**

**`news_article_entities` at 140% index-to-heap also survives scrutiny** — all three of its indexes
are heavily used (5.1M / 4.1M / 280k scans). The ratio is a small heap and a wide PK, not bloat.

**The real finding is small in bytes and lives on the hot path — dead-column indexes still being
maintained on every article INSERT:**

| index | size | scans (11 days) | why |
|---|---|---|---|
| `idx_news_articles_feed_rank` | 10 MB | **0** | partial btree, `WHERE duplicate_of IS NULL` |
| `idx_editor_reads_resolved` | 7,216 kB | **0** | GIN |
| `idx_news_articles_topic_heat` | 2,608 kB | **0** | on a column with **0 writes all window** |
| `idx_news_articles_routing_tags` | 936 kB | **0** | GIN, on the **dead** `news_articles.routing_tags` |
| `idx_news_articles_bucket` | 888 kB | 2 | on the frozen `bucket` |

**~21 MB — the value is not storage, it is write amplification: every ingest maintains two GIN
indexes and three btrees that nothing has read in eleven days, on the busiest table in the pipeline.**

**Deliberately NOT shipped this session.** Three of these five index the exact columns that now come
out with **D-T24** (`bucket`, `topic_heat`, `routing_tags`). Dropping the index now and the column
later is two changes where one will do. **They go WITH D-T24, in the same migration.** The remaining
two (`feed_rank`, `editor_reads_resolved`) are independently droppable — but `feed_rank` at 0 scans
while `collapse_exact_title_duplicates` reads that column every ingest means the planner is
seq-scanning instead, which is a question about that function, not a licence to drop the index.

**A TRAP FOR D-T24, found while checking F-022 deploy order:** `news_articles.topic_heat` reads like
a free drop — 0 writes in the entire window, 0 index scans — **but the LIVE Influencer selects it**
(`rust/src/junctions/influencer/mod.rs:148`, `SELECT max(a.topic_heat)`, into a struct field, with
`COALESCE(...,1)` so an all-NULL column silently yields the constant 1). Dropping the column ahead
of a Rust change fails the API at boot (`db.New` prepares every statement and fail-fasts on drift).
**Column DROP inverts deploy order (F-022): ship the backward-compatible binary FIRST, then
migrate.** `news_articles.routing_tags` has the same shape — its only writer is
`article_reader/mod.rs:1073`, which is Phase 9's demolition set, so it must go WITH Phase 9 and not
before it.

**ACCEPTED DRIFT, not an open worry:** the 2026-08-20 15:33 boundary above still stands —
`compute_transfer_heat`'s corpus widens ~33% on its own when the last bucketed article ages out of
its 14-day window. With the heat index now slated for replacement under D-T24, that drift is
**accepted and recorded rather than defended**. It is only a hazard if D-T24 slips well past it, in
which case the Insider is scoring pairs off a corpus that quietly changed shape. Worth one line in
the D-T24 opening: check whether the drift has already happened before trusting any pre-existing
heat baseline.

**STILL NOT COVERED, and still not a clean bill:** the 106 functions never individually read (this
pass read 5 of them properly, and 1 of those 5 — `refresh_typed_links` — turned out to be a grep
artefact, which is not a reassuring hit rate for the other 106). Every index on the stats/fixtures
side. And the right-shape question is now *posed* rather than *answered* for exactly one column
family; the other 81 tables have not been asked it at all.

---

#### PASS 4 — THE FUNCTION READ (2026-08-06, Scott: *"read the rest of the functions"*)

**Corrected inventory: there are 77 functions, not 116.** The 116 count included 35 `C`-language
extension functions (pg_trgm, unaccent, vector) that are not ours and 4 aggregates.
**All 77 have now been read individually — 4,620 lines.** Method: a seven-leg caller map built in
SQL (trigger / other function / view) joined to a repo grep on five languages (Rust / Go / Python /
shell / SQL), then every body read.

*(Process note worth keeping: the first caller-grep pass reported 38 functions with no caller,
including `collapse_exact_title_duplicates` and `recompute_entity_tiers`, both of which are
provably live. The loop had been run from the scratchpad directory, so every relative path missed.
**A grep that returns "no callers" for a function you already know is live is the only reason that
error was caught** — if it had listed only unfamiliar names it would have become the finding.)*

**Only 6 of 77 have no caller on any leg** — a far better result than the table sweep, and the two
that matter are not deletion candidates:

| function | lines | what it actually is | verdict |
|---|---|---|---|
| `assert_provenance_firewall` | 50 | **a safety guard that nothing ever runs** — see below | **WIRE IT** |
| `refresh_entity_name_surfaces` | 53 | full rebuild of the T9 resolution surface | **LABEL IT** — latent hazard, below |
| `backfill_narrative_episodes` | 117 | one-shot historical backfill; the ONLY reader of `source_tiers` | tool — keep or retire deliberately |
| `box_score_coverage_report` | 48 | diagnostic report | harmless tool |
| `apply_rate_siblings` | 42 | rate-sibling expansion, superseded by `rating_datapoints`' inline rate handling | DROP candidate |
| `resolve_provider_fixture_id` | 13 | provider→fixture lookup | **PYTHON PRUNE SET** (sibling of `resolve_provider_season_id`) |

##### THE FINDING: `assert_provenance_firewall` IS A GUARD AGAINST EXACTLY THIS AUDIT'S FAILURE MODE, AND NOTHING CALLS IT

It asserts that `refresh_typed_links` and `score_transfer_likelihood` — the two measurement-side
readers of `narrative_events` — still filter `origin = 'extraction'`, so that junction-authored
events (mig 170) can never re-enter the numeric feedback loop. It raises with a written HINT
telling you how to fix the breach. **It has no caller: no cron, no shell heredoc, no Rust, no Go,
no trigger, no view, no other function.** A tripwire with nothing attached to it.

**Verified rather than assumed, because that is the whole lesson of this audit: the firewall
currently HOLDS.** Both consumers still carry `origin = 'extraction'`
(`refresh_typed_links` and `score_transfer_likelihood`, checked by regex against
`pg_get_functiondef`, both `true`). So the finding is **"the guard is unwired," not "the firewall
is breached."** But the guard is the only thing that would ever tell us — and `compute_transfer_heat`
is the standing proof that a filter can survive in SQL long after everyone believes it is gone. It
costs one line in `cron-narrative-links.sh`, which already runs both consumers every 6 hours.

##### THE LOADED GUN: `refresh_entity_name_surfaces` — safe today, destructive later

It does `DELETE FROM entity_name_surfaces` and rebuilds from `teams`/`players`/`persons`
`.name` + `.search_aliases` only. But the table is maintained INCREMENTALLY by Rust — the
Investigator mirrors a surface for every proven alias at `investigator/entity.rs:501`, sourced from
`entity_aliases`, **which the rebuild does not read.** Anything the Investigator has learned that
is not also in `search_aliases` is destroyed by a function whose name reads like a harmless refresh.

**Measured before claiming it: 0 surfaces would be lost today** — every one of the 16,712 current
rows is reproducible from names + search_aliases, and no entity is missing a surface (player 0,
team 0, person 0). **So this is LATENT, not live.** It becomes real the first time the Investigator
proves an alias that never lands in `search_aliases`. The fix is a `COMMENT ON FUNCTION` saying so —
the same cheap fix as the table drivers, for the same reason.

##### TWO DEPENDENCIES D-T24 DID NOT KNOW ABOUT — both found only by reading

1. **`source_reliability_for_pair` CALLS `compute_transfer_heat`.** Not "reads the same table" —
   literally `FROM public.compute_transfer_heat(p_team_id, p_player_id, p_sport) h`, unnesting its
   `news_ids` to find the corpus's sources. Its own comment says why: *"Reuse compute_transfer_heat's
   news_ids so 'the corpus' has ONE definition and this card can never drift from what the prompt
   shows."* **`source_reliability_for_pair` has 3 live Rust callers in the Insider.** So retiring the
   heat function silently breaks the Insider's source-reliability card too — and if it is instead
   left pointing at a stale corpus definition, the card drifts from the prompt, which is the exact
   failure its comment was written to prevent. **D-T24's blast radius is one function wider than
   recorded.**
2. **`source_tiers` is inert.** Its only reader is `backfill_narrative_episodes`, and that has no
   caller. 13 rows of tier weights that nothing consumes. **This corrects migration 215's own
   COMMENT**, which names the driver truthfully but omits that the driver never runs. Not urgent —
   13 rows — but the comment should say "reader exists, is never invoked."

##### DEAD BRANCHES INSIDE LIVE FUNCTIONS (clarity, not measured performance)

* `_compute_rating_bundle` builds a `comp_facet` CTE that **nothing references** — `comp` selects
  only from `comp_flat`. Postgres does not execute an unreferenced CTE, so this is dead code, **not
  a measured cost**; stated that way deliberately rather than sold as a win.
* `compute_event_starline` declares `v_balanced BOOLEAN := FALSE`, never assigns it, then unions
  `comp_flat WHERE NOT v_balanced` with `comp_facet WHERE v_balanced`. The facet arm is permanently
  unreachable. Same class: an abandoned A/B left switched off in the code path rather than removed.

Both are the shape Scott named — *clever and fragile* left standing after the decision it served
was made. Neither is urgent; both belong to whichever migration next touches the rating path.

---

#### (B) HOW THE SCHEMA WORK RIDES ALONG WITH THE VOICE WORK — Scott, 2026-08-06

*"We'll need to find a way to include the schema work as part of the voice work, and include our
findings."* Then, refining it: *"there should be a dedicated schema session after the voice one …
we should be noting schema edits as we move through the voice work."*

**So the rule is a SPLIT, not "everything rides along" — and the split is by COUPLING:**

* **COUPLED → ships inside the voice migration.** Any schema change whose safety depends on the
  voice change, or whose absence breaks something the voice change lands. Dropping
  `compute_transfer_heat` when its replacement arrives; rewiring `source_reliability_for_pair`;
  dropping `topic_heat` after the Influencer binary. These CANNOT wait for a schema session —
  deferring them leaves a half-migrated rail, which is the exact state D-T22 was called to clean up.
* **UNCOUPLED → logged, not done, and handed to the schema session.** Pure cleanup with no
  dependency on any voice change: unused indexes, missing COMMENTs, `apply_rate_siblings`,
  `source_tiers`, the unexamined stats/fixtures indexes, the other 81 tables' right-shape question.
  Doing these mid-voice-work bundles unrelated behaviour changes into a tuning measurement and
  violates ONE CHANGE, ONE MEASUREMENT.

**The test is one question: *does the voice change make this safe, or is it merely near it?*** Only
the first rides along. Everything else goes to **Appendix S** at the end of this document, which is
the schema session's inbox.

Each voice item below therefore carries only its COUPLED payload, ordered, with the constraint
stated.

**D-T23 — one article, many tags. Schema payload: almost none, and that is the point.**
`packets.story_types` is already `text[]`; `packets.routing_tags` is already `text[]`;
`editor_reads.read` is jsonb. The multi-tag change is a **contract** change (`ep1`→`ep2`,
`contract_version` is a cache key, T1) plus Rust — `routing_tags_from_story_type`,
`derive::routing_tags`, the packet compiler's rollup. **No DDL is expected.** If that holds it is
the cleanest possible confirmation that the packet rail was shaped right; if a column IS needed,
that is a signal worth stopping on.

**D-T24 — the heat index moves to the character. This is where the schema debt actually lives.**
ONE migration, in this order, and the order is not negotiable:
  1. **[DEPLOY FIRST]** the Influencer binary that stops selecting `news_articles.topic_heat`
     (`influencer/mod.rs:148`). F-022: a column DROP inverts deploy order.
  2. Land the character-authored heat replacement; **rewire or retire `source_reliability_for_pair`
     in the same breath** (finding above) so the Insider's source card and its corpus keep ONE
     definition.
  3. Then, in one migration: `DROP FUNCTION compute_transfer_heat`; strip the `bucket` branch from
     `seal_narrative_threads`; `DROP COLUMN news_articles.bucket` (+ `idx_news_articles_bucket`,
     888 kB); `DROP COLUMN news_articles.topic_heat` (+ `idx_news_articles_topic_heat`, 2,608 kB);
     drop `idx_editor_reads_resolved` (7,216 kB, 0 scans) and `idx_news_articles_feed_rank` (10 MB,
     0 scans).
  4. **NOT in this migration:** `news_articles.routing_tags` + its GIN index. Its only writer is
     `article_reader/mod.rs:1073` — **Phase 9's demolition set owns it.**

**D-T25 — the Scout listens. Schema payload: one INSERT, and it is the LAST step, not the first.**
`INSERT INTO stage_routing_subscriptions (tag,stage,entity_type) VALUES ('injury', <scout stage>,
'player'|'team')` — two rows, because neither trigger reads `'*'` as a wildcard (D-T15). But the
Scout is `Role::StatsLogic` today and does not consume packets at all, so the row is the last line
of the work. **Inserting it early does not "enable" anything — it enqueues `pipeline_work` for a
stage with no consumer, which is precisely the stalled-producer shape migration 215 just deleted.**

**D-T26 (new) — wire `assert_provenance_firewall`.** One line in `cron-narrative-links.sh`, which
already runs both consumers every 6 hours. No schema change. Cheapest item in the ledger and it
guards the failure mode that cost this audit the most.

**D-T27 (new) — settle the 6 caller-less functions**, per the table above: `COMMENT ON FUNCTION` for
`refresh_entity_name_surfaces` (the loaded gun) and `backfill_narrative_episodes` (+ correct
`source_tiers`' comment); DROP `apply_rate_siblings`; `resolve_provider_fixture_id` joins the Python
prune set; `box_score_coverage_report` stays as a tool.

**STILL NOT COVERED after pass 4:** every index on the stats/fixtures side (`event_box_scores`
carries 820 MB of indexes on a 4.1 GB heap and none were examined). The right-shape question is now
answered for the `news_articles` column family and posed for none of the other 81 tables. And the
`nba.*` / `nfl.*` / `football.*` schemas were never in scope — only `public`.

---
---

### D-T28 — TWO LIVE DEAD-LETTER STREAMS (found 2026-08-06 ~22:50 EDT taking an 8.7 reading)

Neither halts a phase (§0 law). Both burn voice capacity every hour and both sit inside 8.7's
window. **Recorded, NOT fixed — the cause of the second is not proven and I will not ship a theory.**

**(a) `momentum` — 77 failed, 76 of them TODAY.** Resumed at **11:00** (the flip was 10:55) and ran
**8–12/hour every hour through 22:00**. One cause, unchanged and already known:
*the voice answers in markdown* — `invalid response (raw="**Momentum Read: …`. **The one-rail STATE
said these stopped at the deploy. That was true when written and is now false.** This is the same
contract-label class as the Analyst's `VIBE:`/`READ:` miss (momentum-s13) and belongs to **7.11**.

**(b) `sigil` — 14 failed, ALL today, first 16:52, 12 in the 22:00 hour alone. NEW, and
ENTITY-SHAPED.** The failure is `crown: could not parse reading+score` — the model returns
well-formed-looking JSON opening `{"reading": "…` and the parser cannot pull `reading`+`score`.

**The shape is the finding.** Failures: **12 NBA `team`** + 1 player each in NBA/NFL/FOOTBALL.
Successes in the same 36h: 298 NFL player, 46 NBA player, 20 FOOTBALL player — and **only 2 NBA
team**. So **NBA team crowns are failing ~86% of attempts while player crowns succeed ~99%.**
It is not entity rot: every failing entity has **28–142 prior successes**.

**What was RULED OUT, so the next session does not re-walk it:**
* **Not a timeout.** `OLLAMA_TIMEOUT_SECONDS=600`, `COGNITION_HANDLER_TIMEOUT_SECONDS=1200`; the
  slowest *successful* call is 156 s. *(I proposed the timeout theory and it did not survive
  contact — noted so it is not proposed again.)*
* **Not obviously the output budget.** `ORACLE_NUM_PREDICT=1100` against successful `eval_count`
  of **171–345**. Nothing near the ceiling.
* **Prompt size is suggestive but NOT proof.** Team prompts average **7,669 chars** vs player
  **4,852** — but player prompts reach **9,010** and still parse. So "team prompts are bigger" does
  not by itself explain an 86% failure rate at `VOICE_NUM_CTX=4096`.

**WHY IT CANNOT BE SETTLED RIGHT NOW, which is itself the actionable finding:** the error truncates
the response at **200 characters** (`util.rs::truncate`), and **failures never reach
`cognition_ledger` at all** — the ledger holds 26,007 `parsed` rows and no failure outcome, because
the parser bails before the ledger write. **So the single most useful next step is not a fix, it is
raising the diagnostic capture on the error path** (and/or ledgering the failure) so the next
occurrence is diagnosable. That is a small change but it needs a **deploy**, which restarts
`scoracle-cognition` inside 8.7's watch — Scott's call, not a silent one.

**Correction, logged because it was said out loud:** this was first described as *"looks like a
parser bug, the cheapest of the two fixes."* **That was wrong.** The parser is behaving correctly on
a malformed input; the defect is upstream, it is entity-shaped, and its cause is still open.

**Known-adjacent, already in the record:** Phase 7's Log flagged *"the first sigil at 4096 named TWO
peers and used z-scores/percentile in served prose — or8's own documented defects, now reproducing
live at the smaller window."* **Sigil was already known to degrade at `VOICE_NUM_CTX=4096`.** This
may be the same window pressure escalating from a quality defect to a hard parse failure on the
largest prompts. Suggestive, unproven, and the reason (b) rides **7.11 / window sizing** rather than
being patched in the parser.

---

### D-T29 — **TARGET: EVERY JUNCTION RUNS AT A 4096 CONTEXT WINDOW** (Scott, 2026-08-07)

> ## ✅ DEPLOYED 2026-08-08 16:03 EDT @ `39db36ee9d45` — the hold below is DISCHARGED
>
> **It read:** *"STAGED, COMMITTED, NOT DEPLOYED — do not build into `rust/bin/` before Sat
> 2026-08-08 10:55 EDT"*, per Scott 2026-08-07 *"Don't deploy until after Saturday"*, because the
> `.path` watcher restart would land inside 8.7's 48 h window alongside its two existing confounds
> (the flip; D-T21's cap arming at 02:00 Aug 7).
>
> **8.7 closed 10:55 and the binary shipped at 16:03**, verified by the journal boot stamp
> `commit="39db36ee9d45" built="2026-08-08T20:01:56Z"`. Deployed narrowly — `cargo build --bin
> scoracle-cognition`, staged outside `rust/bin/` and atomically renamed in (a plain `cp` over the
> running binary fails `ETXTBSY`) — so no Go binary or API restart rode along.
>
> **The pre-change baseline this hold names — gemma3:4b resident at 5.3 GB of 8 GB at 8192 — was
> read live at deploy time and still held**, because the runner had reloaded off the accidental 4096
> hours earlier. **The VRAM before/after is therefore real: measure ~4.99 GB at the 02:00 drain**,
> which is the next local call. Wall-clock is expected FLAT (`num_ctx` is memory, not compute).

**Scott's instruction:** *"we can move to 4096 ctx for both the 1070 characters (Editor +
Investigator) … the target is for all junctions to be working with a 4096 ctx window. That will
improve speed."*

**DONE — the 1070 Ti half.** `ARTICLE_NUM_CTX` and `EDITOR_NUM_CTX` both 8192 → **4096**. One
constant per host is deliberate: Editor, `graph`, the Investigator and the Insider's identity
adjudication all read it, so the whole card moves together and ollama never reloads the runner. A
unit test (`editor_num_ctx_matches_the_shared_runner`) pins that agreement and **caught the change
when only one of the two constants had moved** — the guard works.

**Sized on measurement.** Tokens counted through gemma3's own tokenizer (the Editor's largest 24h
prompt, 9,731 chars, tokenized to **2,049** — i.e. 4.75 chars/token, not the ~3–4 I first guessed):

| local stage | max prompt (72h) | + num_predict | worst case | headroom in 4096 |
|---|---|---|---|---|
| `editor` | 10,185 chars ≈ 2,144 tok | 900 | **3,044** | 1,052 (26%) |
| `graph` | 3,441 chars ≈ 724 tok | 768 | **1,492** | 2,604 (64%) |

Why it is a real speedup on this card and not just tidiness: the 1070 Ti is Pascal (compute 6.1,
**no tensor cores**) and bandwidth-bound at 256 GB/s. At an 8192 window gemma3:4b was resident at
**5.3 GB of 8 GB with only ~2.9 GB free**. Halving the window halves the KV allocation and the
bytes attention sweeps per token — which is the axis this card is actually limited on.

---

##### THE OTHER HALF OF THE TARGET IS **NOT** MET, AND THE MEASUREMENT SAYS SO

The voices on the Mac are **already** nominally at 4096 on the packet rail
(`route::VOICE_NUM_CTX_PACKET`). Their prompts are not:

| voice stage | max prompt (72h) | ≈ tokens | vs 4096 |
|---|---|---|---|
| `narratives` | 35,975 chars | **≈ 7,574** | **~1.8× OVER** |
| `vibe` | 30,576 chars | **≈ 6,437** | **~1.6× OVER** |
| `momentum` | 12,040 chars | ≈ 2,535 | fits |
| `sigil` | 9,010 chars | ≈ 1,897 | fits |
| `transfers` | 5,314 chars | ≈ 1,119 | fits |
| `rating` | 3,433 chars | ≈ 723 | fits |

**`route::VOICE_NUM_CTX`'s own doc predicted this exact diagnostic:** *"a voice that still needed
16384 on the packet rail would mean the render or the memory block had quietly grown back."*
**It has — for `narratives` and `vibe`.** And the failure mode is the silent one that constant
documents: when prompt + `num_predict` exceeds the window, **the system prompt is evicted
mid-generation**, with no error and no dead-letter. It degrades quality invisibly.

**So D-T29's remaining work is NOT a config change — it is a DIET.** `narratives` and `vibe` need
their packet render / memory block measured and cut until they fit 4096. Raising the window instead
would abandon the §7 envelope and put the thrash back.

*(Token counts for the Mac voices are scaled from the gemma3 ratio and are indicative, not
tokenizer-exact — ministral tokenizes differently. The conclusion survives a wide margin: even at
5.0 chars/token `narratives` is ≈7,195 tokens, still ~1.75× over. **Confirm exactly with
ministral's tokenizer before sizing the cut.**)*

**One thing this measurement also RULES OUT:** `sigil`'s prompts max at ≈1,897 tokens, less than
half the window. **So D-T28(b)'s NBA-team crown failures are NOT context overflow** — that
hypothesis is dead, and the sigil cause is still open. Recorded so it is not re-walked.

---

### D-T30 — ~~MAC CONCURRENCY IS SET TO 1~~ **IT IS SET TO 2. CORRECTED 2026-08-08 17:30 EDT.**

> ## ⚠ THE TITLE BELOW IS WRONG AND THE STEP PLAN IS HALF-DONE ALREADY
>
> **Read from the LIVE process rather than from recollection** (`ps eww` on the running
> `ollama serve`, PID 63533, up since Aug 1):
>
> ```
> OLLAMA_NUM_PARALLEL=2      OLLAMA_MAX_LOADED_MODELS=1   OLLAMA_KEEP_ALIVE=24h
> OLLAMA_FLASH_ATTENTION=1   OLLAMA_KV_CACHE_TYPE=q8_0    OLLAMA_CONTEXT_LENGTH=16384
> ```
>
> **`OLLAMA_NUM_PARALLEL` is 2, not 1.** So D-T30's plan — *"1 → 2, measure, then consider 4"* —
> **has already had its first step taken.** The remaining move is **2 → 4**, and the "client sends 3
> to a server that runs 1" framing below is wrong: the server runs 2.
>
> **This is the SECOND label-vs-observation error found on 2026-08-08** (the first: the phantom
> archbox mirror, struck above). Both came from reading an env file or a memory instead of the
> running system. **T2 applies to our own instrumentation, not just to the models.**
>
> ##### The live llama.cpp runner flags, which settle several open questions at once
>
> ```
> llama-server -c 8192 -np 2 --cache-type-k q8_0 --cache-type-v q8_0 --flash-attn on
>              --context-shift --keep 4 -b 512 -ub 512
> ```
>
> * **`-c` is TOTAL context across slots, so 8192 / `-np 2` = 4096 PER SLOT** — which is exactly what
>   `ollama ps` reports. **This confirms the client's per-request `num_ctx=4096` is being honoured
>   and that KV scales as `num_ctx × slots`**, the relation §7's budget assumes.
> * **Therefore the arithmetic for TARGET 2 is direct:** halving voice `num_ctx` to 2048 would fund
>   `-np 4` at the SAME total KV. **That is the concrete form of "lower ctx → more slots → throughput"
>   on this host**, and it is the cheapest version of D-T30 available.
> * **`--context-shift` IS ON, with `--keep 4`.** ⚠ **This is the live mechanism for D-T29's silent
>   degradation.** `narratives` (≈7,574 tok) and `vibe` (≈6,437 tok) **exceed the 4096-per-slot window
>   right now**, so their prompts cannot fit and something is being discarded with no error and no
>   dead-letter. **NOT YET MEASURED — do not assert the exact mechanism** (truncate vs shift) **until
>   the `prompt_eval_count` test is run against an over-long prompt.** But that the two largest voices
>   are being cut is arithmetic, not conjecture.
>
> ##### ⛔ AND THE 2→4 MOVE WAS MEASURED 2026-08-08 18:14 EDT. **IT IS A REGRESSION. DO NOT SHIP IT.**
>
> **Run on a throwaway `ollama serve` on port 11435 with `OLLAMA_NUM_PARALLEL=4` (production config
> untouched), same prompt/settings as D-T34:**
>
> | | `-np 2` (production) | **`-np 4`** |
> |---|---|---|
> | single-stream decode | 11.8 tok/s | **10.4 tok/s** |
> | aggregate @ 2 concurrent | 16.7 tok/s | **10.1 tok/s** |
> | aggregate @ 4 concurrent | 16.5 tok/s | **11.4 tok/s** |
>
> **Every cell got worse.** The runner came up `-c 16384 -np 4` (4 slots × 4096) and resident rose
> **8.83 GB → 9.54 GB**, with free pages down to ~66 MB and swap active on a 16 GB box.
>
> **THE SIZING PREDICTION WAS RIGHT AND THE THROUGHPUT PREDICTION WAS WRONG.** The entry below
> predicted *"4 slots already fit at 9.74 GB"* — measured **9.54 GB**, essentially exact. **They fit.
> They just don't help.** On a bandwidth-bound M4 the larger KV costs more memory traffic per token
> than the extra slots recover, so *"budget 2–2.5× aggregate"* is not achievable this way.
>
> **THIS DOES NOT KILL THE THROUGHPUT GOAL — IT RELOCATES IT.** Two live routes remain, and D-T34
> measured one of them working:
> 1. **MLX reached 35.2 tok/s at 4 concurrent in the SAME 16 GB** where llama.cpp at 4 slots managed
>    11.4. **The concurrency win is available; llama.cpp is simply not the engine that delivers it
>    here.** → D-T34.
> 2. **Cut `num_ctx`, THEN add slots** — 4 slots at 2048 costs the same total KV as today's 2 at 4096.
>    ⛔ **But see D-T35: the prompts must be TRIMMED before the window is lowered, or that trade buys
>    throughput by silently corrupting output.**
>
> *(Caveat: the `-np 2` figures were taken ~10 min earlier under slightly different memory
> conditions. The direction is far too large to be that, but one re-verification is owed.)*
>
> *(Original entry follows, kept for its Mac-side sizing which is unaffected.)*

### D-T30 (original text) — **MAC CONCURRENCY IS SET TO 1 AND THE CLIENT ALREADY SENDS 3** (measured 2026-08-07)

**Scott's observation:** *"only two of our characters use more than 4096 tokens. Once we tune the
ctx window to keep everything under 4096, we should be able to unlock concurrency on Mac. That
should dramatically speed up our output on those."*

**The conclusion is right and the payoff is real. The MECHANISM is not the prompt diet** — and the
difference matters, because the actual blocker is available to fix right now and the diet would
never have reached it.

**Measured on the voice host itself (192.168.1.77, confirmed by `ipconfig`):**

| | |
|---|---|
| unified memory | **16 GB** |
| `ministral-3:14b` resident | **8.8 GB, 100% GPU, CONTEXT 4096** |
| `OLLAMA_NUM_PARALLEL` | **1** ← the constraint |
| `OLLAMA_MAX_LOADED_MODELS` | 1 |
| client setting | `COGNITION_BACKEND_CONCURRENCY=…192.168.1.77=3` |
| decode rate | **12.3 tok/s** (vs 52.5 tok/s for gemma3:4b on the 1070 Ti) |

**The client sends up to 3 concurrent calls to a server configured to run ONE at a time.** The other
two queue at ollama. That is the unlock Scott is reaching for, and it is an env change on the Mac —
**not a prompt change, and not a deploy** (it does not touch `rust/bin/`, so no `.path` watcher).

**Why the diet does NOT unlock it.** `num_ctx` on the Mac is ALREADY 4096. KV is allocated from the
WINDOW, not from the prompt — so a 7,574-token `narratives` prompt and a 2,000-token one cost the
same memory. The oversized prompt is a **correctness** problem (silent system-prompt eviction) and
a small wasted-prefill problem. It is not a memory problem, so trimming it frees nothing.

**But Scott's instinct is right in a subtler way, and this is the part worth keeping.** Raising
parallelism multiplies KV: ollama allocates `num_ctx × slots`, so 3 slots at 4096 ≈ **12,288 tokens
of KV**. On 16 GB unified (macOS makes ~12 GB available to Metal by default) against 8.8 GB of
resident weights, that is affordable — **but only while the window stays at 4096.** If anyone
"fixed" `narratives`/`vibe` by RAISING the window instead of trimming them, they would spend exactly
the memory the extra slots need. **So the diet does not create the concurrency headroom — it
PROTECTS it.** Trim the prompts; never raise the window.

**Expected payoff, and why it is the biggest one on the board:** the voice tier decodes at
**12.3 tok/s, ~4× slower per token than the Editor's model**, and 87% of model time is decode
(§7b). Six voices serialised through one slot is the deepest queue in the system. Going 1 → 2 slots
is the single largest available throughput change, and it costs one environment variable.

**NOT DONE — same hold as D-T29.** Voice throughput is one of 8.7's watched metrics, and its window
closes **Sat 2026-08-08 10:55 EDT**. Changing parallelism now adds a fourth confound to a reading
that already carries three. **After Saturday:** raise to **2 first**, measure, then consider 3 —
`OLLAMA_MAX_LOADED_MODELS=1` must stay 1 (a second resident model would evict the 8.8 GB incumbent).

---

##### HOW FAR CAN IT GO? **4 SLOTS ALREADY FIT — THE KV UNLOCK IS ALREADY INSTALLED**

Scott asked what it would take to reach 4 concurrent. **Nothing new: `OLLAMA_FLASH_ATTENTION=1` and
`OLLAMA_KV_CACHE_TYPE=q8_0` are ALREADY set on the Mac** — that is exactly the lever that would
otherwise have been needed, and it is on.

Computed from the model's own architecture (`/api/show`: `block_count=40`,
`head_count_kv=8`, `key_length=value_length=128`, 13.9B @ Q4_K_M), not estimated:

**KV per token = 2 × 40 × 8 × 128 × bytes** → **160 KB/token at f16, 80 KB at q8_0** →
**0.312 GB per 4096-token slot** with q8_0 active.

| slots | KV | total resident | vs ~10.7 GB budget |
|---|---|---|---|
| **1 (today)** | 0.31 GB | **8.80 GB** | matches the observed figure — the model is validated |
| 2 | 0.62 GB | 9.11 GB | fits |
| 3 | 0.94 GB | 9.43 GB | fits |
| **4** | 1.25 GB | **9.74 GB** | **fits, ~0.9 GB spare** |

*(Weights + buffers back-solved at 8.49 GB from the observed 8.8 GB at 1 slot, so the table is
anchored to a measurement rather than to a spec sheet. macOS budget is the default —
`iogpu.wired_limit_mb = 0`, untouched — which on a 16 GB machine is ~10.7 GB.)*

**So the only thing between the rail and 4 concurrent voices is `OLLAMA_NUM_PARALLEL=1`.** No
quantization change, no `sysctl`, no hardware.

**Three caveats, all of which argue for stepping rather than jumping:**
1. **4 slots is NOT 4× output.** One GPU, and a 14B on Apple Silicon is memory-bandwidth-bound —
   batching raises AGGREGATE throughput while each stream slows. **Budget 2–2.5× realistic.**
   Measure at 2 before going further.
2. **This is Scott's working Mac, not a server.** 9.74 GB wired on a 16 GB machine leaves the OS and
   his applications to share the rest. Sluggishness is the trade, and it is a second reason to walk
   1 → 2 → 4.
3. **It hard-couples to the 4096 window.** KV is `num_ctx × slots`, so at 4 slots a window increase
   costs FOUR times as much: `num_ctx` 8192 at 4 slots ≈ **11.0 GB — over budget.** This is the
   sharpest form of the D-T30 rule: **trim the prompts, never raise the window.**

---

### D-T31 — **THE EDITOR MOVES TO `ministral-3:3b`. SCOTT'S DECISION, 2026-08-07, ON A MEASURED WIN.**

**Scott:** *"we're going to be switching to Mistral 3b. Massive wins in a system already designed
for Gemma."* **The adoption is a human editing `COGNITION_ROUTE_EDITOR`, exactly as the router's
eval discipline requires — never an auto-promote.**

**THE MEASUREMENT (2026-08-07 ~18:55 EDT, `eval --task editor --fixtures`, temp 0, 12 fixtures /
53 property checks, `scoracle-cognition` STOPPED per the D-T19 determinism rule):**

| model | property checks passed |
|---|---|
| `gemma3:4b` (incumbent) | **47/53** |
| **`ministral-3:3b` (candidate)** | **52/53** |

**The incumbent reproduced its documented 47/53 baseline EXACTLY.** That is the result that makes
the other one trustworthy — it independently re-validates both the fixture harness and D-T19's
"daemon stopped or the gate is invalid" finding, on a different day and a different session.

**Where the win actually is — the `names[]` discovery channel.** On `result-line-verbatim-score`:
* `gemma3:4b` emitted `names=[Real Madrid]` and **FAILED** `name_found[Bellingham]`.
* `ministral-3:3b` emitted `[Real Madrid, Arsenal, Jude Bellingham, Bukayo Saka, Vinicius Junior,
  Mikel Arteta]` and passed.

`names[]` is the Investigator's nomination source (§1a: *"the discovery channel"*), so this is not a
cosmetic scoring difference — it is directly the **§6a `names[] coach/manager class`** defect that
has been carried as an open model-quality item. A richer `names[]` feeds entity discovery downstream.

Model facts, read from `/api/show` rather than the tag: **3.8B params** (the `:3b` tag understates
it), 26 layers, 8 KV heads, Q4_K_M, Apache-2.0, multimodal — and the **same Ministral 3 family the
six voices already run at `:14b`**, which is an argument on its own for prompt-formatting
consistency across the rail.

---

##### ⛔ ORDERING CONSTRAINT — **THE 4096 BINARY MUST DEPLOY BEFORE THE MODEL SWITCH. NOT WITH IT. BEFORE IT.**

**This is the one that breaks production if it is missed.** `EDITOR_NUM_CTX` in the **currently
deployed** binary is still **8192**. Archbox runs `OLLAMA_NUM_PARALLEL=4` and has **no**
`OLLAMA_KV_CACHE_TYPE` set, so its KV is **f16**. Computed from `ministral-3:3b`'s own architecture:

| window | KV (f16, ×4 slots) | est. total resident | on an 8 GB card |
|---|---|---|---|
| **4096** | 1.62 GB | **~6.0 GB** *(observed live — matches)* | comfortable |
| 8192 | 3.25 GB | **~7.65 GB** | **~0.35 GB margin — spills** |

**So flipping `COGNITION_ROUTE_EDITOR=ministral-3:3b` while the deployed binary still asks for 8192
would put the model at the edge of the card and spill it to CPU — which is not a small regression,
it is an order-of-magnitude one.** The correct order, after 8.7 closes:
1. **Deploy the D-T29 4096 binary FIRST.** Confirm `ollama ps` shows the Editor's model at
   `CONTEXT 4096`.
2. **THEN** edit `COGNITION_ROUTE_EDITOR=ministral-3:3b` in archbox `.env.local` and restart.
3. Confirm resident size ≈6.0 GB and that gemma3:4b has been evicted (`MAX_LOADED_MODELS=1`).

**⚠ STEP 1'S VERIFY IS NOW BLIND — the live runner ALREADY reads `CONTEXT 4096`.** See the finding
immediately below. `ollama ps` will show 4096 both before and after the deploy, so it cannot confirm
the deploy landed. **Verify from the binary instead:** `scoracle-cognition starting … built=` in the
journal must postdate `d4c80a0` (2026-08-07 10:03 EDT).

---

##### THE EDITOR IS ALREADY RUNNING AT 4096 — BY ACCIDENT, NOT BY DEPLOY (measured 2026-08-08 ~01:12 EDT)

**Found while checking readiness for the swap. It does not change the order; it changes what the
post-deploy measurement is allowed to mean.**

| | |
|---|---|
| deployed binary | commit `6fbf798`, built **2026-08-06 19:32Z** — predates D-T29 (`d4c80a0`, Aug 7 10:03) |
| what that binary requests | `EDITOR_NUM_CTX = 8192` (confirmed via `git show 6fbf798:…/editor/mod.rs`) |
| what the live runner reports | `gemma3:4b`, **`context_length 4096`**, **4.99 GB**, `expires_at` **2318** |

**The mechanism.** Archbox's ollama unit sets **`OLLAMA_KEEP_ALIVE=-1`**. Nothing in the Rust tree
sets `keep_alive` (grepped — zero hits), so the pin is server-side. **The D-T31 eval run of
2026-08-07 ~18:55 used the `target/debug` binary, which carries the 4096 constants** — it loaded a
4096 runner, and `KEEP_ALIVE=-1` pinned it permanently. Production's 8192 requests have not forced a
reload in the ~6 h since. The **4.99 GB** resident size corroborates a genuine 4096 runner: D-T29
recorded gemma3:4b at **5.3 GB** at 8192.

**PREDICTION BANKED BEFORE THE MEASUREMENT, which is the only reason it is worth writing down:**
> **The D-T29 deploy will produce NO measurable Editor speedup.** The 4096 window is already in
> effect at runtime. **A flat post-deploy wall-clock is the EXPECTED result, not a failed change** —
> do not rationalise it afterwards, and do not go looking for a second knob to explain it.

**The deploy is still required, and this finding is the sharpest argument for it.** Today 4096 is an
accident held by a pinned runner. **Any event that reloads that runner restores 8192** — a manual
`ollama stop`, an ollama restart, a host reboot. With `ministral-3:3b` adopted, that reload is
D-T31's **~7.65 GB on an 8 GB card, i.e. the spill.** The deploy converts an accident into the
binary's intent and closes the hole permanently. **The ordering constraint is not merely still
correct — this is WHY it is correct.**

*(Side effect worth knowing: `OLLAMA_KEEP_ALIVE=-1` means any eval run from `target/debug` leaves its
window pinned on the live card. The D-T19 rule — stop `scoracle-cognition` before the gate — keeps
the gate honest, but it does not un-pin what the gate loaded.)*

##### ⛔ THE PREMISE ABOVE EXPIRED BEFORE THE DEPLOY — MEASURED 2026-08-08 16:05 EDT

**The accident did not survive to Saturday's deploy, and the banked prediction is void because its
premise is.** Read at the deploy, `ollama ps` returned:

```
gemma3:4b   5.3 GB   100% GPU   CONTEXT 8192   Forever
```

**8192 at 5.3 GB — D-T29's documented pre-change baseline exactly**, not the 4096/4.99 GB recorded
at 01:12. **The pinned 4096 runner was reloaded, and production's 8192 requests restored 8192.**

**When, and by what.** Local `/api/generate` calls stop at **05:20 Aug 8** and there are none after
(the local card has been idle since; the Mac voices carried the day). The editor drain ran
02:00→05:20 Aug 8 under the **old `6fbf798` binary, which asks for 8192** — so the 02:00 ingest burst
is what reloaded it. The accident lived from the Aug 7 ~18:55 eval to ~02:00 Aug 8 — it was already
**~13 h dead when the binary was deployed at 16:03.**

**What this does and does not change:**
* **The ordering constraint is now proven by event, not by argument.** The plan said *"any event that
  reloads that runner restores 8192, and with `ministral-3:3b` adopted that reload is the spill."*
  **That reload happened.** Had the model been flipped without the binary, ministral would have
  loaded into an 8192 runner at ~7.65 GB on an 8 GB card. The order was not a precaution; it was the
  thing that saved it.
* **"No speedup" still stands — for the OTHER reason.** Not because the window was already 4096, but
  because `num_ctx` governs memory, not per-token compute (the correction at `92a63d6`). Flat
  wall-clock remains the expected result.
* **A real before/after on VRAM is now available and was not before:** 5.3 GB @ 8192 → ~4.99 GB @
  4096. **Not yet observed** — nothing local is queued (the editor queue is empty, see D-T32), so the
  first 4096 request is the **02:00 ingest drain**. Verify there.
* **`ollama ps` is a usable check again** — the "blind verify" note above applied only while the
  accident held. The journal `built=` stamp remains authoritative.

---

##### FLAGGED BEFORE THE DECISION, AND SCOTT DECIDED ANYWAY — SO IT IS A WATCH ITEM, NOT A BLOCKER

**The two models disagree on the taxonomy fields, and the fixture gate does not pin them:**

| fixture | `gemma3:4b` | `ministral-3:3b` |
|---|---|---|
| `result-line-verbatim-score` | `story_type=fixture`, `register=anticipation` | `story_type=performance`, `register=neutral` |
| `place-collision-paris` | `page_kind=article`, `story_type=fixture` | `page_kind=roundup`, `story_type=general` |

Both PASSED their checks — the fixtures assert relevance and names, not topic. But **`story_type`
and `register` are exactly what `routing_tags` derives from** (`bucket::routing_tags_from_story_type`
+ `derive::routing_tags`), and routing tags decide **which voices wake**. So this swap can shift the
tag distribution across the whole rail **without the gate registering anything**.

**Therefore, after the switch, MEASURE THE TAG DISTRIBUTION, not just the score.** The
before-picture is already banked (D-T22 pass 3): packets carry `fixture` 4,693 · `charged` 2,237 ·
`roster` 2,203 · `transfer` 1,831 · `performance` 1,737 · `general` 1,655 · `injury` 349. A large
move in that mix after adoption is a finding, not noise — and `injury` is the one to watch hardest,
since **nothing subscribes to it yet (D-T25)** and a change there would be silent.

##### ✅ THE BASELINE IS RE-BANKED POST-CAP, PRE-FLIP — 2026-08-08 16:30 EDT, gemma3:4b

**Banked to unblock the flip: D-T32's cap made the D-T22 numbers unusable as a comparator, and this
replaces them.** Method, so the after-picture uses the same one:
`unnest(packets.routing_tags)` grouped by `packets.compiled_at::date`.

**FIRST, THE METHOD DEFECT IN THE OLD BASELINE.** D-T22 pass 3 was an **ALL-TIME CUMULATIVE** count,
not a windowed one — re-running it today gives `fixture` 6,173 · `charged` 3,241 · `roster` 2,993 ·
`transfer` 2,602 · `performance` 2,382 · `general` 2,054 · `injury` 534 · **`contract` 459**. Two
consequences: **a cumulative total barely moves when new packets arrive, so it could never have
detected the shift it was banked to detect**, and **it omits `contract` entirely — the taxonomy has
EIGHT routing tags, not the seven on record.**

**THE USABLE INSTRUMENT IS SHARE-PER-DAY, AND IT IS STABLE ACROSS THE CAP BOUNDARY:**

| tag | Aug 6 (pre-cap) | Aug 7 (post-cap) | Aug 8 (post-cap) |
|---|---|---|---|
| `fixture` | 30.8% | 26.7% | 31.0% |
| `charged` | 15.3% | 19.0% | 16.1% |
| `roster` | 14.8% | 14.4% | 12.6% |
| `transfer` | 12.1% | 15.0% | 16.0% |
| `performance` | 11.8% | 11.2% | 10.5% |
| `general` | 10.8% | 6.5% | 8.6% |
| `injury` | 2.5% | 3.7% | 1.6% |
| `contract` | 1.9% | 3.5% | 3.6% |
| *packets compiled* | *9,914* | *1,238* | *523* |

**This is the finding that matters for sequencing: packet VOLUME fell ~8×, but the SHARES held to a
few points.** So **normalising to share largely neutralises the cap confound** — which means the
swap does NOT have to wait for the cap ruling, as long as the comparison is share-based and never
count-based. **The hold on D-T31 can be lifted on this instrument.**

**Two honest limits on it:**
* **`injury` is too small to read per-day at current volumes** (113 → 17 packets; 3.7% → 1.6% is
  ~20 packets of noise). **Pool several days before calling any `injury` move**, which is awkward
  precisely because it is the tag that most needs watching (D-T25).
* **`general` is the one real pre-existing move** (10.8% → 6.5%/8.6%) and it happened WITHOUT a model
  change, so some share drift is native to the cap/volume shift. Treat a post-swap move of that size
  as suggestive, not conclusive.

**`story_types` mirrors `routing_tags` exactly except `charged` is absent from it** — so `charged` is
a derived routing tag, not a story type. **`register` is EMPTY on ~75% of packets** (Aug 6: 7,431 of
9,914 blank; then `anticipation` 1,317 · `celebration` 521 · `outrage` 508 · `resignation` 137).
**That blank majority is itself worth a look** — `register[outrage]` reading neutral is already a
known 7.11 defect, and a three-quarters-empty field is a weak signal to route on.

##### ⚠ THE TOKENIZER IS DENSER — MEASURED 2026-08-07, AND IT CUTS BOTH WAYS

Scott expected more token speed from the swap. **Half right, and the other half tightens D-T29.**
Identical prompt, identical `num_ctx`/`num_predict`, same card, back to back:

| model | prefill | decode |
|---|---|---|
| `ministral-3:3b` | **2,705 tok** in 2.92 s (926 tok/s) | **59.2 tok/s** |
| `gemma3:4b` | **2,049 tok** in 1.68 s (1,221 tok/s) | 52.5 tok/s |

1. **Decode IS faster: 59.2 vs 52.5 tok/s, +13%.** Scott's instinct confirmed on the axis that
   carries 87% of model time (§7b).
2. **But ministral tokenizes the SAME TEXT into 32% MORE TOKENS** (2,705 vs 2,049). That is a
   tokenizer property, not a prompt change — the input was byte-identical. It shows up immediately
   as a **74% slower prefill on this prompt** (2.92 s vs 1.68 s), and it almost certainly costs
   extra OUTPUT tokens to express the same ep1 JSON.

**So the net wall-clock is NOT a clear win and may be a wash.** Rough arithmetic on a typical call
(1,476 gemma-tokens of prompt, 419 out): gemma ≈9.7 s; ministral ≈10.0 s once the denser tokenizer
is applied to both ends. **Do not promise a speed improvement from this swap.** The measured,
defensible reason to adopt is **QUALITY (52/53 vs 47/53) and the `names[]` discovery win** — the
speed question stays open until real Editor `eval_count` is observed under ministral in production.

**AND IT TIGHTENS THE 4096 WINDOW — D-T29's arithmetic was computed on gemma's tokenizer:**

| model | max observed prompt | + `num_predict` 900 | headroom in 4096 |
|---|---|---|---|
| `gemma3:4b` | 2,049 tok | 2,949 | 1,147 (**28%**) |
| **`ministral-3:3b`** | **2,705 tok** | **3,605** | **491 (12%)** |

**It still fits — but on 12% margin, not 28%.** `ARTICLE_MAX_MODEL_CHARS = 9_000` is what holds the
prompt down, so that constant is now load-bearing for the window and **must not be raised without
redoing this table on ministral's tokenizer.** A 2048 window is now firmly out of reach: the max
prompt alone (2,705) exceeds it.

##### CHEAP FOLLOW-UP SPOTTED WHILE MEASURING

**Archbox does NOT set `OLLAMA_KV_CACHE_TYPE=q8_0` or `OLLAMA_FLASH_ATTENTION=1` — the Mac DOES.**
Mirroring the Mac's config on archbox would halve local KV (1.62 GB → 0.81 GB at 4096×4), buying
back most of what the model swap costs. Untested on this card; Pascal's flash-attention support is
the thing to verify first. **Not done, not assumed to work — logged as a candidate.**

**CONFIRMED 2026-08-08 ~01:12 EDT** by reading the unit file directly, so the candidate above rests
on a measurement rather than a recollection. Archbox's ollama unit carries exactly three settings:
`OLLAMA_NUM_PARALLEL=4`, `OLLAMA_KEEP_ALIVE=-1`, `OLLAMA_MAX_LOADED_MODELS=1`. **No
`OLLAMA_KV_CACHE_TYPE`, no `OLLAMA_FLASH_ATTENTION`** — the KV is f16, as D-T31's spill table assumed.

---

##### D-T30's FINDING HAS A MIRROR ON ARCHBOX — AND IT IS ON THE STAGE §0a CALLS "THE NUMBER TO PROTECT"

**Measured 2026-08-08 ~01:12 EDT, same readiness check.**

| | |
|---|---|
| archbox ollama server | **`OLLAMA_NUM_PARALLEL=4`** — can serve four at once |
| Scoracle client cap, archbox `.env.local` | **`OLLAMA_MAX_CONCURRENT=1`** ← sends one at a time |
| what D-T19's handoff recorded | `max_concurrent 4` — **the handoff and the live env disagree** |

**This is D-T30 inverted.** On the Mac the client sends 3 to a server that runs 1. **On archbox the
server can run 4 and the client sends 1.** Both are one-line env changes, and this one lands on the
**Editor** — the stage §0a identifies as running at ~96% of ingest with *no headroom to burn down a
backlog, absorb a re-read, or take a prompt that costs 20% more.*

**NOT CHANGED, and deliberately so.** It is a second behaviour change, and D-T31's swap is already
the one change this window gets (§0 rule 4). Sequence it AFTER the model swap has its reading, or the
two are unattributable. **Also settle the disagreement rather than assuming the env is right** —
D-T19's handoff may be describing a value that was since changed, or may simply have been wrong; the
1070 Ti's 8 GB and f16 KV at 4 slots is the arithmetic that decides whether 4 was ever safe.

##### ✅ SETTLED 2026-08-08 16:04 EDT — **D-T19's HANDOFF WAS RIGHT AND THE TABLE ABOVE IS WRONG. THERE IS NO ARCHBOX MIRROR.**

**Read from the daemon's own boot line rather than from the env file, which is what the row above got
wrong.** At the D-T29 restart:

```
ollama reachable base_url=http://192.168.1.77:11434  max_concurrent=3
ollama reachable base_url=http://localhost:11434     max_concurrent=4
```

**The client sends FOUR locally, against a server running `OLLAMA_NUM_PARALLEL=4`. Client and server
agree; there is nothing inverted and nothing to fix.**

**Where the error came from:** the Rust client does not read `OLLAMA_MAX_CONCURRENT`. It reads
**`COGNITION_BACKEND_CONCURRENCY`**, live in archbox `.env.local` as
`"http://localhost:11434=4,http://192.168.1.77:11434=3"`. `OLLAMA_MAX_CONCURRENT=1` **is also in that
file and is inert for this client** — grepping the env for a plausible-looking name found a variable
nothing reads. **The lesson is the one T2 keeps making: read the observation (the boot line), not the
label (the env var).**

**Consequence: the "archbox mirror of D-T30" is struck from the queue** — it was an artifact of the
misread, and the Editor is not being throttled to one call at a time. **D-T30 on the Mac is
unaffected and remains real** (that one was measured on the host, not inferred from an env file).
*(`ARTICLE_NUM_CTX`'s own doc already recorded local concurrency as 4 — "`COGNITION_BACKEND_CONCURRENCY=localhost=4`",
effective parallelism ~1.85× — so the tree and the boot line agreed all along; only this table did not.)*

---

### D-T32 — **D-T21'S CAP AND §2 CLAUSE 1 CANNOT BOTH HOLD. THE EDITOR IS AT 100% OF WHAT IT IS ASKED; THE CAP SHRANK THE ASK BY 81%.** (measured 2026-08-08 15:50 EDT)

**Found by running step 1 of the Saturday deploy — `rail-cutover-check.sh`, no `DAY` override, i.e.
8.2 day 1. It came back FAIL and the session stopped on it (§0 rule 3) before deploying anything.**

| clause | reading | verdict |
|---|---|---|
| 1 · coverage ≥95% | **921 / 4,813 = 19.1%** | **FAIL** |
| 2 · packet presence | 206 / 206 legacy entity-days, 0 missing | PASS |
| 3 · precision | 50-link sample emitted | **UNSCORED** (needs a human) |
| 4a · dead-letters | 0 | PASS |
| 4b · fixture gate | `rust/bin/eval` not present | NOT RUN |
| 5 · accounting | 57,557 claims, 0 orphans | PASS |

*(context: 1,238 packets compiled, 2,257 editor reads, 543 storylines opened)*

##### THE COLLAPSE IS REAL, MONOTONIC, AND NOT A LATENCY ARTIFACT

| day | articles | read within 24h | coverage |
|---|---|---|---|
| Aug 3 | 6,905 | 6,904 | **100.0%** |
| Aug 4 | 7,985 | 7,984 | **100.0%** |
| Aug 5 | 8,358 | 8,132 | 97.3% |
| Aug 6 | 9,628 | 8,155 | 84.7% |
| **Aug 7** | **4,813** | **921** | **19.1%** |

**Checked before theorising: "read EVER" equals "read within 24 h" — both 921.** The missing 3,892
were not read late, they were **never read**. So it is not the mid-drain sampling artifact that
`ARTICLE_NUM_CTX`'s doc comment offers ("a low clause-1 reading means the sample was taken
mid-drain, not that the Editor is starved") — **that sentence is now false and should be corrected
when the constant is next touched.**

##### THE CAUSE IS D-T21'S CAP, AND THE ARITHMETIC CLOSES TO THE ARTICLE

`logs/pipeline-ingest.log` logs the bite directly —
`msg="editor read cap reached" enqueued=10 withheld=N cap_per_entity_day=10`:

| day | entities capped | withheld | + read | = arrivals |
|---|---|---|---|---|
| Aug 6 | **0** | 0 | — | — |
| Aug 7 | 110 | 3,892 | 921 | **4,813 ✓ exact** |
| Aug 8 (to 15:50) | 121 | 4,493 | 1,085 | **5,578 ✓ exact** |

**Two days, two exact identities. The cap accounts for 100% of the coverage miss — there is no
residual for anything else to explain.** Aug 6 shows `entities_capped=0`, so the cap's first bite is
the **02:00 Aug 7 cron**, exactly where the curve breaks. (Aug 6's own 84.7% is the same cap reaching
backwards: its late-arriving tail was enqueued by that Aug 7 run and capped there.)

**Measured against what the Editor was actually ASKED to do, it read 921 of 921 = 100.0%.** Nothing
is stuck: `pipeline_work` holds **zero `editor` rows**, 0 due and 0 deferred; no dead-letters; the
harness is healthy and drained everything handed to it. **The Editor is not starving. It is idle
because the queue is empty by design.**

**The pre-cap baseline predicted this and named its own falsifier — it has been SCORED, in the
one-rail phase 8 Log under "8.2 DAY 1".** Short version: `p50` held at 2 and the tail trimmed as
intended (272 → 111 → 24 entities over 10), **but the volumes it said must not fall, fell** (entity-
reads 16,077 → 5,966 → 2,039), which is verbatim the condition the baseline called *"the cap is
over-reaching, and that is a finding."* **One trap recorded there: `max` does NOT clamp to 10 (101 on
Aug 7), because the cap bounds ENQUEUES per entity while that metric counts a read once per LINKED
entity — one enqueued article credits every entity it links to. Do not score `max` against the cap
and conclude it is broken.**

##### WHY THIS IS A RULING AND NOT A BUG REPORT

**Both things are working as configured, and they are incompatible:**
* **D-T21's cap** withholds at ENQUEUE, per entity per day, by Scott's chosen 10.
* **§2 clause 1** measures reads against articles ARRIVED, and needs ≥95%.

**While the cap is armed at 10, clause 1 is unreachable — so 8.2's "7 consecutive green days" can
never start, and the rail cannot close on a condition it is configured to fail.** The cap hits the
head of the distribution hardest (110 entities — the big clubs — withheld 3,892 between them), so
raising it slightly would not move coverage much; the shape of the fix matters more than the number.

**Scott's ruling, 2026-08-08: leave the cap at 10 and decide separately. One change, one
measurement.** The candidates, not chosen and not ordered:
1. **Redefine clause 1 against the QUEUE, not arrivals** — currently 100%. The 8.9 STATE already
   pointed here (*"do not trip 8.7's `<80%` rollback trigger on that ratio without defining coverage
   against the queue"*). Plan-file change only; makes the clause measure the Editor rather than the
   ingest policy.
2. **Raise or reshape the cap** — a production behaviour change, measured alone.
3. **Accept that 81% of arrivals are deliberately unread** and say so in §2, which is a product
   decision about how much of the feed the rail is meant to cover, not a tuning knob.

##### ⚠ IT ALSO CONFOUNDS D-T31, WHICH IS WHY THE MODEL FLIP WAS HELD

**The D-T31 before-picture (`fixture` 4,693 · `charged` 2,237 · `roster` 2,203 · `transfer` 1,831 ·
`performance` 1,737 · `general` 1,655 · `injury` 349) was banked at D-T22 pass 3 — PRE-CAP.** An
after-picture taken now would be drawn from a sample **81% smaller and reshaped toward the capped
head of the distribution**. A tag-mix move would be unattributable between the cap and ministral —
and `injury` (349, the one to watch hardest because nothing subscribes to it, D-T25) is the smallest
count and the most vulnerable to exactly this. **Scott held the flip on this basis, 2026-08-08.**
**Re-bank the before-picture post-cap before the swap, or the swap has no honest baseline.**

---

### D-T33 — **THE VOICE HOST WAS RUNNING TWO OLLAMA SERVERS. FIXED 2026-08-08 17:32 EDT.**

**Found while checking the Mac's serving stack for the MLX question (D-T34).**

| | |
|---|---|
| `/usr/local/bin/ollama serve` (PID 63533, PPID 1) | **holds :11434** — the configured server, correct env |
| `Ollama.app` (PID 63515, PPID 1) | retried the bind **forever**, never succeeded |
| `~/.ollama/logs/server.log` | **36 MB, 100% `bind: address already in use`** |

**They are independent processes** (both parented to launchd, started 2 s apart on Aug 1), which is
why stopping the GUI was safe — the CLI server and its `llama-server` child were untouched.

**Fix applied:** AppleScript quit was refused (`-128`), so a direct `TERM` to the GUI only; log
truncated in place. **Verified after: server ALIVE, runner ALIVE, `ministral-3:14b` still 8.76 GB
resident at 4096 ctx, `/api/tags` 200, and ZERO new log lines in 12 s.** The voices never noticed.

**⚠ IT WILL COME BACK.** The GUI is a **Login Item**, so it returns at next login and starts
spamming again. **Untick "launch at login" in Ollama.app's settings** — left for Scott deliberately
rather than poked at from a script.

*(Worth knowing for any future Mac work: `/usr/local/bin/ollama` is a symlink INTO the app bundle
(`/Applications/Ollama.app/Contents/Resources/ollama`), so the CLI and the GUI are the same build —
the conflict is over who runs the SERVER, not which binary.)*

---

### D-T34 — **MLX vs llama.cpp FOR THE VOICE TIER (Scott, 2026-08-08). EVALUATION, NOT A DECISION.**

> *Scott: "We're on Mac and using Ollama for the voice work. I think we should switch to oMLX as
> we're using Ministral which isn't on the Ollama list for MLX."*

**THE PREMISE IS CORRECT AND IS NOW CONFIRMED FROM THE RUNNING PROCESS, not from docs:** the voice
tier runs `/Applications/Ollama.app/Contents/Resources/**llama-server**` — i.e. **llama.cpp/Metal,
NOT MLX.** This ollama (0.32.4) *does* ship MLX internally (7,219 `mlx` symbol hits in the binary),
so the engine exists but is not serving ministral. No `mlx`/`mlx_lm` was installed on the host.

**AND THE MODEL EXISTS, so the path is viable:** `mlx-community/Ministral-3-14B-Instruct-2512-4bit`
is published, matching the voice tier's `ministral-3:14b` (13.9B, `mistral3`, Q4_K_M, vision,
Apache-2.0 → `Ministral-3-14B-Instruct-2512`).

##### WHY THIS IS NOT THE "QUICK WIN" IT LOOKS LIKE — THREE COSTS, ONE OF WHICH COULD MAKE IT A LOSS

1. **It is a PROTOCOL change, not a `BASE_URL` change.** The Rust client speaks **ollama's** API
   (`/api/generate`, `keep_alive`, `options.num_ctx`). `mlx_lm.server` speaks **OpenAI-compatible**.
   Switching means a second client path for the Mac host, or a shim.
2. **Per-request `num_ctx` is the control TARGET 2 depends on**, and it does not map cleanly onto
   mlx-lm's server. Losing it would cost the very lever the ctx work needs.
3. **⚠ CONCURRENCY IS THE ONE THAT COULD INVERT THE RESULT.** The throughput win is *via slots*
   (D-T30), so **a faster single stream on a runtime with weaker parallelism can still be LOWER
   aggregate throughput** — the exact metric being optimised. **This must be measured at concurrency,
   not just single-stream, or the benchmark will flatter whichever engine wins one request.**

##### THE BOUNDED EXPERIMENT (Scott authorised it 2026-08-08; run in the 18:00 GPU rest window)

**Constraint that shapes it: 16 GB unified, and the voice model already holds 8.76 GB.** An MLX 4-bit
14B is ~8 GB, so **the two cannot co-reside** — the comparison requires unloading the voice model,
which is why it runs inside the harness's own **18:00–19:00 pause** rather than against live traffic.

* **Fixed prompt, byte-identical to both engines**, seeded and sized to **8,735 chars — deliberately
  just under production's `ARTICLE_MAX_MODEL_CHARS = 9_000`.**
* **Both at `num_ctx` 4096, `num_predict`/`max_tokens` 300, temperature 0**, warmup then 3 runs,
  medians reported.
* **Measured on BOTH axes: single-stream prefill + decode tok/s, AND aggregate tok/s at 2 and 4
  concurrent.**
* **⚠ HONEST CAVEAT TO CARRY INTO THE RESULT: this is an ENGINE + QUANT comparison, not a pure engine
  one.** GGUF `Q4_K_M` (~4.5 bpw, mixed) and MLX `4bit` (group-wise) are different quantisations, so
  a small quality/size difference is baked in and neither side is a perfect control.

##### ✅ RESULT — MEASURED 2026-08-08 18:03–18:22 EDT IN THE GPU REST WINDOW. **SCOTT WAS RIGHT.**

**All figures: `ministral-3:14b` / `Ministral-3-14B-Instruct-2512-4bit`, 2,3xx-token prompt,
300 generated tokens, `num_ctx` 4096, temperature 0, M4 / 16 GB, voice model unloaded for the MLX
runs so nothing else was resident.**

| axis | llama.cpp (`-np 2`, production) | **MLX** | winner |
|---|---|---|---|
| prefill, **uncached** | **148 tok/s** | 126 tok/s | llama.cpp **+17%** |
| decode, single stream | 11.8 tok/s | **13.1 tok/s** | MLX **+11%** |
| **aggregate @ 2 concurrent** | 16.7 tok/s | **22.2 tok/s** | MLX **+33%** |
| **aggregate @ 4 concurrent** | 16.5 tok/s | **35.2 tok/s** | **MLX +113% (2.13×)** |

**SINGLE-STREAM IS A WASH. CONCURRENCY IS THE WHOLE STORY, AND IT IS SCOTT'S CALL THAT WAS RIGHT.**
Per-call wall clock for a 2,372-token prompt + 300 out works out **identical to the second**:
llama.cpp 16.0 s prefill + 25.4 s decode = **41.4 s**; MLX 18.5 s + 22.9 s = **41.4 s**. The engines
are indistinguishable on one request. **They are 2.13× apart when four are in flight**, which is the
regime production actually runs in.

**MLX SCALES; llama.cpp AT `-np 2` DOES NOT.** MLX went 13.1 → 22.2 → 35.2 as concurrency rose 1→2→4.
llama.cpp went 11.8 → 16.7 → **16.5** — flat from 2 to 4, because `-np 2` caps it. **The third
request the client already sends (`…1.77=3`) is queueing today.**

##### ⚠ THE COMPARISON UNDERSTATES MLX, FOR A REASON WORTH RECORDING

**The concurrency runs resent a byte-identical prompt, and the two engines cached it differently.**
llama.cpp's prefill collapsed to **0.09 s** (a full KV cache hit — that is where the absurd
"27,000 tok/s prefill" in the first run came from, and it is NOT a prefill measurement). **MLX's
server log shows it actually processing the prompt each time** (`Prompt processing progress:
2048/2328`). **So llama.cpp's aggregate numbers were achieved with prefill nearly free, while MLX's
were achieved while doing the prefill work. The real gap is wider than 2.13×, not narrower.**
*(The uncached 148 tok/s figure above was measured separately with per-request unique prefixes.)*

##### HONEST LIMITS ON THIS RESULT — none of them overturn it, all of them belong in the record

1. **Engine + quant, not pure engine.** GGUF `Q4_K_M` (~4.5 bpw mixed) vs MLX group-wise `4bit`.
2. **The `-np 2` aggregate figures were taken ~10 min before the `-np 4` ones**, under slightly
   different memory conditions. Worth one re-verification before this is treated as final.
3. **⚠ MLX's tokenizer emits a correctness warning:** *"incorrect regex pattern … will lead to
   incorrect tokenization. You should set `fix_mistral_regex=True`."* **Unresolved, and it is a
   QUALITY risk, not a speed one — it must be settled before any migration**, since the whole point
   of standardising on Mistral is prompt/tokenizer consistency.
4. **Synthetic prompt, not a rendered production packet.**
5. **An earlier run crashed** (`broadcast_shapes … (3,1,1,3) and (3,32,1,2329)`) — **that was memory
   exhaustion from running MLX beside the resident voice model, not a batch-path bug.** With the card
   to itself, MLX batched 4 requests cleanly. *(Recorded because the crash is in the session history
   and would otherwise look like evidence against MLX. It is not.)*

##### WHAT STILL BLOCKS ADOPTION — the numbers justify the work, they do not remove it

**The protocol gap is unchanged and is now the whole cost of the move:** the Rust client speaks
ollama's API, `mlx_lm.server` speaks OpenAI-compatible. **A real-traffic test requires a shim on
:11434** — and feeding live voice work through an untested translation layer is how shim bugs get
mistaken for model-quality regressions, next to voices that ALREADY dead-letter on strict parsing.
**Recommended order: settle the tokenizer warning → build the shim → shadow ONE voice → then cut
over.** Per-request `num_ctx` control must survive the shim, or D-T35 gets worse.

---

### D-T35 — ⛔ **THE SILENT SYSTEM-PROMPT EVICTION IS NOT A RISK. IT IS HAPPENING NOW, AND IT IS THE MOST SERIOUS FINDING OF 2026-08-08.**

**`route::VOICE_NUM_CTX`'s doc predicted this failure mode and D-T29 warned it "degrades quality
invisibly". Both were right. Measured live 2026-08-08 18:01 EDT, and it is worse than described.**

**Method — two independent signals, so neither can be argued away:**
* **NUMERIC:** a `narratives`-scale prompt (34,660 chars, **7,192 tokens** by the model's own
  tokenizer) sent at production's `num_ctx=4096`, reading back ollama's `prompt_eval_count`.
* **BEHAVIOURAL:** a secret code (`ALPHA-7731`) placed **only in the prompt's HEAD**, in a system
  instruction ordering the model to echo it. If the head survives, the model can obey. **If the head
  is evicted, the model cannot possibly know the code.**

**RESULT:**

| | |
|---|---|
| true prompt length | **7,192 tokens** |
| `prompt_eval_count` (what the model ACTUALLY saw) | **2,051 tokens** |
| **discarded** | **5,141 tokens — 71.5% of the prompt** |
| secret code in the reply | **NO — the head was evicted** |
| `error` field | **`None`** |
| dead-letter raised | **none** |

**THE MODEL DID NOT FAIL. IT FABRICATED.** Asked to open with a code it had never seen, it invented a
plausible substitute and continued in perfect confidence:

> `**🔒 *SYSTEM-947-AUTH: PROCESSING TRANSFER TENSIONS…`

**`SYSTEM-947-AUTH` does not exist anywhere in the prompt.** This is the exact shape of failure the
rail is least equipped to notice: **no error, no dead-letter, no parse failure — just confident,
well-formed, wrong output.** Nothing downstream can distinguish it from a good answer.

##### THE MECHANISM, AND WHY 2,051 AND NOT ~4,000

**`--context-shift` with `--keep 4` is live on the runner** (D-T30's flag dump). When the window
fills, llama.cpp discards roughly the first HALF of the context beyond `--keep`. **4096 halved ≈
2048 — and the measurement came back 2,051.** That is the mechanism, confirmed to within three
tokens. **The `--keep 4` is why only the first four tokens of the system prompt survive.**

##### WHO IS AFFECTED RIGHT NOW

**Chat-template overhead is ~554 tokens** (control: a 1,792-token prompt reported
`prompt_eval_count` 2,346), so **the usable content budget inside 4096 is ~3,540 tokens, not 4,096.**

| voice | prompt | + template | vs 4096 | status |
|---|---|---|---|---|
| `narratives` | ≈7,574 tok | ≈8,128 | **~2× over** | ⛔ **EVICTING NOW** |
| `vibe` | ≈6,437 tok | ≈6,991 | **~1.7× over** | ⛔ **EVICTING NOW** |
| `momentum` | ≈2,535 tok | ≈3,089 | fits | ok |
| `sigil` | ≈1,897 tok | ≈2,451 | fits | ok |
| `transfers` | ≈1,119 tok | ≈1,673 | fits | ok |
| `rating` | ≈723 tok | ≈1,277 | fits | ok |

**HYPOTHESIS WORTH TESTING, NOT YET TESTED:** `narratives` carries **17 dead-letters** with
`parse narratives failed (raw="{...` — **if its JSON contract lives in the evicted head, the parse
failures are a SYMPTOM of this, not a separate defect.** *(It would NOT explain `momentum`, which
fits the window comfortably — consistent with the existing reading that momentum is a contract-prompt
problem (D-T28a). Two different causes that look alike.)*

##### WHY THIS OUTRANKS THE THROUGHPUT WORK

**D-T29's Mac half was filed as a DIET for speed. It is not. It is a CORRECTNESS repair.** The two
largest voices are running on ~2,000 tokens of a ~7,500-token prompt with their instructions gone,
and the rail cannot see it. **Trimming `narratives` and `vibe` to fit ~3,540 tokens of content stops
active data corruption; it does not merely tidy the envelope.**

**⚠ AND IT INTERACTS WITH BOTH STANDING TARGETS:**
* **Target 1 (ministral):** ministral tokenizes ~32% denser, so **adopting it makes this WORSE** —
  more voices cross the line, not fewer.
* **Target 2 (lower ctx):** ⛔ **lowering `num_ctx` before the prompts are trimmed would evict
  MORE.** The order is not optional: **TRIM THE PROMPTS FIRST, THEN LOWER THE WINDOW.** A ctx
  reduction applied first is a silent quality cut dressed as a throughput win.

---

# APPENDIX S — THE SCHEMA LEDGER (the next session after the voice work)

**Status: OPEN and ACCUMULATING. This is an inbox, not a plan.**

**Scott, 2026-08-06:** *"It seems like there should be a dedicated schema session after the voice
one. I think we should start adding our findings to the end of the voice session document, and note
that a schema session is next so we should be noting schema edits as we move through the voice
work."*

**Session order, settled:** the **VOICE** session (D-T23 multi-tag → D-T24 heat moves to the
character → D-T25 the Scout listens) runs FIRST. The **SCHEMA** session runs after it, and works
this appendix. That order is not a preference, it is a dependency: most of the schema debt below is
only droppable *because* of something the voice work does, and D-T22 spent this entire session
proving what happens when you delete ahead of the code.

### ✅ D-T38 (2026-08-08 22:37 EDT) — **THE MINISTRAL FLIP IS DONE, AND IT WAS FOUND HALF-APPLIED**

**Scott's word given ~22:30; executed 22:37.** The dangerous discovery first: `.env.local` on archbox
had been edited at **17:23** and the service restarted at **17:25** with
`COGNITION_ROUTE_EDITOR=ministral-3:3b` — **but `OLLAMA_MODEL=gemma3:4b` was untouched**, and the
Investigator, Multilang and Sql roles fall back to `OLLAMA_MODEL`. With `MAX_LOADED_MODELS=1` that is
**precisely the reload-thrash configuration §3 and `route.rs`'s `VOICE_NUM_CTX` doc both forbid** —
two tags alternating on one 8 GB card. **It never bit only because the Editor's queue was empty for
those five hours** (see D-T37). *Lesson: a route flip is not one variable — it is every role sharing
the runner, and `OLLAMA_MODEL` is a route.*

**Applied:** `OLLAMA_MODEL`, `COGNITION_ROUTE_ARTICLE_READER`, `COGNITION_ROUTE_EMOTIONAL_NEWS` →
`ministral-3:3b`. Backup `.env.local.bak-pre-ministral-20260808`. The boot line's
`resolved model topology` now reads all six local roles at `ministral-3:3b@localhost` with the Mac's
six voices still `ministral-3:14b@192.168.1.77`.

**Verified live, not asserted:** 3 articles enqueued by hand read back
`parser_outcome=parsed`, `model_version=ministral-3:3b`, `last_error` NULL, queue drained to zero.
*(All three came back `irrelevant`, which is NOT a quality signal — the probe forced
`sport=FOOTBALL` onto three arbitrary newest articles, so resolution correctly found nothing.)*

⚠ **THE VRAM WENT THE WRONG WAY.** `ministral-3:3b` @ 4096 = **6.07 GB resident**; `gemma3:4b` @ 8192
was **5.31 GB**. A 3.0 GB model file costs 6.0 GB loaded, leaving **~2.1 GB** on the card. **Archbox
has LESS headroom after this flip, not more** — anything that assumed the smaller model buys slots
(target 2's `lower ctx → more slots` chain) must be re-measured on this number, not on the file size.

⛔ **STILL UNMEASURED — THE FLIP'S OWN GATE IS UNSCORED.** The post-cap tag-share baseline is banked
below and **no after-picture has been taken.** Compare SHARES, never counts. **Watch `injury`
hardest — nothing subscribes to it (D-T25), so a regression there is silent.**

### ⛔ D-T37 (2026-08-08 22:35) — **THE EDITOR IS IDLE 20 HOURS A DAY AND 202,565 ARTICLES ARE UNREAD**

**Found while looking for something to exercise the ministral swap with — there was nothing in the
queue, and that turned out to be the finding.**

| observation | value |
|---|---|
| `editor` rows in `pipeline_work` | **0** (no pending, no running, no error) |
| newest `editor_reads` | **2026-08-08 05:20:55** (~17 h stale at time of reading) |
| newest article with NO read | **2026-08-08 02:03** → **news ingest last ran at 02:03** |
| articles with no `editor_reads` row, all time | **202,565** |
| `editor_reads` in last 24 h | 1,085 |

**The daily cycle is: ingest fires ~02:00 → D-T21's cap admits ~1,000 → the Editor drains them by
~05:20 → the card idles for twenty hours.**

⛔ **THIS RETIRES §0a's FRAME FOR THE EDITOR.** §0a says the model layer is the throughput ceiling and
the Editor *"runs at parity with ingest — about 96% — which means it never catches up."* **That was
measured PRE-CAP (Aug 3–5, 7,041–8,063 reads/day).** Post-cap the Editor reads 1,085/day and has
**20 hours of spare capacity** against a **202,565-article** backlog. **The ceiling moved from the
model to the cap.**

**This and D-T32 are one fact seen twice**, and together they reframe the question. It is NOT *"can
the Editor keep up?"* — it demonstrably can. It is *"why is a cap sized for a throughput emergency
still armed after the emergency ended?"* **Scott's answer to D-T32 (redefine clause 1 against the
queue) remains correct for the GATE**, but this says the cap itself is now the live product question,
with measured headroom to widen it. **Not changed here — one change, one measurement, and D-T38 was
this session's one change on archbox.**

### ⛔ S-NEW (2026-08-08, from 6.7's failed reading) — **`storyline_articles` RECORDS NO ATTACH SCORE, AND THAT BLOCKS A PHASE**

**This one is not a tidy-up. It is currently blocking Phase 6 from closing.** `rail-6.7-bands.sh`
failed its band and instructed *"STOP and inspect attach scores"* — **and there are none to inspect.**

```
storyline_articles(storyline_id, article_id, attached_at, attach_method)
```

**Four columns. No score, no matched entity, no reason.** `attach_method` is `'auto'` for **all
7,349** rows in the window, so it discriminates nothing. **A wrong merge cannot be audited after the
fact — only re-derived by re-running the scoring code against a moving corpus.**

**Wanted (additive, no rewrite): the attach SCORE and the MATCHED ENTITY that won, per row.** That
converts an impossible instruction into a query, and it is the prerequisite for D-T36's tuning — do
not touch an attach threshold before it exists, or the change cannot be measured either.

#### ✅ SHIPPED 2026-08-08 22:45 — migration 217 + `editor/storyline.rs`

```
storyline_articles(storyline_id, article_id, attached_at, attach_method,
                   attach_score, matched_entities, seed_size, candidate_count)
```

* `attach_score` — the winning score from `pick`. **It was always computed and discarded at the
  INSERT** (`storyline.rs:194` binds it, `:299` logs it to `debug!`, the write dropped it).
* `matched_entities text[]` — WHICH seed participants matched, as `entity_type:entity_id`. A new
  `array_agg(DISTINCT …)` in the candidates CTE; `Candidate.matched` carries it. **Unread by
  `score`/`covers_seed`/`pick`** — record-only, so it cannot move an attachment.
* `seed_size` — the `covers_seed` denominator, because the gate is a **ratio** and the numerator
  alone is unreadable.
* `candidate_count` — written on **every** row including openings ("N scored, none cleared").

**NULL discipline: nullable, no backfill.** Pre-217 rows (19,920) and opening articles read NULL.
`attach_score IS NULL` means *pre-217 or opened*, **never "scored 0"**. Rows begin landing at the
next cognition deploy.

#### ⛔ AND THE MECHANISM IN D-T36 WAS WRONG — RETRACTED THE SAME NIGHT

Writing the instrumentation meant reading the candidate query, which **falsifies the feedback loop**
D-T36 was filed with. The join is constrained to the **frozen seed**
(`storyline.rs:436`, `se.joined_at = min(joined_at)`), so a storyline's 169 entities never widen its
own matching. **Measured seeds: 4 / 3 / 5** on storylines 7474 / 7477 / 8012.

**It is SEED COMPOSITION.** 7474's seed is `{Vinicius, Real Madrid, Arsenal, Espanyol}`;
`covers_seed` needs `shared*2 >= 4`, i.e. **2 of 4**; a same-week transfer piece naming Real Madrid
and Arsenal scores `1+1+1+1 = 4 > 3` and joins. `covers_seed` was built against an **11-name NBA
listicle** — it guards big seeds and leaves small hot seeds wide open (a 2-entity seed has a bar of
**one**). The intruders are all `attach_method='auto'`, so this is live behavior, not backfill.

**Candidates to measure once 217 has rows** (none chosen, none applied): weight seed entities by
corpus frequency so Real Madrid counts less than Vinicius; require the seed's `subject` to be among
the matched rather than any 2; scale the bar with seed hotness. **⛔ Still no global threshold
change — p50=1, p90=3.**

**This is the third instance in one day of the same failure class** — the phantom archbox mirror, the
`OLLAMA_NUM_PARALLEL` misread, and now this. **We keep being asked to read an observation we never
recorded.** T2 says the model describes and code judges; **this is the corollary — when code judges,
it must WRITE DOWN WHY, or the judgment is unauditable.**

### THE WORKING RULE WHILE DOING VOICE WORK

**Log every schema observation here as you hit it. Do not act on it unless it is COUPLED** (§B's
test: *does the voice change make this safe, or is it merely near it?*). One line is enough:

```
- [ ] <object> — <what you noticed> — <why it is not being done now> — <date, where you were>
```

Two failure modes this is designed to prevent, both already demonstrated this week:
1. **Acting on an uncoupled finding mid-tuning** bundles a second behaviour change into a
   measurement. (D-T22 nearly did this to three live functions on a premise that was false.)
2. **Not writing it down** means the next session re-derives it from scratch — or worse, re-derives
   it *wrong*, which is how `season_recompute_needed` and `provider_entity_map` came within one
   migration of being dropped while live.

### D-T39 — ⛔ **EVERY RUST PRODUCTION BINARY UNTIL 2026-08-08 22:55 EDT WAS AN UNOPTIMIZED `debug` BUILD.**

**Found by a size check during the mig-217 deploy, not by looking for it.** The staged binary came
out **23,280,384 bytes** where the running one was **300,966,352** — 12.9× smaller. That is not a
code delta; that is `debug` vs `release`.

**OBSERVATION (not judgment) — three independent confirmations:**
1. `rust/target/debug/scoracle-cognition` is **300,966,352 bytes**, byte-size-identical to what was
   sitting in `rust/bin/` (placed 16:03 Aug 8). `rust/target/release/scoracle-cognition` is
   **23,280,384**.
2. **`scripts/hosting/release.sh:137` builds without `--release`**, and line 139 copies from
   **`$REPO_ROOT/rust/target/debug/$bin`** — the comment at line 133 says so in as many words
   (*"cargo writes to rust/target/debug/<bin>"*). `scripts/hosting/install.sh:95` is the same.
3. **`rust/Cargo.toml` has NO `[profile.*]` section at all**, so nothing was overriding the defaults
   in the other direction.

**So this is not a slip in one deploy — the release path itself has always shipped `debug`.**

**WHAT CHANGED AT 22:55, beyond the 217 code** (`--release` was explicit in this session's authorised
deploy block, so it was instructed, not improvised):
| | `debug` (all prior deploys) | `release` (live now) |
|---|---|---|
| `opt-level` | **0** | **3** |
| `debug-assertions` | **on** | **off** |
| `overflow-checks` | **on** — integer overflow **panics** | **off** — integer overflow **wraps silently** |

⚠ **THE OVERFLOW CHANGE IS THE RISK, AND IT CUTS THE UNSAFE WAY.** Arithmetic that would previously
have died loudly now wraps and continues. Two `debug_assert_eq!` (`investigator/gate.rs:143-144`,
length invariants) also stop firing. Neither is load-bearing, but **a new class of silent-wrong is
now possible where a panic used to be** — the same failure shape as D-T35, and worth remembering if
a number starts coming back subtly wrong rather than not at all.

⛔ **DO NOT CLAIM A THROUGHPUT WIN FROM THIS UNTIL IT IS MEASURED.** It is tempting to book it
against standing target 0 (4–5 h) and that would be a theory, not a reading. **Most of a cognition
call's wall-clock is spent waiting on ollama (GPU-bound); `opt-level` touches only the Rust-side
work** — prompt assembly, JSON parse, DB serialization, the packet compile. The honest prior is
"the CPU share of the call gets much faster and the GPU share does not move," and nobody has
measured what that share is. **D-T37 also says the Editor is idle 20 h/day, so even a real speedup
buys nothing the cap is not already withholding.**

⚠ **THIS DEPLOY IS THEREFORE BUNDLED — a §0-rule-4 violation, recorded rather than hidden.** The
22:55 binary carries **two** changes: the 217 provenance writes *and* debug→release. Consequences
for tomorrow's readings, stated precisely so nothing is misattributed:
* **Reading (a) — 217 columns populate: UNAFFECTED.** Scored-vs-unscored is presence/absence of a
  write path; optimization cannot fake it either way.
* **Reading (c) — ministral tag shares: UNAFFECTED.** That is model output content; the Rust build
  profile does not touch what the model is asked or what it answers.
* **Any wall-clock or throughput comparison across the 22:55 boundary: CONFOUNDED.** Do not read the
  02:00 drain's duration as a ministral number or a 217 number. It is now a three-variable change.

**THE FOLLOW-UP THAT IS OWED (not done here — it is a separate change, and this session's authorised
block was cognition only):** `release.sh` and `install.sh` still build and stage `debug`. **The next
routine `release.sh` run will therefore REVERT this binary to `debug` without anyone noticing** —
including the Go/API path's own Rust bins (`statcommentary`, 248 MB, is still a debug build sitting
in `rust/bin/` right now). Fixing them is a one-line change in each plus the `target/debug` →
`target/release` staging path, and it wants its own deploy and its own measurement. → *rail Appendix D D-T39*

### D-T42 — **THE 217 READING, 2026-08-09 09:00 EDT. THE INSTRUMENT WORKS; THE HYPOTHESIS IT WAS BUILT FOR IS NOT SUPPORTED.**

**The deploy earned its window.** The 02:00 Sun drain ran on `129d50e6f582`: 1,101 `editor_reads`
between 02:00:28 and 07:03:32, **796 editor calls on `ministral-3:3b`** — the first real production
volume on the new model, and the first attaches written by code that persists its own decision.

**READING (a) — 217 POPULATES. CLEAN PASS.** 348 attaches in the window: **227 scored, 121 unscored**.
The 121 are not a gap and not a bug — they are **exactly** the **121 storylines opened** in the same
window (`storylines.first_seen_at`), and `storyline.rs`'s bind is deliberate:
`.bind(winner.map(|_| score))` writes NULL when there is no winner, with a comment stating that
writing 0 "would read back as *scored zero*, which is a different and false claim." `candidate_count`
is written on **all 348, zero NULL**, exactly as designed — so "N were scored and none cleared the
bar" remains distinguishable from "there were no candidates."

**READING (b) — THE QUESTION 6.7 COULD NOT ASK, NOW ASKED. THE ANSWER IS "NOT THIS."**

*Attaches per storyline, normalised by seed size* (the raw counts mislead — most storylines simply
ARE size 3–4):
| seed_size | 2 | 3 | **4** | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|
| storylines | 27 | 29 | 38 | 9 | 11 | 6 | 6 |
| **attaches/storyline** | **1.48** | 1.79 | **1.97** | 1.78 | 1.64 | 1.17 | 1.33 |

⛔ **SMALL SEEDS DO NOT OVER-ATTRACT.** D-T36 predicts a small seed of hot entities running away;
the measured peak is **seed 4**, and **seed 2 is the second LOWEST** rate on the board. The worst
case in the window is 6 attaches, and it occurs at sizes 2, 3 and 4 alike.

⛔ **AND THERE IS NO HOT CLUB.** Matched-entity wins are flat: **12, 11, 9, 9, 8, 8, 8, 8, 7, 7, 7,
6.** The hypothesis predicts a steep head — one entity winning everything. This is a plateau.

⚠ **WHAT THIS DOES *NOT* SETTLE, STATED PLAINLY: AN 8-HOUR WINDOW CANNOT SEE A TAIL THAT TOOK 72
HOURS TO BUILD.** These are fresh storylines in their first hours; the runaways accumulated over
days. **So D-T36 is NOT KILLED and 6.7 STAYS OPEN** — what has changed is that the aggregate
mechanism it proposed now has evidence against it, and the instrument to settle it finally exists.

**THE ONE THREAD WORTH PULLING — 7477.** The three known runaways now total **7477 = 173, 7474 =
171, 8012 = 75** articles. In this window **only 7477 grew: +3 attaches, `seed_size` 3, avg_score
5.33** (the bar is 4). 7474 and 8012 took **none**. A 3-entity seed needs `shared*2 >= 3` → **2
matches**, and it keeps finding them. **Watch 7477 specifically across a multi-day window rather
than re-running the aggregate** — the aggregate has now been asked and answered.

### D-T43 — ⛔ **A GRAMMAR CONSTRAINS SHAPE, NEVER MEANING. THE EDITOR TRIM SHIPPED, REGRESSED, AND WAS CORRECTED IN ONE SESSION.** (2026-08-09 09:30–11:00 EDT; the ep1 → ep2 → ep3 drain)

**This is the D-T40 trim actually shipped, and the reading D-T40 asked for killed its central
premise.** Written at length because the mistake is a GENERAL one and the next voice to be trimmed
will meet it again.

**WHAT D-T40 CLAIMED, AND WHY IT WAS REASONABLE.** `EDITOR_FORMAT_SCHEMA_RAW` pins all 11 keys,
their order, and every enum via constrained decoding, so the system prompt's closing
`Return strict JSON only, with the keys in exactly this order: {…}` template looked like
belt-and-braces over a structural guarantee. It cost a measured 235 tok on every one of ~27k weekly
calls. Deleting it was called safe. **The deletion was shipped as `ep2` and deployed at 09:42.**

⛔ **WHAT THE DRAIN SHOWED — THE TEMPLATE WAS NOT REDUNDANT.** The schema types the FREE-TEXT fields
as bare `{"type":"string"}`. The grammar therefore constrains nothing about what goes IN them, and
the template's placeholders were the ONLY place their semantics were ever written down.
**`"ISO 639-1"` appeared EXACTLY ONCE in the whole ep1 prompt — inside the block ep2 deleted.** The
surviving prose said only *"Detect the source article language"*, which never asks for a code.

| measured on real production reads | ep1 (3-day baseline) | ep2 | ep3 |
|---|---|---|---|
| `source_language` = `"unknown"` | **1.4%** (95/6,939) | ⛔ **100%** (29/29) | ✅ **9.1%** (4/44) |
| markdown (`**`) inside `caveats`/`evidence_blurb` | **0.3%** | ⛔ **13.8%** | ✅ **0%** |
| role word (`absent`…) sitting in `names[].descriptor` | **0.50%** (224/44,655) | ⛔ **4.51%** (6/133) | ✅ **1.58%** (3/190) |

⭐ **WHY THESE THREE ROWS ARE TRUSTWORTHY WHEN THE OTHERS ARE NOT: ep2 AND ep3 DRAINED THE SAME
SWEEP, INTERLEAVED.** ep3 was deployed mid-queue at 09:52, so both arms read the same 98 articles'
worth of material. **The ep2 → ep3 contrast is therefore a controlled A/B**, and it is the contrast
that carries the finding; the ep1 column merely corroborates. **100% → 9.1% on shared material is
not a mix artefact.** ⚠ ep3 is still ~6× ep1's rate, so the prose fix is a large recovery, **not a
full one** — the remaining 4 are worth reading before trimming further.

⛔ **AND THE COLUMN THAT MUST *NOT* BE READ THAT WAY — THE MATERIAL WAS THE CAP'S DREGS.** Because
the 02:00 sweep had already spent the day's allowance, the 09:45 re-sweep returned what `feed_rank`
leaves for last, and the page mix is nothing like the baseline's:

| `page_kind` | ep1 baseline | ep2 | ep3 |
|---|---|---|---|
| `article` | **94.5%** | 51.7% | 40.5% |
| `score_table` | 2.3% | **34.5%** | **35.1%** |
| `listing_or_schedule` | 1.3% | 10.3% | 16.2% |

**So two alarming-looking rows are CONFOUNDED and must not be attributed to the prompt:**
* **`avg_names` 6.44 → 4.59 → 4.32.** Score tables and TV listings name fewer people than match
  reports. Expected under this mix; **no conclusion about the trim.**
* **`fail_closed` 3.37% (7d) / 4.77% (today's 02:00 ep1 drain) → 9.38% ep2 → 12.00% ep3.** The rise
  is real in the data and **may well be the material**, not the prompt — and ep2 vs ep3 are
  indistinguishable at this n (3/32 vs 6/50). ⛔ **DO NOT bank this as "the trim broke parsing" and
  do NOT bank it as "the trim is fine". It is the open question, and it is the SAME question D-T40
  left open about `EDITOR_NUM_PREDICT`.**

✅ **THE FIX — `ep3`: RESTORE THE MEANING IN PROSE, NOT THE TEMPLATE.** Three lines, each aimed at a
field the grammar cannot carry: name ISO 639-1 and its code set; say outright that
`subject|opponent|passing_mention|absent` belong to FIELD 3 and never to `descriptor`; restore
`evidence_blurb`'s "2-4 compact English sentences" and ban markdown in every field. **~40 tokens
instead of 235, so most of the trim survives.**

**THE MEASURED COST, on the live ministral runner** (`prompt_eval_count`, empty-user control = the
554-token chat-template floor — this reproduces D-T40's numbers exactly, which validates the
instrument):

| | system prompt | fixed cost/call | article headroom | overflow @900 | overflow @~450 |
|---|---|---|---|---|---|
| ep1 | 8,312 ch = **1,431 tok** | 1,985 | 1,211 tok | 68.4% | 45.7% |
| ep2 | 7,472 ch = **1,192 tok** | 1,746 | 1,450 tok | 55.5% | 35.1% |
| **ep3** | 7,819 ch = **1,287 tok** | **1,841** | **1,355 tok** | **60.3%** | **39.1%** |

*(Overflow computed over all 27,667 real 7-day prompts at 4.68 ch/tok — the ratio implied by D-T40's
own p50 datapoint. It reproduces D-T40's published 68.1%/45.5%/55.4% to within 0.3 pt.)*

**SO THE NET RESULT IS REAL BUT HALF THE HEADLINE: −144 tok/call, not −235**, and the Editor's
prompt is still far too big for its window. `names` (FIELD 2) and `entity_roles` (FIELD 3) remain
~50% of what is left.

⚠ **THE RULE THIS BUYS, AND IT APPLIES TO EVERY REMAINING VOICE:**
> **Before deleting anything from a prompt because "the grammar enforces it", check WHICH of the
> two things the grammar enforces. It pins SHAPE — keys, order, types, enums. It says NOTHING about
> the CONTENT of a free-text field. Any semantics stated only inside a JSON template — formats,
> lengths, "verbatim", "from the text", units, codes — is load-bearing prose in disguise.**

**Concretely, the enum'd fields (`page_kind`, `kind_hint`, `role`, `story_type`, `register`) were
NEVER at risk and did not move. Every field that regressed was a bare string.**

⚠ **WHAT THIS READING DOES NOT SETTLE.** n≈30 per arm is decisive for a 100%-vs-12% effect on shared
material and **useless for the fail-rate question**, which is additionally confounded by the page
mix above. **`EDITOR_NUM_PREDICT` was deliberately NOT touched** (§0 rule 4 — one change at a time),
so D-T40's truncation hypothesis is **still unproven and still owed the `/tmp/trunc.py` replay or a
large drain.** **The 02:00 drain is the instrument for both** — it reads ~800–1,100 articles at the
baseline's 94.5%-`article` mix, which is the only honest comparator.

⛔ **AND THE THING THAT MADE THIS SESSION SMALL: D-T21's CAP.** The 02:00 sweep had already spent
today's allowance, so a fresh 09:45 sweep yielded **98 fresh articles and withheld 1,954**. The cap
was NOT re-sized (it is on the session's DO-NOT list). **The Editor's daily read budget is now the
binding constraint on how fast a prompt change can be measured at all** — a same-day A/B is
~50 reads per arm, and anything statistical needs the 02:00 drain.

##### ✅ `ep4` — THE STRUCTURAL FORM OF THE FIX, SHIPPED SAME SESSION (deployed `e15ef96a0923`)

**Scott's steer: stop asking the model to remember a convention when the schema can make the wrong
answer unrepresentable.** `source_language` is now an **enum of 63 ISO 639-1 codes + `unknown`**.

**What only structure could fix:** across 35,288 ep1 reads the field held **59 distinct values**,
including **`al`, `ge`, `md`, `me` — COUNTRY codes, not language codes.** No amount of prompt text
prevents that; an enum makes it unrepresentable.

⭐ **AND THE ENUM IS FREE.** `format_schema_raw` is compiled to a grammar and **never enters the
context window** — the 2,043-char schema is not part of D-T40's 1,985-token fixed cost (which is
floor 554 + system 1,431 only). **So structural constraints cost ZERO prompt tokens while prose
constraints cost real ones.** ep4 therefore also deleted ep3's ISO-639-1 explanation and code list:
**8,312 → 7,472 → 7,819 → 7,774 chars.**

⛔ **WHAT WAS DELIBERATELY KEPT, AND IT IS THE LIMIT OF THE STRUCTURAL FIX:** the prose clause
*"use `unknown` only when it genuinely cannot be told — never as a default"*. **`unknown` is a legal
enum member, so the grammar cannot stop the model choosing it lazily — which is exactly what ep2 did
on 100% of reads.** **Structure pins the value SET; only prose discourages a legal-but-lazy choice
inside it.** Both halves are load-bearing.

**THE GENERAL RULE, now the working method for every remaining character:**
> **Enum or bound it in the schema (free); instruct in prose only for what the schema cannot express
> (costs tokens on every call). Prefer the free one.**

### D-T44 — ✅ **THE EDITOR REWRITE (`ep5`) AND THE BODY EXTRACTOR. THE OVERFLOW IS CLOSED, AND THE BIGGEST WIN WAS NEVER IN THE PROMPT.** (2026-08-09, Scott: *"this is a mess… we're burning LOTS of our context budget on slop"*)

⭐ **THE HEADLINE: THE SLOP WAS THE ARTICLE BODY, NOT THE PROMPT.** `fetch::clean_html` stripped
`<script>`/`<style>`, removed tags and kept **every remaining text node** — there was no content
extraction at all, so the Editor was handed the whole page. A representative 7,922-char production
prompt carried **~2,700 chars of article inside ~5,200 chars** of betting-site menus, an African
country list, "Related To This Article", "Popular News" and the publisher's street address in Accra.
**Two thirds of the article budget was site furniture** — and because `EDITOR_MAX_MODEL_CHARS`
truncated at 9,000, the cap was cutting off real prose to make room for navigation.

`fetch::extract_article_text`, two passes: delete every non-content element whole (`nav`, `header`,
`footer`, `aside`, `form`, `noscript`, `svg`, `iframe`, `button`, `select`, `textarea`, `template`,
`figure`), then prefer the **LARGEST** `<article>`, then `<main>` — largest because related-post
cards are themselves `<article>` on most CMS themes.

**Measured on 12 real publishers, one per domain, pulled from `editor_reads`:**
| | |
|---|---|
| total | **88,011 → 60,143 chars = 31.7% of the article budget reclaimed** |
| best | 7news 52.8% · 67hailhail 52.7% · iheart 52.0% · 933thedrive 47.9% |
| worst | 49erswebzone 0.8% (already clean) — **nothing was destroyed** |

⛔ **It can only SHRINK the body, and anything under `ARTICLE_MIN_WORDS` is discarded for the
full-page text**, so a site it cannot parse is exactly as well off as before. Three tests pin it.
`examples/extract_probe.rs` re-measures any URL set.

✅ **`ep5` — THE PROMPT REWRITTEN TO THE JOB.** What was cut, and it was all real slop:
* **a phantom `FIELD 4`** — the prompt numbered FIELD 1, 2, 3, then "Then story_type", then FIELD 5,
  FIELD 6. **There has never been a FIELD 4.** Four contracts of edits and nobody noticed.
* the **ar7 / `co_mentions` / `relevant_entities` history** — describing contracts the model has
  never been asked to emit;
* the **`gemma3:4b` seat and the 8192-ctx arithmetic**, both false since the runner became ministral
  at 4096 (and the same stale attribution cleaned out of `derive.rs`, where the descriptor-arm
  finding now says plainly it was measured on gemma and NOT re-measured on ministral);
* a **250-char JSON example blob** for `names[]` and **two long worked `absent` examples**, whose
  content survives as one clause each;
* restatements of enum values **the grammar already pins for free** (D-T43).

**Scott's statement of the job is now IN the prompt and the module doc:** read the article,
summarise with special attention to **emotional text, names, injuries/suspensions, transfers**; be
the **second layer of false-positive defence** behind Google's ranked query (which is only ever a
hypothesis); and **surface unfamiliar names, because unresolved names are the Investigator's
discovery channel.**

**MEASURED ON THE LIVE RUNNER (`prompt_eval_count`, 554-token floor control):**
| | ep1 | ep3 | **ep5** |
|---|---|---|---|
| system prompt | 8,312 ch = 1,431 tok | 7,819 ch = 1,287 tok | **5,429 ch = 692 tok** |
| fixed cost/call | 1,985 | 1,841 | **1,246** |
| article headroom | 1,211 tok | 1,355 tok | **1,950 tok** |
| overflow @900 | 68.4% | 60.3% | **structurally impossible** |

⭐ **THE OVERFLOW IS CLOSED, AND THAT RETIRES D-T40.** `EDITOR_MAX_MODEL_CHARS` was re-derived
**9,000 → 7,500** (D-T40 item 2, finally done) from the budget that is actually left: 4096 − 554
floor − 692 system − 900 `num_predict` = **1,950 tokens ≈ 9,100 chars**, so a 7,650-char worst-case
user message (7,500 + ~150 preamble) lands at **~3,781 of 4,096 tokens**. ⚠ **Honest limit: that
holds at the measured 4.68 ch/tok. Text denser than ~4.0 ch/tok could still cross — a TAIL now,
against 68.4% of all calls before.**

**TWO CONTRACT FIXES RODE ALONG:**
1. **`suspension` joins `story_type`** (Scott: injuries/suspensions are what the Editor must never
   miss) and maps to **BOTH** tags, `injury` + `suspension`. ⛔ Deliberate: every
   `stage_routing_subscriptions` row today is written against `injury`, so emitting the new tag
   ALONE would route real availability news **to nobody** under the fail-open rule. Additive today,
   subscribable alone tomorrow.
2. **`entity_roles` now says "one entry for each HYPOTHESIS ENTITY … and nothing else."** Observed
   directly in ep2 envelopes: the model was emitting one row per `names[]` entry instead, and that
   feeds `derive_relevance` — the false-positive gate — directly.

⚠ **UNVERIFIED ON REAL OUTPUT AT WRITING.** ep5 is deployed (`0ddec9451a13`) and boot-verified, but
production is paused, so no ep5 read exists yet. **The tests and the token measurements are real;
the reading is not taken.** Next drain reads it. *(Superseded same day: the fixture gate read it —
D-T45 — and `ep6` shipped on what it found. The production reading is still owed, now of ep6.)*

### D-T45 — ✅ **THE GATE READ ep5, AND THE READING BOUGHT `ep6`: KIND_HINT WAS AFFILIATION, STORY_TYPE TRACKED THE LAST-NAMED ENUM VALUE, THE GATE COULDN'T SEE EITHER — AND D-T44'S "THE TRIM LOST NOTHING" WAS WRONG.** (2026-08-09, Scott: *"Okay, let's test it!"*, then *"I want an example of the output"*)

**THE INSTRUMENT.** Production stays paused, so the reading D-T44 owed was taken on the fixture
gate instead: `eval --task editor --fixtures` (frozen system) vs `--live-system` (current
source), 12 fixtures, temp 0, daemon stopped (D-T19's validity condition, checked: `inactive`).
Per-check diffs throughout, never totals (D-T19). Field-level probes via raw Ollama replays of
fixture user-prompts — the same instrument that produced the worked examples below.

⛔ **AN INSTRUMENT ERROR WORTH RECORDING FIRST, BECAUSE IT NEARLY SHIPPED A FALSE FINDING: the
on-disk fixtures had been re-frozen at ep5** (the version-pin test enforces exactly that), so the
session's first "frozen ep1-era baseline vs live ep5" A/B was **ep5 against itself** — the
identical per-check tables it produced (45/53 twice) are a clean DETERMINISM check of the
D-T19 condition, and NOTHING about the trim. The true ep1 system was recovered from git
(`0b2da3a`, 8,312 ch) and spliced into the current fixtures for an honest run. **When a frozen
baseline agrees with the live arm suspiciously well, check what is actually frozen.**

**ep5's misses, 8 checks, five sharing one root cause:**

⛔ **`kind_hint` WAS BEING READ AS AFFILIATION, NOT IDENTITY.** `Vinicius <club "Real Madrid
forward">`, `Dragojevic <club "Rangers defender">`, `Buendia <club>` — the model set `kind_hint`
from the club in the descriptor. Downstream that single inversion: failed both `name_kind`
checks, collapsed the Vinicius namesake TIE to `unresolved` (person surfaces are kind-incompatible
with `club`, so the refusal bucket never saw it), and let `Paris <club "">` auto-link (empty
descriptor, so the place-arm could not fire). The ep5 prose was one line — "what the text treats
this name as" — and the model answered a different question than the one intended. **The fix is
identity stated outright: "what the name ITSELF is, never its affiliation — a 'Rangers defender'
is a person, and his club is its own entry in this list."** Six checks flipped ✓ on the next run,
zero regressed.

⛔ **THEN THE DISPLAY-LINE SWEEP (fields the gate had NO checks for) CAUGHT THE PROMPT-EDIT CLASS
D-T43 WARNED ABOUT, IN A NEW FORM: `story_type` SMEARS TOWARD WHICHEVER ENUM VALUE THE PROSE
NAMES LAST.** ep5's clause ended "…is suspension": suspension on a fan protest and a Tour de
France page. Round 2 ended the clause on injury: injury smeared onto SIX fixtures (a renovación,
a 2-1 semi-final). Three prompt variants, three story_type distributions, near-identical check
totals — **because not one fixture asserted `story_type`.** Two fixes, one structural:
* **Balanced taxonomy:** every enum value glossed exactly once, ending on the fallback ("general
  for anything else"). Under it story_type is right across the whole set and stable through a
  subsequent unrelated edit.
* **The gate now sees the field:** `story_type_is`/`register_is` existed in `Expect` but were
  authored on ONE fixture. Now on eight (denominator 53 → 60, pinned in `eval.rs`). ⚠ *The rule
  this buys: a field the gate cannot see is a field a prompt edit can quietly break — author the
  check BEFORE tuning the field.*

**TWO MORE, FROM THE WORKED EXAMPLES:**
* **`register` mislabeled its own correct quote** ("People are furious…" → `anticipation`). Fix:
  the label must describe the quoted phrase. But "label the phrase" ALONE made the model force
  charged labels onto flat quotes (ep5 quoted the same sentences and said neutral), so neutral is
  stated as legal for a quoted-but-flat phrase — and `anticipation` is deliberately NOT glossed:
  "looking ahead" turned out to describe every routine "expect him back after the break" club
  statement. The borderline over-charging seen mid-session (Pérez's "prioridad absoluta" →
  outrage) disappeared once the example blob returned in round 5 — the final sweep is neutral
  everywhere it should be, with one oddity left (Arteta's "remain confident" → `resignation`,
  unchecked, phrase correct).
* **English articles returned `source_language: "unknown"` — under ep5 too** (the model named
  `es` fine; English is unmarked, so "genuinely cannot be told" read as license). Fix: `en` is
  named outright. The D-T43 drain metric (1.4% ep1 baseline) is the production check owed.

⭐ **AND THE HEADLINE, FROM THE HONEST ep1 RUN: D-T44's "THE 52% TRIM LOST NOTHING" WAS WRONG —
THE `names` EXAMPLE BLOB WAS LOAD-BEARING.** True ep1: **58/60**, ep5: **48/60**. The gate had
simply never been able to see the fields the trim broke (no `story_type_is` checks) and the
production metrics D-T44 cited (tokens, overflow) measure cost, not quality. ep1's blob
demonstrates the exact shape the model kept inverting — a club entry AND a person whose
descriptor names that club — and prose alone never taught it: rounds 1–4 of clause-writing
recovered Buendia/Dragojevic/Vinicius but never Rangers-as-own-entry or Paris. Restoring a
MINIMAL worked pair (~250 ch against ep1's full blob) fixed both on the next run **and** settled
the register over-charging as a side effect. **D-T43 gains its third clause: the grammar pins
shape, prose carries meaning — and for a 3B, a worked EXAMPLE carries what prose cannot.**

**THE SCORE, measured on the 60-check gate, same fixtures, same runner, temp 0, daemon stopped:**
| | full 60 | system prompt |
|---|---|---|
| ep1 (true, from git) | 58/60 | 8,312 ch = 1,431 tok |
| ep5 | 48/60 | 5,429 ch = 692 tok |
| **ep6** | **59/60** | **6,384 ch = 914 tok measured** |

⛔ **THE ONE THAT REMAINS FAILS UNDER EVERY PROMPT TESTED, ep1 INCLUDED — capacity, not
contract:** Fortuna Mining Corp accepted as `subject` for hypothesis "Fortuna" (the
false-positive class; flips run-to-run on near-ties). It is the documented honesty gap, and the
code-side idea on file is a superset-name arm in `derive.rs` (a `subject` vote whose only
`names[]` support is a LONGER name containing the hypothesis — "Fortuna Mining Corp" ⊇
"Fortuna") — unbuilt, because it would also retract "Real Madrid Castilla"-class legitimates.

**COST:** 914 tok measured via the nonce-prefix method (Ollama omits `prompt_eval_count` on a
full cache hit — prepend a nonce to both arms and difference). Fixed cost/call 1,468 (ep1 1,985).
`EDITOR_MAX_MODEL_CHARS` re-derived **7,500 → 7,200**: 4096 − 554 − 914 − 900 = 1,728 tok ≈
8,090 ch of user budget; worst case ~3,940 of 4,096, dense-text tail at ~4.25 ch/tok. **ep6
beats ep1's gate score at 64% of its token cost.** The fixtures are re-frozen at ep6 (the
version-pin test in `eval_tasks.rs` enforces the re-freeze on every bump — it is what caught
this session's instrument error).

⚠ **STILL OWED: the production reading, now of ep6** — the gate is 12 curated pages; the 02:00
drain at the 94.5%-article mix is the only honest measure of the register rate, the `unknown`
rate, and the story_type distribution at scale.

### D-T46 — ✅ **THE INVESTIGATOR'S PROSE ARM IS BUILT (5.4's deferred fallback, contract `ip1`) — THE SEAT'S FIRST MODEL PATH, AND THE VERBATIM CONTRACT SURVIVED ITS FIRST CONTACT WITH THE LIVE 14B UNTUNED.** (2026-08-09, same session as D-T45; Scott: *"start building out the Investigator"*)

**WHAT IT IS.** The fallback for the D-T8 class: a name the news writes one way ("Airious
Bailey") that the encyclopedia titles another ("Ace Bailey"), invisible to `wbsearchentities`
(labels/aliases only) but sitting verbatim in the page's opening sentence. Flow, triggered ONLY
on the Wikidata arm's `RejectedInsufficientEvidence` (not-sport already had identified evidence;
a tie needs a discriminator, not more prose):
1. **Wikipedia FULL-TEXT search** (`w/rest.php/v1/search/page`, new `discover::wikipedia_search`)
   — validated live: the D-T8 page is rank 1 for the legal name, excerpt carrying
   `Airious "Ace" Bailey … Utah Jazz`.
2. **Code prescreen** — `mentions_all_tokens`: every word of the sought name in the page surface;
   ≤2 surviving pages get a summary fetch + model read (`MAX_PROSE_PAGES`).
3. **The model QUOTES, code decides (T2's rule for this arm, fixed before it was built):**
   4 fields, all verbatim — `subject_kind` (enum, free), `sought_name_evidence`,
   `occupation_phrase`, `team_names[]` — and **every free-text field must pass
   `contains_normalized` against the exact page text the model was shown** (`prompt::page_text`
   is the ONE definition both sides use). A hallucinated field fails containment and is treated
   as absent. ep6's lessons applied from birth: order-true raw schema, worked example in the
   prompt, enum for the triage field.
4. **`gate::decide_prose`** — same three clauses, same shape as `decide`: evidence screen →
   sport class (`prose_role_class`, sport-gated like the description screen) → team
   discriminator (`resolve_team_names`: verbatim phrases → sport-scoped exact `nrm()` surfaces,
   unique-only) → exactly-one-survivor. **Plus the count-threshold clause: the Editor's
   descriptor is the second independent observation** (`descriptor_role_class`), and a
   role-class CONFLICT between news and encyclopedia refuses.
5. **Accept rides the ONE write path** — `accept_candidate`, generalized: pseudo-item with
   empty `qid` (wikidata external-id/meta writes gated), `enwiki` external id instead, aliases =
   the page's connecting form AND the news form — the news form is what makes the next resolver
   pass hit. Merge-to-existing-player keeps the team-agreement discriminator, now over the
   caller's pre-resolved teams (both arms).

⭐ **LIVE PROBE, ministral-3:14b, temp 0, three real pages:** the D-T8 case → person /
`Airious "Ace" Bailey` / "American professional basketball player" / ["Utah Jazz"] — a clean
Accept; the Rutgers TEAM page (which legitimately passes the prescreen) → `club`, all fields
empty; the 1930s hockey namesake → occupation quoted honestly, wrong sport, refused. **Zero
prompt iterations were needed.** The instrument caught two REAL bugs before any deploy, both in
CODE, not the prompt: (1) the prescreen was written as contiguous containment and the D-T8 page
writes the nickname INSIDE the sought phrase — `Airious "Ace" Bailey` — so the arm would have
dropped the exact page it exists to find (fix: token presence for discovery, contiguity for the
model's evidence); (2) the search excerpt carries HTML entities (`&quot;`) that break every
containment check (fix: decode in `strip_tags`).

**COST PROFILE:** ≤2 model calls/candidate on the Mac's 14B (`Role::Investigator`, its own
governor — never archbox), ~5 Wikimedia fetches/candidate at the 2s spacing. `acquisition_runs`
rows self-describe (`model_version` + `parser_version` `ip1` from the run plan).

**SAME-SESSION ADDENDUM — THE OWNER CLASS AND ON-DEMAND QUEUEING (Scott: "queue Jerry Jones…
coaches, owners, agents, etc.", then "easy to queue up… grab metadata like headshots").** The
Jerry Jones probe found THREE stacked failures for the owner class, all in code: he ranks 5th
of 5 in Wikidata search behind four college-basketball namesakes (`MAX_ITEMS` 3 never fetched
the item at all); his P106 carries "American football player" from a 1960s college career, so
occupation-first classification misfiles him as Player; and owners carry no P54/P6087, so the
discriminator had nothing to resolve. Shipped: **P1830 (owner of, current tenures) parses,
outranks occupation history in `classify_role` (the same logic that puts P6087 above P106 for
coaches), joins the discriminator QID resolution, and writes an `owner_of` relationship edge on
accept**; `MAX_ITEMS` 3 → 5; **owner/executive words outrank the bare "manager"** in both role
vocabularies (his lede says "general manager" — a coach-first chain misfiles it; FOOTBALL's
"manager"-means-coach still classifies right because those phrases carry no owner word); and on
the prose arm **a sport-scoped team match unlocks the role vocabulary for phrases with no sport
keyword** (the lede says "owner … of the Dallas Cowboys", never "football owner" — the
resolution itself proves the sport). Enrichment: **P18 Commons images become `photo_url`
wherever the NBA CDN id is absent — NFL's headshot source** (no usable league-id property).
`scripts/investigate.sh` queues a name, a sport's metadata gaps (the vetting-seed shape), or
prints status.

⭐ **THE LIVE RESULT IS A CONTROLLED BEFORE/AFTER ON ONE CANDIDATE.** "Jerry Jones" was already
candidate 548 — nominated by the news 2026-08-04, **rejected `no sport-relevant item` by the old
code 2026-08-07**. The manual queue reopened him; the owner class **accepted in ~20s**:
`persons` 959 kind `owner`, team 19 (NFL → Dallas Cowboys, sport-scoped edge verified),
`owner_of` relationship, `wikidata Q1280022` + `enwiki` external ids — and Wikidata's aliases
rode along free, so "Jerral Wayne Jones Sr." now resolves too.

✅ **THE GATE EXISTS (same session): `eval --task investigator --fixtures` — 8/8 on first run.**
The task registers in `eval_tasks.rs` (fixture-driven by design: live prompts depend on a
Wikipedia fetch, so pages are FROZEN into fixtures; capture new ones from
`acquisition_runs.query_plan`, which records every page the prose arm read). Five new `Expect`
axes (`subject_kind_is`, `evidence_includes`, `evidence_empty`, `occupation_includes`,
`prose_teams_include`); three fixtures frozen 2026-08-09 from the probe set — the
connect-under-another-name accept, the club-page triage, the honest wrong-sport quote. ⚠ The
8/8 was read with the daemon RUNNING (voices share the Mac runner), so it is a first reading,
not a determinism-grade baseline — D-T19's condition applies to any future per-check diff.

⭐ **THE FIRST ENRICHMENT SMOKE (100 players, 50/league) READ 11% — AND 83 OF THE 89 REFUSALS
WERE ONE FINDING: WIKIDATA'S CLAIMS LAG ITS PROSE.** `Ambiguous { survivor_idxs: [i] }` with
exactly ONE survivor, en masse — the Aaron Gordon class: the one name-agreed, sport-relevant
item whose P54 stops at a previous club (Gordon's carries Arizona + Orlando; nobody added the
2021 Denver stint), so the current-team discriminator fails through no fault of ours. **The
fix composes the arm we built the same day:** the single survivor earns one `ip1` prose read of
its OWN enwiki page (`prose_team_corroborates`), and the containment-verified team names must
resolve onto the player's current team — the same discriminator, taken from the fresher of the
encyclopedia's two layers. Everything else still refuses; cost is one summary fetch + one model
call, only in the single-survivor case. `investigate.sh enrich` also re-opens non-pending work
rows so refused players re-queue (the metadata-gap filter already drops enriched ones).

⚠ **STILL OWED:** (1) the live reading once the nomination sweep refills `investigate_entity` —
the Jerry Jones manual queue is the first live datapoint; (2) `descriptor_role_class`'s
vocabulary is unmeasured against real descriptors; (3) the executive/agent classes have no
structural Wikidata claim like P1830 — they ride description/occupation words only; (4) the
corroborated-enrichment recovery rate — the re-run of the NBA 50 is the measurement.

### D-T50 — ✅ **THE INFLUENCER'S v17 — THE PLATFORM-NATIVE-CREATOR REGISTER, AND THE VIBE BODY IS FINALLY GATED.** (2026-08-10, the Influencer/Scout/Analyst session)

**Scott's register, verbatim option:** *"The creator who lives in the feed: reads the room
instantly, translates crowd emotion into one clean take. Feels first, but never fakes —
sincerity stays the craft."*

**Gate first (the D-T45 rule, worst case yet):** the VIBE prose field had **ZERO checks of any
kind since v6** — `VibeTask::evaluate` understood only `score_min`/`score_max`/`hook_nonempty`.
The evaluate now reads the shared prose axes (`prose_includes/excludes`, `prose_min_words/max`,
`total_sentences_max` via the extracted `sentence_runs` helper the Journalist's n18 counter now
shares) plus two NEW `Expect` axes: `hook_max_words` (missing hook fails — asserting the cap
asserts presence) and `hook_excludes` (":" = the Topic:Subtitle construction, "?" = bait). All
authored across the 5 fixtures BEFORE the prompt edit; the v16 baseline then read **43/46**, and
all three misses were one real defect the gate had never seen: **the body leans on pronouns and
never names the subject when the HOOK already did** — which matters because the Analyst renders
`Vibe prompt:` WITHOUT the hook (`analyst/prompt.rs:149-158`), so a pronoun-opening body degrades
momentum's input.

**The edit:** voice paragraph rewritten to the register (lives in the feed, reads the room before
the room reads itself, one clean take); a worked example lands — vibe's first ever (the ep6/n18
lesson) — with invented entities (Marchetti / Union Verde / Riva Nova) so it cannot leak content;
and the new stands-alone rule: *"The VIBE stands alone: it travels downstream without the HOOK,
so name the entity by name inside the body itself."* HOOK cap restated as a hard cap.

**Two gate checks were themselves corrected by the probes (checks can be wrong too):**
`quiet-mixed`'s `prose_excludes:"surge"` tripped on *"It's not a surge, but it's not a slump
either"* — honest negation; the score band already gates inflation; removed. `clearly-positive`'s
`"trick"` relaxed to `"goals"` (the model grounds the thread as "three goals" — equally
corpus-true). One phrasing lesson: "twelve words at the absolute most — cut it down, never cram"
made hooks LONGER (9→14 words on clearly-negative); the terse "Twelve is a hard cap — count the
words" recovered it.

**Reading: 52/53 on oMLX unconstrained, temp 0, fixtures re-frozen at v17.** The one residual:
`continuity-deliberate-move`'s hook runs 13 words ("Morrow's streak just hit a wall and the room
is holding its breath") — good prose one word over, the documented honesty gap (the ep6/Fortuna
precedent; production temp 0.7 varies the draw). Bodies now name Vale/Fenn/Morrow/Sharks/Trent
and stand alone.

### D-T49 — ✅ **THE JOURNALIST'S n18 — THE ELOQUENCE PASS.** (2026-08-09, same session as D-T48)

**Scott's register, verbatim:** *"We want the voice of one of the Athletic's dedicated writers
for a team. This is a good writer, who takes pride in their craft. They relish telling the story,
but they understand the facts are the story."* Same-session addition: *"It should also cite which
publications are contributing to the narrative."*

**Gate first:** `sources_any` (OR-check, case-insensitive: ≥1 body names ≥1 of the fixture's
corpus `[source]` tags; authored on 7 of 9 — vague-hype and off-entity file nothing, so no
citation demand) and `total_sentences_max: 10` (a padding backstop over the 8-sentence edition
budget; crude terminal-punctuation count). Both landed BEFORE the prompt edit.

**The edit:** the voice paragraph rewritten to the register (facts carry every line; attribution
woven in — "first reported by ESPN, since matched by Marca"); the n17 "no source lists"
prohibition INVERTED to "credited in prose, never dumped as a list"; arc voicing must be the
writer's own words, never a pasted label — the before-probe (frozen surge fixture, temp 0) had
**"The arc is NEW" verbatim inside a body** and "confirmed by multiple outlets" with none named.
A single worked-example storyline carries the register (ep6: the example teaches what prose
cannot); generic subjects (Leeds/Braga/Carvalho) so it cannot leak content.

**One axis fell to the new idiom, deliberately:** ongoing-saga's `body_excludes:
first reported/first reports` (anti-scoop: a third-month saga must not read as breaking) now
conflicts with legitimate origin attribution ("what Record first reported in June…"). The prose
that tripped it ALSO passed the continuation-voicing axis — the intent is covered there and by
the surviving "out of nowhere" exclude; the two phrase excludes are removed and noted here.

**Reading: 110/110 on oMLX unconstrained (D-T47 path), fixtures re-frozen at n18.** Before/after
on the surge fixture: n17 filed one unnamed-sourced storyline ending "The arc is NEW, with the
last 48 hours…"; n18 files "first reported by Fabrizio Romano and since matched by Kicker and
Bild… Flamengo, meanwhile, is fielding multiple European bids, per ESPN… with Globo noting the
transfer now gathering real momentum after weeks of speculation." Five outlets, three sentences,
arc voiced. Exhibits in the session scratchpad; the production reading rides the post-flip drain.

### D-T48 — ✅ **THE JOURNALIST'S n17 (THE SEPARATION PASS) + THE INVESTIGATOR'S TOPOLOGY CORRECTION.** (2026-08-09, the voices session)

**Scott's brief:** *"remove the legacy work of it needing to tag transfers and pass down
vibes/emotional work. Those are completely separate now. The Journalist is the seasoned best
writer who's writing on the developing narratives around the entity."* The trace found the output
side already clean (n16 removed `article_buckets`; no emotional instructions existed) — the live
coupling was the INPUT: the heat section rendered the Insider's vetted direction/stage as ground
truth. Scott's call: remove entirely.

**n17:** heat section + both contract paragraphs gone from the prompt; `load_transfer_heat` gone
from the loader (single load, no join); `transfer_heat` gone from the input-hash components —
heat movement alone no longer re-triggers narratives (the Insider-side waker now debounces to a
no-op; removing that waker is an open simplification). Voice line reframed: "the seasoned writer
at the table … your column is the developing narratives around it." Vibe's heat usage untouched.

**Gate first (the D-T45 rule):** `card_score` had NO fixture check since n12 — three prompt
variants could have drifted it invisibly. `card_score_min`/`card_score_max` axes now exist
(`ParsedNarratives::card_score()` accessor; a missing score fails any band), authored across all
9 fixtures with deliberately loose direction-asserting bands; `established-story-background` was
still frozen at n10 and re-froze with the rest.

**Readings (oMLX, unconstrained D-T47 path, temp 0):** n16 baseline **78/78** — the first voice
validated on the new runtime — then n17 **96/96** (the 78 carried checks + 18 card bands; surge
78, hype-only 12, fizzled 5, saga 42). Fixtures re-frozen at n17 (2 of 9 carried heat blocks in
their frozen user prompts, stripped).

⚠ **THE TOPOLOGY CORRECTION (Scott, mid-session):** the Investigator belongs on archbox's pinned
3B — easy 3B work on the 1070 Ti, the SIX CHARACTERS alone on the Mac's 14B. It had ridden the
Mac since D-T46 built `ip1` there. Re-routed (`COGNITION_ROUTE_INVESTIGATOR=ministral-3:3b`, no
overrides) and the gate read on the 3B: **8/8** — `Airious "Ace" Bailey` quoted verbatim with
escapes, full occupation phrases, the club-page triage right. The D-T46 cost note ("never
archbox") is superseded by this measured pass.

### D-T47 — ⛔ **oMLX'S xgrammar PATH CORRUPTS TEKKEN-TOKENIZER OUTPUT AT THE CHARACTER LEVEL — GRAMMAR IS OFF FOR THE OPENAI BACKEND, AND D-T41'S "NO QUIET MIDDLE" IS CORRECTED.** (2026-08-09, the voices session, found by the investigator fixture gate during the flip)

**THE INSTRUMENT WAS THE GATE, EXACTLY AS DESIGNED.** First grammar smoke of the flip:
`eval --task investigator --fixtures` against oMLX (client: the shipped `Backend::OpenAi`,
`json_schema strict` on the wire) read **4/8 — the frozen ollama baseline is 8/8** — and every
miss was a corruption INSIDE a grammar-constrained string:

| fixture | expected (verbatim contract) | oMLX constrained emitted |
|---|---|---|
| legal-name accept | "American professional basketball player" | "American professional **basketbal** player" |
| legal-name accept | teams ["Utah Jazz"] | ["Rutgers", "Scarlet", "Jazz", …] — "Utah" dropped, "Scarlet Knights" split |
| wrong-sport refusal | occupation "ice hockey…" quoted | "ice" — string closed early |
| legal-name accept | `Airious "Ace" Bailey` | "Airious" — closed before the quoted nickname |

**THE PINNING PROBES:**
* **Deterministic:** two eval runs byte-identical on the corrupted fields (the array tail varied
  — batching nondeterminism, the known D-T19 caveat — but "basketbal" never wavered).
* **Cross-model, same tokenizer family:** the 3B, instructed to emit the exact phrase, produced
  the same missing letter: `{"occupation": "American professional basketbal player"}`. Both
  Ministrals ride mistral's tekken tokenizer → the fault is the constrained-decoding path's token
  masking against that vocabulary, not the 14B or its quant.
* **Every constrained mode is the same path:** `json_schema strict`, `json_object`, and the
  vLLM-style `structured_outputs` field all corrupt; `guided_json` is silently ignored. The bug
  is even self-documenting: the 3B once emitted the corrupted value in its answer field and the
  CORRECT spelling in a free-prose note field of the same constrained response — inside one
  generation, the mask bit specific token sequences, not the vocabulary.
* ⭐ **The control that decides everything: the SAME frozen fixture prompt, unconstrained, at
  temp 0, is byte-perfect** — `Airious "Ace" Bailey` with the interior quotes properly escaped,
  "American professional basketball player" whole, `["Utah Jazz"]` exact, valid JSON in a
  ` ```json ` fence. The model was never the problem.

⛔ **THE CORRECTION D-T41 IS OWED: "oMLX structured output either enforces the grammar or errors —
there is no quiet middle" is FALSE.** The quiet middle is corrupted output that still LOOKS
schema-shaped — the worst possible failure for verbatim-containment contracts (`ip1` discards any
field that fails containment against page text, so every corrupted quote would read as a
hallucination and the arm would silently refuse everything it exists to accept). D-T41's own probe
passed because its crown schema happened not to cross a poisoned token sequence — a reminder that
one conforming probe is a smoke, not a warrant.

**THE DECISION: `response_format` IS WITHHELD BY THE `OpenAiClient` (default), contracts ride the
fail-closed parsers.** Grounds: (1) the unconstrained output is byte-perfect on the very contract
that found the bug; (2) every junction already parses fail-closed behind the balanced-brace
salvager (fences tolerated — `find('{')`/`rfind('}')`); (3) a parse/enum failure without grammar
is VISIBLE (failed work row → backoff → dead-letter), where the corruption was silent.
`with_constraint(true)` keeps the grammar wire-path built and test-pinned
(`schema_is_withheld_by_default` locks the production shape) for the day upstream fixes it —
re-enable is a one-line change plus THIS gate re-run.

⚠ **WHAT GRAMMAR-OFF COSTS, NAMED HONESTLY:** D-T43's third clause ("the grammar pins shape,
prose carries meaning — token-free") no longer holds for Mac-routed seats. Enum values and shape
are pinned by NOTHING but the prompt and the parser now: narratives (`format_schema`), sigil
(`format_schema`), transfers (`json_mode`), investigator (`format_schema_raw`). The voices were
already due per-voice tuning THIS session — each voice's pass must now also (a) audit its prompt
for enum/shape guidance the D-T43 era deleted as redundant, and (b) read its fixture gate on the
unconstrained path before its backlog drains. Integer fields should also carry `minimum`/`maximum`
bounds in their schemas for re-enable day — an unbounded constrained integer digit-looped to
`max_tokens` in probing (grammar-legal, still garbage).

**Deploy order (⛔ before any daemon restart):** archbox's routes now name
`_BACKEND=omlx` / `:8000` / `ministral-3-14b` (backup `.env.local.bak-20260809-preomlx`), so the
running binary must include this commit — an OLD binary would send the corrupted grammar path to
production. Upstream: file against `jundot/omlx` with the 3B one-liner repro; xgrammar 0.2.3→0.2.4
in the venv changed nothing (and needed a `/opt/homebrew/lib/libtvm_ffi.dylib` symlink to load —
rolled into the venv as-is, server holds the working state).

### D-T41 — **oMLX IS A PROGRAM, NOT "MLX SERVING". RESEARCHED 2026-08-09 00:20 EDT, ON SCOTT'S CORRECTION.**

⚠ **Written because this session got it wrong first.** D-T34 quotes Scott saying *"switch to oMLX"*
and the session read that as shorthand for MLX-in-general, and began installing `mlx-lm` +
`mlx_lm.server`. **Scott: "oMLX is a program like Ollama that's used on Mac. Research this before
going any further."** He is right, and the distinction changes the plan.

**WHAT IT ACTUALLY IS:** [`github.com/jundot/omlx`](https://github.com/jundot/omlx), **Apache-2.0** —
a macOS **menu-bar app AND headless inference server** for Apple Silicon, currently **v0.5.7**.
* **Serves `http://localhost:8000`.** Endpoints: `/v1/chat/completions` (OpenAI),
  **`/v1/messages` (Anthropic)**, `/v1/completions`, `/v1/embeddings`, `/v1/models`. Admin UI at
  `/admin`; model search + download from HuggingFace in the dashboard.
* ⭐ **`--max-concurrent-requests`, DEFAULT 8, with CONTINUOUS BATCHING.** **This is the whole
  D-T34 win made native** — D-T34 measured MLX **2.13× at 4 concurrent** and noted it *"was still
  scaling"*; production llama.cpp runs `-np 2` and the Mac's client budget is **3**.
* ⭐ **PAGED SSD KV CACHING** — KV blocks persist to disk and are restored when a prefix recurs
  (reported TTFT 30–90 s → 1–3 s on long contexts). ⚠ **Potentially large here for a reason nobody
  has costed yet: every voice sends a FIXED system prompt on every call** (D-T40 measured the
  Editor's at 1,431 tok; the voices' are larger). That is exactly the recurring-prefix case.
  **UNMEASURED — do not book it until measured on our own prompts.**
* Install: `brew tap jundot/omlx https://github.com/jundot/omlx && brew install omlx`, or a signed +
  notarized DMG. `omlx serve` / `omlx start|stop|restart`; the formula ships a `service` block, so
  `brew services` can run it headless. Requires macOS 15+, **Python 3.11–3.13**.

*(⛔ Superseded 2026-08-09 same day, D-T47: the grammar path CORRUPTS tekken output at the
character level — "structured output works" below was one lucky probe, and grammar is now OFF for
the OpenAI backend. Kept verbatim for the record.)*

✅ **THE STRUCTURED-OUTPUT ANSWER WAS IN THE FORMULA, NOT THE DOCS — AND IT IS OPT-IN:**
`option "with-grammar", "Install xgrammar for structured output (requires torch, ~2GB)"`.
⛔ **INSTALL IT WITH `--with-grammar` OR THREE VOICES LOSE THEIR CONTRACT.** Measured which:
**narratives (`format_schema`), sigil (`format_schema`), transfers (`json_mode`)** constrain
decoding; **vibe, rating, momentum do not.** *(The Insider's identity adjudication also uses
`json_mode` but runs on `EmotionalNews` = archbox/ollama, so it is unaffected.)*
⚠ Also solves a prerequisite clash for free: **the Mac's python is 3.14.6** and oMLX wants ≤3.13 —
the formula `depends_on "python@3.11"` and builds its own venv, so the brew path works where a
`pip install` would have failed.

**WHAT DOES *NOT* CHANGE — THE ARCHITECTURE CALL STANDS.** oMLX is **OpenAI/Anthropic-compatible,
NOT ollama-compatible**, so the protocol gap D-T34 identified is real and unchanged. ⛔ **But do NOT
build D-T34's "shim on :11434".** `route.rs` already has the seam: `Backend` is an enum with one
variant dispatching into `Arc<dyn Inference>`, and the trait is **three methods**
(`generate`, `model`, `request_body`). Its own doc says *"a second impl (vLLM) waits until it is
real, not built on speculation."* **It is real now.** A `Backend::OpenAi` impl is the designed
extension point, keeps `num_ctx` handling explicit and testable, and puts no untested translation
layer in the live network path.

##### ✅ INSTALLED AND PROBED ON THE MAC — 2026-08-09 00:30–00:50 EDT. **THREE ANSWERS, ONE OF THEM A BLOCKER.**

`brew install --with-grammar omlx` → **v0.5.7**, xgrammar present, `python@3.11` venv. Model wired by
symlinking the already-pulled HF snapshot into `~/.omlx/models/ministral-3-14b`; `omlx serve` also
auto-discovers the HF cache (it found a **3B** MLX build already sitting there too). `/v1/models`
reports `max_model_len` **262144**. **ollama's model was unloaded for these probes — the two cannot
co-reside — and reloaded afterwards.**

**1. ✅ STRUCTURED OUTPUT WORKS — the risk is retired.** `response_format: {"type":"json_object"}`
returned valid JSON; `{"type":"json_schema", strict:true}` with a crown-shaped schema
(`{reading, score}`) came back **exactly conforming and parsing clean**. So narratives, sigil and
transfers keep their contracts — *provided the `--with-grammar` build is the one installed.*

**2. ⭐ THE SILENT-EVICTION CLASS DOES NOT EXIST HERE — IT FAILS LOUDLY INSTEAD.** This is the
single most valuable difference from llama.cpp and it is the direct antidote to D-T35/D-T40. An
oversized prompt returns **HTTP 400** with a named code:
`prefill_memory_exceeded` — *"Prefill context too large … pre-chunk guard at 7136 tokens … predicted
peak would exceed prefill safety cap 10.7GB (90% of metal_cap 11.8GB)"*. **And at every size that
PASSES, the head is intact:** a secret code placed at the very start was recalled correctly at
1,394 / 2,549 / 3,869 / 5,189 / 6,835 tokens. **Either the whole prompt is evaluated, or the request
errors — there is no quiet middle.** ⚠ *(This is the probe D-T40 botched by fighting the JSON
contract; here the marker IS the requested output, which is why it works.)*

**3. ⛔ THE BLOCKER — THE PREFILL CEILING IS BELOW THE NARRATIVES PROMPT.** Measured on this 16 GB
M4, Metal capped at 11.84 GB:
| memory guard | largest prompt accepted | rejected at |
|---|---|---|
| default | **5,189 tok** ok | **7,136 tok** |
| `--memory-guard aggressive` | **6,835 tok** ok | ~9,300 tok |

**`narratives` is ≈7,574 tok and `vibe` ≈6,437 (D-T35).** So **narratives would HARD FAIL on oMLX
today**, and vibe sits right on the line. ⚠ **This INVERTS the cost of the voice diet: on ollama an
oversized prompt is silently degraded; on oMLX it is a 400.** The trim stops being a quality nicety
and becomes **a precondition of the migration.** Three levers, in order of preference:
**(a) TRIM THE PROMPTS** — already mandated by D-T35 and Scott's order item (D), now load-bearing;
**(b) `--memory-guard aggressive`** — free, buys ~1.6k tokens, already measured above;
**(c) raise `iogpu.wired_limit_mb`** — the error message names it; **kernel-level and needs sudo, so
it is SCOTT'S call, not an agent's.**

**4. ⭐ THE PAGED KV CACHE IS DEMONSTRABLY HITTING.** `usage.prompt_tokens_details.cached_tokens`
climbed **512 → 512 → 1,280 → 3,840** across probes sharing a prefix — **74% of a 5,189-token prompt
served from cache.** ⚠ Still not a booked number for production: these probes shared a synthetic
prefix by construction. **The real test is whether a voice's FIXED system prompt caches across
different entities' calls** — that is the measurement to run, and `cached_tokens` is the instrument
to run it with. It also gives the Rust backend something worth logging to the ledger.

**API SHAPE FOR `Backend::OpenAi` — confirmed from live responses.** `usage.completion_tokens` →
`GenerateResult.eval_count`; `choices[0].message.content` → `response`; `model` → `model`;
`usage.total_time` (seconds) → `total_duration`; plus `cached_tokens` worth carrying for (4).

**TWO COSTS TO CARRY IN, BOTH UNRESOLVED AT WRITING:**
1. ⛔ **`num_ctx` HAS NO OpenAI EQUIVALENT** — D-T34's cost #2, still unpaid. `max_tokens` maps from
   `num_predict`; the *context window* does not map. **Whether oMLX exposes it per-request is
   UNVERIFIED.** ⚠ It may also be moot in a good way: MLX grows KV as needed rather than
   pre-allocating a fixed window, so **D-T35/D-T40's silent-eviction class may simply not exist on
   this runtime** — which would be a correctness win, not just a speed one. **Test it; do not
   assume it.**
2. ⚠ **16 GB, AND THE TWO CANNOT CO-RESIDE.** The Mac's ollama holds `ministral-3:14b` at **8.8 GB
   resident, `keep_alive` 24 h**; the MLX 4-bit build is **7.9 GB** (already pulled to the HF cache).
   8.8 + 7.9 > 16, so **cutover means unloading ollama's model, not running both.** Scott has
   authorised the disruption explicitly.

### D-T40 — ⛔ **THE EDITOR IS THE NEXT D-T35, AND ITS OWN CODE COMMENT SAYS THE OPPOSITE.** (measured 2026-08-08 23:40 EDT, on ministral, on live production prompts)

**Scott asked to start character tuning at the Editor and to drive ctx down. The measurement
inverts the second half of that: the Editor's window is not too big — its PROMPT is too big for
the window.** Measured from `cognition_ledger.built_prompt` (26,837 real calls, 7-day window) with
`prompt_eval_count` read back from the live ministral runner at production's `num_ctx=4096`.

**THE FIXED COST, BEFORE ONE WORD OF ARTICLE:**
| | tokens |
|---|---|
| chat-template floor (empty call) | **554** |
| `EDITOR_SYSTEM_PROMPT` (8,312 chars) | **1,431** |
| **subtotal, paid on every one of ~27k calls** | **1,985 — 48% of the 4,096 window** |
| `EDITOR_NUM_PREDICT` output | **900** |
| **left for the article** | **1,211 tokens ≈ ~5,700 chars** |

**But `EDITOR_MAX_MODEL_CHARS` admits 9,000 chars of body, and the measured prompt p50 is 7,314
chars.** The budget and the cap disagree by roughly a factor of two, and the cap wins.

**MEASURED PROMPT SIZES (`system` + user + template, as production sends them):**
| percentile | user chars | `prompt_eval_count` | + observed output | vs 4096 |
|---|---|---|---|---|
| tiny (control) | 56 | 2,022 | +288 | fits |
| p50 | 7,314 | 3,548 | +365 = 3,913 | fits, 183 spare |
| p90 | 9,316 | 4,038 | +456 = 4,494 | ⛔ **over by 398** |
| p100 | 10,185 | **4,096 exactly** | +900 = 4,996 | ⛔ **prompt itself CLAMPED** |

**p100 landing on 4,096 EXACTLY is the signature.** Its true length is ~4,742 tokens
(554 + 1,431 + 2,757); the runner reported exactly the window size. **~646 tokens were discarded
silently, `error: None`** — D-T35's mechanism, on the top of the funnel.

**EXPOSURE, bounded honestly rather than to the scarier number:** overflow depends on how long the
reply actually is (measured 365–900 tokens). **45.5% of calls** overflow at a typical ~450-token
reply; **68.1%** if the full 900 reservation is used. **The prompt alone is clamped on 0.01%.**
Either way this is not a tail — it is the common case.

⛔ **THE CODE COMMENT AT `editor/mod.rs:42-43` IS WRONG TWICE** and would have sent the next
session the wrong way: *"largest 24h prompt 9,731 chars = 2,049 tokens through gemma3's tokenizer,
+ 900 = 2,949, leaving 1,147 tokens of headroom."* It (a) is computed on **gemma's** tokenizer for a
runner that is now **ministral**, and (b) **never counts the system prompt at all** — the single
largest fixed term. There is no 1,147-token headroom; at p90 there are **82** tokens, and then it
goes negative during generation.

⚠ **A STANDING ASSUMPTION NEEDS A CAUTION, NOT YET A CORRECTION.** Editor article text tokenizes at
**~4.6–4.75 chars/token under ministral** — essentially the same ratio D-T35 measured for gemma.
The "ministral is 32% denser" figure (D-T31: 2,705 vs 2,049) was measured on **voice-prompt text**,
which is more structured. **This is not a same-text comparison, so it does not refute D-T31 — but
do not assume the 32% penalty applies to article bodies.** Measure per prompt family.

⚠ **THE BEHAVIOURAL PROBE FAILED AND PROVES NOTHING — recorded so nobody re-runs it expecting an
answer.** D-T35's secret-code-in-the-head trick was repeated here; the code was **not echoed at ANY
size, including the 2,022-token control that comfortably fits.** The Editor's system prompt drives
hard toward a JSON envelope and simply overrode the injected instruction. **The numeric evidence
above stands on its own and does not depend on this probe.** A working probe would have to hide the
marker INSIDE the required JSON contract (e.g. a mandated field value), not fight it.

✅ **THE GOOD NEWS, AND IT DECIDES THE ORDER: AN EDITOR PROMPT BUMP IS RETROACTIVELY FREE.**
`read_is_current` (editor/mod.rs:761) makes `contract_version` a T1 cache key, so `ep1→ep2`
invalidates every stored read — **but nothing re-enqueues on it.** The Editor's `pipeline_work` item
is written **only by Go at ingest**, for fresh articles, under D-T21's cap (`news.go:capFreshReads`);
there is no contract-version sweep. **So a bump changes how NEW arrivals are read and re-reads
nothing.** This is the opposite of narratives, where the debounce hash regenerates the whole fleet
(§S-NEW). **The Editor is both the top of the funnel and the cheapest character to tune — start
here, exactly as Scott said.**

##### ✅ FIRST TRIM MEASURED, AND A SECOND FINDING UNDERNEATH IT — 2026-08-09 01:00–01:25 EDT

**THE GRAMMAR ALREADY ENFORCES WHAT THE PROMPT'S LAST PARAGRAPH SPELLS OUT.** `EDITOR_FORMAT_SCHEMA_RAW`
(2,043 chars) pins the **exact key order** — all 11 keys — **and every enum** (`page_kind` 6,
`kind_hint` 4, `role` 4, `story_type` 7, `register` 5). The system prompt then closes with a literal
`Return strict JSON only, with the keys in exactly this order: {…}` template restating both. **The
model cannot emit a different shape, order, or out-of-enum value — constrained decoding forbids it.**
So the template is belt-and-braces over a structural guarantee.
* **Measured saving: 981 chars → 235 TOKENS PER CALL** (system prompt 1,985 → 1,750 incl. the 554
  template floor), on ~27k calls/week.
* **Effect on the overflow, stated honestly: 68.1% → 55.4% worst-case. THAT IS PROGRESS, NOT A FIX.**
  The prompt is still far too large for the window; `names` (FIELD 2) and `entity_roles` (FIELD 3)
  are together ~50% of what remains and can only be cut by ablation + scoring, not by inspection.
* ⭐ **Bonus: it removes an UNTESTED coupling.** `prompt.rs`'s module doc says *"The literal template
  … must match `editor_format_schema`'s order exactly"* — but the only test pins the SCHEMA's order
  against a hardcoded list; **nothing checks the template against the schema.** They agree today by
  luck. Deleting the template deletes the drift risk.

**⛔ SECOND FINDING — THE OUTPUT RESERVATION IS UNDERSIZED, AND THE EDITOR IS FAILING 3.33% OF CALLS.**
Measured `editor_reads.read` through ministral's tokenizer (7-day window, successes only):
| | p50 | p90 | p99 | max |
|---|---|---|---|---|
| output tokens | 490 | 784 | **921** | **940** |

**`EDITOR_NUM_PREDICT` is 900.** The p99 and max outputs are ABOVE the reservation. In the same
window the ledger shows **894 `fail_closed` (3.33%)** and `editor_reads` shows **906 `parse_failed`**
— the same ~900 articles. ⚠ **With a GRAMMAR enforcing shape, the main remaining way to emit
unparseable JSON is to be CUT OFF before the closing brace**, which makes truncation the leading
suspect for those failures.
⛔ **BUT IT IS A HYPOTHESIS, NOT A MEASUREMENT — THE TEST TIMED OUT AND HAS NOT BEEN RUN.** The
direct experiment is written and ready (`/tmp/trunc.py` on archbox): replay a `fail_closed` row's
`built_prompt` at `num_predict` 900 vs 1400 and see whether the 1400 run parses. It needs ~100 s per
call on this card, so it wants the daemon stopped and a clear window. **Do not act on this until it
is run** — and note the `editor_reads` figures are jsonb-normalized text, not the raw reply bytes,
so they are indicative of the raw output size, not identical to it.

**IF IT CONFIRMS, THE TWO FINDINGS PAY FOR EACH OTHER:** spend ~150 of the template trim's 235
tokens on `num_predict` 900 → ~1,050 (clearing the measured 940 max), and the Editor still nets
**+85 tokens of article headroom AND ~900 fewer failed reads a week.** That is one coherent change
with two measurable effects, not two guesses.

**TOOLING THIS REQUIRED (shipped, `aa5e64a`):** `eval --task editor --fixtures` **could not have
scored any of this.** `run_one_fixture` overwrote the options' system prompt with the fixture's
FROZEN copy unconditionally, so a prompt edit was never actually sent — an A/B would have scored the
old prompt twice and reported "no difference." **`--live-system`** sends the current source constant
instead, holding `user_prompt` and `expect` fixed so the prompt is the one variable, and the run
banner now states which prompt was used. ⚠ **D-T19's condition still binds: daemon stopped, and diff
the per-check table, never the score** — greedy decode is not deterministic on a busy GPU.

**WHAT THE FIX IS, IN ORDER (D-T35's law applies with force — TRIM BEFORE SHRINKING):**
1. **Trim `EDITOR_SYSTEM_PROMPT` (1,431 tok, 8,312 chars).** Paid on every call, ~48% of the fixed
   cost. Every token removed is bought back for the article on all 27k calls.
2. **Re-derive `EDITOR_MAX_MODEL_CHARS`** from the budget that is left instead of the 9,000 it
   asserts today — the two currently contradict each other.
3. **Only then** revisit `EDITOR_NUM_CTX`. ⛔ **Lowering 4096 today would make this strictly worse**,
   and any change moves `route::LOCAL_STAGE_NUM_CTX` for graph and the Insider too (shared runner).
4. Score every step on `eval --task editor --fixtures` — the gate exists and the fixtures are
   two-directional (7 cases, both accept and reject).

### S-NEW · PHASE 9 LEFT TWO TUNING-SHAPED ITEMS BEHIND (2026-08-08, from the 9.1 demolition)

**Both are here rather than in the rail file because both are MEASURED changes wearing a deletion's
clothes.** §0 rule 4 applies to each on its own.

**1. The embedder / `threads` cosine clustering in the narratives path — Appendix A's last Rust
item, deliberately NOT executed.** Retiring it changes *what narratives reads* (its pre-model corpus
clustering and thread-identity centroid), so it changes narratives' OUTPUT **and** its
`input_hash` — a **fleet-wide regen**, the same cost the 9.1 extraction was done specifically to
avoid. **The measurement it owes, before anyone deletes a line:** how many corpus items does the
clustering actually collapse per entity today, and what does narratives produce for the same entity
with it off? `narrative_threads.centroid` keeps being written until that is answered.
⚠ **It is also the last consumer of the CPU embedder**, so this item — not any model change — is
what decides whether `Harness.embedder` and the candle dependency can leave the tree.

**2. The narratives debounce pre-image is now a NAMED, DEFENDED thing — treat it as a contract.**
9.1 found `reading_fingerprint` + `build_article_reading_input_components` living inside the module
it was demolishing, feeding `article_readings_hash` into
`journalist::build_narratives_input_components`. **Anything that changes that string re-runs
narratives for every entity**, exactly like a `NARRATIVES_PROMPT_VERSION` bump, and it does so
*without* looking like a prompt change in the diff. The functions now sit in `journalist/mod.rs`
under a ⛔ comment saying so. **Relevant to the voice diet (D-order item D): when the narratives
prompt is trimmed, the version bump and any pre-image change should land in ONE act, so the fleet
regenerates once rather than twice.**

### WHY THE TAG SYSTEM IS THE REFERENCE SHAPE

**Scott, 2026-08-06:** *"The tag system is pretty dramatically better than our old one. Much more
organized."* Recorded here because it is the standard the schema session should measure everything
else against. Concretely, what makes it better is copyable:

| the old `bucket` shape | the tag shape |
|---|---|
| ONE value per article (`transfer` XOR `injury`) — a story could only ever reach one voice | a SET, so one story reaches the Insider and the Influencer at once |
| the routing decision was **code** (a match arm) | the routing decision is **data** (`stage_routing_subscriptions`) — a new voice is an INSERT |
| the classifier decided **relevance** deterministically | the Editor **describes**; the character decides validity (T2, describe-then-derive) |
| adding a voice meant a deploy | adding a voice means a row |

**The schema session's real question is therefore not "what can I delete" — it is "where else is a
routing or judgment decision frozen into a column or a match arm that should be a row?"** That is
the right-shape question with a worked example attached.

---

## S1 · CARRIED FROM D-T22 — found, measured, deliberately NOT done

### S1.a Indexes — ~21 MB, never scanned in an 11-day window
*(Stats window is genuine: `stats_reset` is NULL, postmaster up since 2026-07-26.)*

- [ ] `idx_news_articles_feed_rank` — 10 MB, **0 scans**, partial btree. **Open question attached:**
      `collapse_exact_title_duplicates` reads `feed_rank` on every ingest, so 0 scans means the
      planner is seq-scanning instead. **Answer that before dropping** — it may be an index that
      should be *used*, not removed.
- [ ] `idx_editor_reads_resolved` — 7,216 kB, **0 scans**, GIN. Uncoupled; droppable on its own.
- [ ] `idx_news_articles_topic_heat` / `idx_news_articles_bucket` — **COUPLED to D-T24**, go with
      their columns. Listed here only so the count reconciles.
- [ ] `idx_news_articles_routing_tags` — GIN, 936 kB, 0 scans. **COUPLED to Phase 9** (its writer is
      `article_reader/mod.rs:1073`).
- [ ] **NEVER EXAMINED:** every index on the stats/fixtures side. `event_box_scores` carries
      **820 MB of indexes on a 4.1 GB heap** and nobody has looked. This is the largest unexamined
      surface in the database and it is where the real storage answer probably is —
      `momentum_scores` was the predicted win and turned out to be a false lead (its 1 GB index is
      its hottest, 2.7 M scans).

### S1.b Functions — from the pass-4 read of all 77
- [ ] **`assert_provenance_firewall` — WIRE IT (this is D-T26, and it is the cheapest item here).**
      A safety guard with no caller. Firewall verified intact today; the guard is what would tell us
      if it ever stopped being. One line in `cron-narrative-links.sh`.
- [ ] `refresh_entity_name_surfaces` — **COMMENT ON FUNCTION**, marking it as a rebuild that
      DELETEs and does not read `entity_aliases`. Latent, not live (0 surfaces lost today).
- [ ] `backfill_narrative_episodes` — one-shot tool, no caller; also the only reader of
      `source_tiers`. Decide: keep as a labelled tool, or retire with `source_tiers`.
- [ ] `source_tiers` — 13 rows, reader exists but is never invoked. **Correct migration 215's
      COMMENT**, which names the driver without saying the driver never runs.
- [ ] `apply_rate_siblings` — superseded by `rating_datapoints`' inline rate handling. DROP.
- [ ] `box_score_coverage_report` — diagnostic tool, harmless. Label and keep.
- [ ] `_compute_rating_bundle` — dead `comp_facet` CTE, referenced by nothing. Clarity only;
      **Postgres does not execute an unreferenced CTE, so this is NOT a measured performance win.**
- [ ] `compute_event_starline` — `v_balanced` is declared FALSE and never assigned; its facet arm is
      unreachable. An abandoned A/B left switched off in the code path.

### S1.c Demolition sets — do NOT front-run these
- [ ] **PYTHON PRUNE SET** — `provider_entity_map`, `season_recompute_needed`, `provider_seasons`,
      `resolve_provider_fixture_id`, `resolve_provider_season_id`. All live via `seed/` Python,
      still on cron. Scott: *"Python was the old seeding layer… We're going to prune Python in the
      future."* **These read as orphans to every Rust/Go search and are NOT.**
      `season_recompute_needed` being EMPTY is its healthy state — a safety queue for a failure that
      has not happened. Retire WITH the prune.
- [ ] **PHASE 9 SET** — `topic_heat_embeddings` (8,924 rows / 15 MB), `news_article_readings`
      (63,798 rows), `news_articles.routing_tags` + GIN index, and the 30,224 parked `article_read`
      rows. Phase 9 owns the ordering.

### S1.d The right-shape question — 1 of 82 tables answered
- [ ] Asked and answered for the `news_articles` column family only. **Not asked of the other 81.**
      Use the tag-system table above as the template: hunt for routing/judgment decisions frozen
      into columns or match arms that should be rows.
- [ ] **`nba.*` / `nfl.*` / `football.*` schemas were never in scope** — the entire audit covered
      `public` only. Unknown surface, not a clean one.

---

## S2 · LOGGED DURING THE VOICE SESSION

*(Append below as you go. Nothing here yet — the voice session has not started.)*



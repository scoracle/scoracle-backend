# PLAN — Character Tuning (session notes)

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

> ## ⛔ STAGED, COMMITTED, **NOT DEPLOYED** — DO NOT BUILD INTO `rust/bin/` BEFORE SAT 2026-08-08 10:55 EDT
>
> **Scott's ruling, 2026-08-07: "Don't deploy until after Saturday."** The 4096 change is in the
> tree and passing tests, but placing a binary in `rust/bin/` trips the `.path` watcher and
> restarts `scoracle-cognition` — **inside 8.7's 48h watch window**, which already carries two
> confounds (the flip, and D-T21's cap arming at 02:00 Aug 7). A third would make Saturday's
> reading uninterpretable.
>
> **Safe until then:** `cargo build --lib` / `cargo test --lib` / `--target-dir target/debug`.
> **After 8.7 closes:** deploy, then re-measure the Editor's wall-clock and VRAM against the
> pre-change baseline (gemma3:4b resident at **5.3 GB of 8 GB** with an 8192 window).

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

### D-T30 — **MAC CONCURRENCY IS SET TO 1 AND THE CLIENT ALREADY SENDS 3** (measured 2026-08-07)

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


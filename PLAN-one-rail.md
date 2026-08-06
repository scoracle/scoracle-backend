# PLAN — One Rail

**STATE: Phases 0–3 and 5 CLOSED. Phase 4 OPEN-PARKED (unchanged). Phase 7 complete on its
plumbing (junction pass 7.11 + 7.15 outstanding). **PHASE 8 IS BUILT AND ARMED: 8.1, 8.3, 8.4 and
8.5 are DONE; only 8.6 (the flip), 8.2 (the 7-day window) and 8.7 (the 48h watch) remain.**
DEPLOYED @ `e5dc978` (2026-08-06 09:10 EDT) on archbox, still `RAIL=legacy` — nothing user-visible
has changed. **THE PACKET BRANCHES HAVE NOW EXECUTED.** Scott's direction this session: *"I want to
get the new rail into production. Old rail totally shut down. That way we have several days of
actual production to work with on the weekend"* — and *"no tuning as we go."* **D-T14 IS RESOLVED,
path (b):** mig 212 gives mig 206's arm 2 the same subscription gate arm 1 has (an EXISTS on
`stage='narratives'` at that grain — an existence gate, the tag column is NOT read, and NOT a
wildcard), so with the table empty the trigger is inert and packets compile in shadow.
**THE REAL BLOCKER WAS NOT THE TRIGGER — IT WAS THE DESK'S CADENCE:** `desk_sweep` ran after
`drain_all`, which drains every stage TO EMPTY; with 6,096 editor items at ~3/min behind 30,222
`article_read` items, "empty" was 34 hours away and the compiler was never called. The Desk now
runs on its own 60s task (`desk_loop`, DB-only, deliberately not beating the drain's Pulse).
**2,000+ packets compiled at ~200/min, `pk:` rows 0 continuously — the gate held under load**, and
packet 2 reads as the product (15 attributed claims, 12 participants, distinct slice fingerprints).
**§2 MEASURED FOR THE FIRST TIME: clause 1 97.3% PASS** (read it on a COMPLETE day — a partial day
says 30% and means nothing), **clause 4a 0 dead-letters PASS**, **clause 5 4,654 claims / 0 orphans
PASS**, clause 2 partial (181/197; unmeasurable before Aug 6 by construction — first honest reading
Aug 7), clause 3 sample emitted and unscored, **clause 4b FAIL at 43–47/53 and unstable between
identical temp=0 runs → D-T19, a tuning item, not a phase gate — but §2's text asks for 100%, so
Scott must waive it explicitly or it must be scored green; do not let it be waived silently.**
**THE FLIP IS ONE PREPARED ACT:** `sql/prepared/8.6_flip_day.sql` drops the three legacy triggers
and seeds **FIVE** subscription rows (not the two 7.4 prepared — mig 212 means the JOURNALIST needs
a row too) in one transaction with an abort-on-no-op assertion block, **rehearsed against live prod
in a rolled-back transaction and verified to leave prod unchanged**. 8.4/8.5 are deployed and inert
behind `RAIL` on purpose, so flip day is env + restart and no untested path meets production for
the first time mid-cutover. **PHASE 6 STAYS OPEN on 6.7 alone** — window closes ~**Aug 8 22:08
EDT**; `scripts/rail-6.7-bands.sh` said INTERIM at +10.4h (top cluster **19 IN BAND**, **98.6%
attached**) and that is not the reading. New this session: **D-T18** (syndicated near-duplicates
double a fact inside one packet) and **D-T19** (above). OPEN OUTSIDE PHASES: D-T9 ops ONLY ON
SCOTT'S GO; sudo /mnt/data/scratch grant pending on archbox (low priority). Last plan commit:
(this one). Updated 2026-08-06 ~09:20 EDT (packets are real; the flip is armed and waiting on
Scott's word).**

*(Superseded STATE of 2026-08-06 ~08:40 EDT — loose ends closed, packets still 0 — kept verbatim below.)*

**STATE: Phases 0–3 and 5 CLOSED. Phase 4 OPEN-PARKED (unchanged). **PHASE 7 IS COMPLETE ON ITS
PLUMBING — 7.7 landed 2026-08-06 and only the junction pass (7.11 + 7.15) is left in it.**
DEPLOYED @ `2c6b038` (2026-08-06 08:23 EDT) on archbox: `RAIL=legacy`, `VOICE_NUM_CTX=4096`,
11 stages, six voices on ministral-3:14b at the Mac (4096, 3 concurrent). The Mac runs NO worker —
it is the model host; one archbox release is the whole deploy. 400 tests green, clippy clean.
**THE CUTOVER BLOCKER IS DEAD:** the Editor was dead-lettering ~1 article/day on a NUL byte
reaching `news_articles.full_text`; the body is now sanitised where it ENTERS the Editor (upstream
of the hash, the prompt and the candidate-evidence slice, not just the bind), the 3 dead rows were
requeued and all three landed, and **§2 clause 4's editor dead-letter count is 3 → 0**. **7.7 IS
IN AND DEPLOYED:** the Scout's personnel block — still no packet subscription, still no
`Voice::Scout` — carrying the four facts the memory card structurally cannot (team DEPARTURES, the
club a player came FROM, REVERTS, and the since-last-read anchor), proven live on both clubs of
one transfer before it shipped. It is out of `input_hash` (the 7.8 ruling) and `s16` is NOT bumped
(s17 is 7.11's, and its bump spends a fleet-wide regen). **PHASE 6 STAYS OPEN on 6.7 alone:** the
72h window closes ~**Aug 8 22:08 EDT** and the Aug-6 session was 62 hours early, so the reading
was NOT taken. `scripts/rail-6.7-bands.sh` now emits all four bands + the Verify clause and prints
`INTERIM` vs `READING` in its own header — run it after the close. Interim at +10.3h, NOT the
reading: top cluster **19 (in band 15–25:1)**, **98.6% attached**, packets/subscriptions/`pk:`
rows all 0, and a clean T3 preserved contradiction on storyline 7477. **STILL SCOTT'S CALL, NOT
ACTED ON:** the packet branches have never executed (packets 0) — the clean de-risk is D-T14
resolution (b), a migration gating mig 206's arm 2 on subscriptions, THEN shadow compile with
nothing subscribed, THEN read one real rendered packet before the flip. **PHASE 8 PRECONDITION,
do not lose it: seed `stage_routing_subscriptions` IN THE SAME ACT as the flip** — under
`RAIL=packet` the Influencer has no waker until those rows exist. New this session: **D-T17** (a
gzip body reached the model and the column; 1 of 19,140 — the source of 266182's NUL). OPEN
OUTSIDE PHASES: D-T9 ops ONLY ON SCOTT'S GO; sudo /mnt/data/scratch grant pending on archbox (low
priority). Last plan commit: (this one). Updated 2026-08-06 ~08:40 EDT (loose ends: blocker dead,
7.7 in, 6.7 still running).**

*(Superseded STATE of 2026-08-05 ~23:45 EDT — phase 7 deployed, loose ends open — kept verbatim below.)*

**STATE: Phases 0–3 and 5 CLOSED. Phase 4 OPEN-PARKED (unchanged — box-score target URLs wait
for a season). PHASE 6 OPEN on 6.7 alone: its 72h window closes ~Aug 8 22:10 EDT — read the bands
then and close the phase (the Log's backfill numbers are the pre-deploy baseline; the +4-min
checkpoint is not a reading). **PHASE 7 IS ESSENTIALLY DONE AND DEPLOYED @ `f256abb`, 2026-08-05
23:28 EDT.** Live: the six voices are BACK ON after the Aug-3 pause, at `RAIL=legacy` with
**`VOICE_NUM_CTX=4096`** — ministral-3:14b is resident on the Mac at `context_length 4096`, three
concurrent. DONE: 7.1 `RAIL` · 7.2 renderer · 7.3 Journalist packet corpus · **7.5 Insider** (the
pair's identity stays Postgres's, the packet replaces its material) · **7.6 Influencer + E3** (a
charged packet is material, so she can file first; the Journalist-side vibe enqueue is now
legacy-only) · **7.8 Analyst + the crown's per-card cap** · 7.9 memory continuity · **7.10 the
storyline memory lens, mig 211 APPLIED** (inert for storyline-free entities, measured 40/40) ·
**7.12 the voice window as its own dial** · 7.13 graph continuity · **7.14 [DEPLOY]**.
391 tests green, clippy clean with `--all-targets` (which had been broken since 7.1 and is fixed).
**SCOTT'S RULINGS, 2026-08-05 — they shaped everything above:** (1) *"I don't want to focus on the
prompts… get the whole rail in operation and then spend the time going through all the junctions
this weekend"* — so this session moved MATERIAL only; **E5's `DISAGREEMENT` field (7.8), the
Influencer's first-voice contract text (7.6) and the diet (7.11) are DEFERRED to the junction
pass**, no prompt versions bumped. (2) *"We don't need to seed anything until the cutover"* —
**D-T15 is CLOSED by doing nothing**; 7.4's seed moves into Phase 8's one act. (3) *"Run them, but
run them at 4096"* + *"I'm fine with an imperfect output run over the next few days"* — hence
7.12's `VOICE_NUM_CTX` dial, and every reservation/cap now keying on the WINDOW rather than the
rail. **PHASE 8 PRECONDITION, do not lose it: seed `stage_routing_subscriptions` IN THE SAME ACT
as the flip — under `RAIL=packet` the Influencer has no waker until those rows exist.**
**LOOSE ENDS BEFORE PHASE 8 — their own handoff block sits above the phase 7→8 one, and item 1
is a CUTOVER BLOCKER:** (1) the Editor is dead-lettering ~1 article/day on a NUL byte reaching
`full_text` (`invalid byte sequence for encoding "UTF8": 0x0`; 3 rows at attempts≥5) — §2
clause 4 requires 0, so the 7-day window can never go green until it is stripped; (2) **7.7**
(the Scout's personnel-changes block — plumbing, genuinely owed); (3) 6.7's reading expires
Aug 8; (4) the packet branches have NEVER executed (packets=0), and D-T14 still blocks the
obvious shadow test. STILL OPEN IN PHASE 7: 7.7 above,
7.11 + 7.15 (the junction pass). OPEN ITEMS OUTSIDE PHASES: D-T9 ops ONLY ON SCOTT'S GO; sudo
/mnt/data/scratch grant pending on archbox (low priority). **The Mac voice pause is LIFTED** by
ruling (3). Last plan commit: (this one). Updated 2026-08-05 ~23:45 EDT (phase 7 deployed @ `ac131ca`).**

*(Superseded STATE of 2026-08-05 ~23:10 EDT — phase 7 started, 7.4 blocked — kept verbatim below.)*

**STATE: Phases 0–3 and 5 CLOSED. Phase 4 OPEN-PARKED (unchanged — box-score target URLs wait
for a season). PHASE 6 OPEN: 6.1–6.6 DONE and deployed @ `a6c467b`; **6.7's 72h window is
RUNNING and closes ~Aug 8 22:10 EDT** — it could NOT be read this session (the session opened at
22:12, four minutes after the deploy; a distribution over four minutes is noise). The +4-min
checkpoint is healthy and logged: binary up 22:08:27, **17 of 17 linked reads attached since,
0 unattached**, 6,170 storylines, packets 0. PHASE 7 STARTED, nothing deployed — the running
binary is still Phase 6's. DONE: **7.1** `RAIL` (legacy|packet, default legacy, total parse,
carried on the Harness, logged at boot); **7.2** `editor/render.rs` (hard 2,000-token budget,
oldest-first truncation with every drop named, contested pairs marked `⇄` by a mechanical
stem-overlap + opposite-polarity rule with BOTH always standing, and NO `Voice::Scout` variant so
T4 is enforced by the type); **7.3** `load_packet_corpus` selected by rail inside
`load_narratives_material`, returning the SAME `(Vec<CorpusItem>, CorpusExclusions)` so grounding,
impact, the debounce hash and the marker path stay shared code — window+reservation rail-scoped
together (16384/4000 legacy, 4096/700 packet) and `voice_num_ctx(rail)` moving ALL SIX voices at
once (a lone voice at 4096 is the measured reload thrash); **7.13** graph continuity (52.0% of the
last 24h now read the Editor's blurb, 0 legacy fallbacks; the packet-gated Editor→graph enqueue is
dead code with a live test); **7.9** Journalist half verified and pinned (the render replaces the
CORPUS, never the memory; memory stays out of `input_hash`). One deliberate Phase 6 contract
extension: `packets.claims` now persists `story_type`, because the Insider's slice and its
fingerprint must hash the same subset (zero packets exist, so it cost nothing). **Under
`RAIL=legacy` every prompt is byte-identical — pinned by a test.** 380 tests green, clippy clean.
**7.4 IS BLOCKED → D-T15 (needs Scott):** `stage_routing_subscriptions` is read by mig 197's LIVE
`enqueue_voices_on_routing_tags` as well as mig 206's inert packet trigger, so seeding
`('transfer','transfers','team')` starts the mig-197 churn loop on the transfers stage on the
LEGACY rail (`s:` vs mig 175's still-live `t:` on one row); and `'*'` is not a wildcard in either
trigger, so `('charged','vibe','*')` would fan to nobody. Seed written, corrected to two rows,
DELIBERATELY UNAPPLIED at `sql/prepared/7.4_seed_packet_subscriptions.sql`. Cheap partial offered:
seed only `charged`/`vibe` (no competing article-grain enqueue) and hold the transfers row until
Phase 8 drops mig 175. NEXT SESSION: (1) build 7.5/7.6/7.8/7.10 (buildable now — only their
wake-up depends on D-T15), then 7.12/7.14; (2) read 6.7's bands after Aug 8 22:10 EDT and close
Phase 6. 7.11 and 7.15 stay parked by Scott's ruling (model testing waits for the rail) — the
diet's prompt CODE is buildable, its ministral-3:14b re-earn is not. OPEN ITEMS OUTSIDE PHASES
unchanged: D-T9 ops ONLY ON SCOTT'S GO; Mac voice work PAUSED (do not resume without his word);
sudo /mnt/data/scratch grant pending on archbox (low priority). Last plan commit: (this one).
Updated 2026-08-05 ~23:10 EDT (phase 7 started; 7.4 blocked; 6.7 still running).**

*(Superseded STATE of 2026-08-05 ~22:25 EDT — phase 6 deployed, 6.7 running — kept verbatim below.)*

**STATE: Phases 0–3 and 5 CLOSED. Phase 4 OPEN-PARKED (unchanged — box-score target URLs wait
for a season). PHASE 6 OPEN: 6.1–6.6 DONE, 6.7's 72h window RUNNING (closes ~Aug 8 22:10 EDT).
DEPLOYED 2026-08-05 22:08 EDT @ `a6c467b` in the 22:00–00:00 active window — the live Editor
attaches organically (4 of 4 post-deploy reads, 0 unattached-with-links, 0 errors), packets 0,
boot line `packet_compile=false`. SCOTT'S RULING 2026-08-05: the packet fan-out seam is a TUNING
item (D-T14), not a phase gate — "we're building the rail first"; no model testing until the
rail is complete. The Desk is built
(storyline.rs + packet.rs + bin/storylinefill + the worker's Desk pass; 363 tests green) and the
shadow corpus is assembled — 12,571 reads → 6,164 storylines, 25,759 participant edges, 51.0%
attached / 49.0% opened new, ZERO packets. §1b's rule needed TWO measured corrections, both
pinned by fixtures (Log): (1) a storyline's identity is fixed at its SEED cast — with the whole
cast matching, one storyline swallowed 569 of 2,000 reads; (2) `covers_seed()` — the join must
cover half the seed — after a conference listicle's 11-entity key gathered 304 articles. Top
cluster now 109 over 4 days, hand-inspected as ONE saga (Vinicius→Arsenal) with the T3
contradiction intact (ESPN "set to stay" beside Football365 "agreement in principle" beside six
"deal not agreed"), not a merge; the residual saga-bleed is logged as D-T13.
**D-T14, the seam Scott parked:** mig 206's arm 2 fans `narratives` unconditionally, so a
compiled packet would alternate `input_version` with the legacy `article_read` enqueue on one
`pipeline_work` row (the mig-197 churn loop). The flag holds it; Phase 7 lands `RAIL` and seeds
subscriptions; the tuning session decides the shape. Phase 6's Verify line is true as shipped —
because nothing compiles, not because the trigger is inert. NEXT SESSION: (1) read 6.7's 72h
bands at ~Aug 8 22:10 EDT (storylines/day/sport, articles-per-storyline, % attached, hand-inspect
the 3 biggest for wrong merges + one preserved contradiction; the backfill numbers in the Log are
the pre-deploy baseline) and close Phase 6; (2) then Phase 7 — the voices onto packets behind
`RAIL=legacy`, where flipping `COGNITION_PACKET_COMPILE` finally belongs. 6.7 does NOT gate the
start of Phase 7's build. OPEN ITEMS OUTSIDE PHASES unchanged: D-T9 ops ONLY ON SCOTT'S GO; Mac
voice work PAUSED (do not resume without his word); sudo /mnt/data/scratch grant pending on
archbox (low priority). Last plan commit: (this one). Updated 2026-08-05 ~22:25 EDT (phase 6
deployed; 6.7 running).**

*(Superseded STATE of 2026-08-05 ~21:40 EDT — phase 5 closed — kept verbatim below.)*

**Phases 0–3 and 5 CLOSED. Phase 4 OPEN-PARKED (4.1–4.4 done; box-score target
URLs wait for a season — top-5 leagues restart ~Aug 14–15; the pulselive_pl seed
one-liner still awaits Scott). NEXT: Phase 6 (storylines + packets — deterministic code,
zero model calls). PHASE 5 CLOSED 2026-08-05 ~21:40 EDT on Scott's tuning ruling — 5.10
cut to +41h with every band GREEN (full numbers in the Phase 5 Log close entry): Editor
within-24h coverage HELD at 100.0% post-deploy (identical to pre-deploy; >5% bar
untouched); 5.8 hand-check 10/10 accepts clean, 0 false merges (census of all accepts;
20-row protocol re-arms under D-T9); funnel populated end-to-end; the investigator
caught the predicted 01:52–02:00 idle window (70 runs, 11.4% acceptance, every refusal
honest) and the compounding metric jumped 5 → 102 resolver links onto persons (Alonso 59
same-day). Known+accepted (D-T10): steady-state nominations ~3k persons/day vs ~70/day
drain — queue 6,670 and growing; the drain knobs are the tuning session's first
Investigator item. SCOTT'S 2026-08-05 RULING also founded `PLAN-character-tuning.md`
(Character tuning session notes; convention in the Appendix D preamble: ledger = index,
notes = diagnosis, nothing fixed mid-rail) — this session's editor findings recorded as
D-T11 (input hygiene: 34.3% of prompts at the 9k cap, Yahoo 95% with nav-menu chrome,
hex entities undecoded) and D-T12 (output tokens dominate call wall ~19s of 22s;
capacity ~7.8k reads/day vs arrivals ~8–8.4k/day, zero headroom; concurrency verified
real 4×4). D-T1 replay verdict stands (Editor beats legacy per name, combined 83.3% vs
51.9%). OPEN ITEMS OUTSIDE PHASES: D-T9 ops ONLY ON SCOTT'S GO (FULL NBA seed → 20-row
hand-check → widen at season start); Mac voice work PAUSED (Scott; resume recipe in
Phase 5 Log 19:50 entry — do not resume without his word), voice queues accumulating;
sudo /mnt/data/scratch grant still pending ON ARCHBOX (low priority). Last plan commit:
(this one). Updated 2026-08-05 ~21:40 EDT (phase 5 closed).**

*(Superseded STATE of 2026-08-04 ~21:05 EDT: phase 5 mid-flight — 5.9 deployed @78c923a
in the 04:00 clean window, organic verify green at +10min, 5.10 interim at +16.6h
recorded contention TOTAL/coverage deferred/compounding nonzero; details preserved in
the Phase 5 Log entries of 2026-08-04.)*

*(Superseded STATE of 2026-08-03 ~21:35 EDT, kept for the record below.)*

*Phases 0–3 CLOSED. Phase 4 OPEN and BLOCKED at 4.3-seed on a Scott decision:
the 2026-08-03 terms/robots review found EVERY keyless box-score family in BOTH D-4 sports
failing (technical blocks of declared bots / robots disallow / JS-signature walls needing
banned automation / explicit anti-AI license terms) — full review table in the Phase 4
Log; the D-4 NBA fallback fails the same review. Unblock options for Scott are in the Log
(recommended: api-football free tier — data-sufficient, 100 req/day vs measured ~10
completed FOOTBALL fixtures/day, needs his 2-min registration). DONE under Phase 4:
4.1 (junctions/investigator/ founded, Role::Investigator, boxscore_fetch.rs moved —
builds+tests green), 4.2 (BudgetedFetcher in fetch.rs: per-domain 1-conc/≥2s/Retry-After/
circuit-break, source_documents provenance, 7 new tests), 4.3-table (mig 208
boxscore_sources applied + snapshot; SEEDED EMPTY pending the ruling). The 137-article
corpus replay had NOT drained when this session ran (18:19 EDT; queue untouched since the
18:00 rest pause; ETA ~23:00 EDT holds) — D-T1 verdict still owed by a later session.
SUPERSEDED SAME EVENING by Scott's rulings (Phase 4 Log entries, 2026-08-03 ~19:10+):
league-page review found a PASSING family (premierleague/pulselive — seed SQL awaits
Scott, executor's DB-write perms blocked the COMMIT); 4.4 nomination BUILT+WIRED; then
the target re-scoped — box-score target URLs park until a season starts; the Investigator
system (mystery entities + metadata writes) is the priority, VETTED ON NBA (head coaches
= the persons test class). Phase 5 machinery BUILT the same night (sweep 5.1/5.2, stage
5.3, Wikidata adapters 5.4, gate 5.5, reopen 5.6, 13-case fixture gate 5.7 at 100%, review
views 5.8 = mig 210; migs 209/210 applied+snapshotted; commits 12338b7/c852588/c49c8f2).
v1 Investigator makes ZERO model calls (Wikidata structured claims; gemma prose triage →
Appendix D follow-up). Smoke seeded: 3 NBA players + Spoelstra/Kerr candidates pending in
investigate_entity. NOT deployed ([DEPLOY] 5.9 still open; COGNITION_STAGES unchanged).
SMOKE: GREEN after three measured iterations (Phase 5 Log) — Spoelstra+Kerr accepted as
persons kind=coach with coach_of edges + sourced aliases; Şengün enriched (dob/height/
headshot from his real NBA.com id); 9 team wikidata mappings bootstrapped; Bailey +
A.J. Green refused honestly (D-T6/7/8 logged). Three defects found+fixed+pinned in the
smoke (wrong coach QID — Q13365117 is HANDBALL player, real one Q5137571; role priority
P6087>coach-occ>player-occ with P582 ended-tenure filtering; excerpt-bound vs
containment on >100k Wikidata JSON). OPERATIONAL: Mac character work PAUSED by Scott
(COGNITION_STAGES narrowed to scrub,graph,editor,article_read; backup env at
/tmp/env.local.bak-voice-pause on archbox; ministral unloaded; voice queues accumulate).
SESSION CLOSED on Scott's ruling (~21:30 EDT): plumbing is IN; the meta-gathering RUN is
parked as Appendix D-T9 (follow-up ops, not construction); 5.1–5.8 ticked, 5.9's
tests/fixture arm done, its [DEPLOY] arm + 5.10 stay open. NEXT SESSION: (1) housekeeping
— the D-T1 paired replay verdict (drain finished overnight; recipe pinned in D-T1);
(2) 5.9 [DEPLOY] in a clean window (04:00–06:00 EDT; carries investigate_entity + the
4.4 fork + the sweep live); (3) on Scott's go, D-T9 ops (FULL NBA seed → 5.8 hand-check →
5.10 72h readings). Mac voice work PAUSED (Scott; resume recipe in Phase 5 Log 19:50
entry). Sudo /mnt/data/scratch grant still pending ON ARCHBOX (low priority).
Last plan commit: 78c923a. Updated 2026-08-03 ~21:35 EDT (session closed).*
*(Phase 0 findings that bind later phases: (1) §0.8 rewritten — `psql` runs on **archbox** over ssh;
the Mac has no psql and empty DB URLs. (2) The **archbox checkout is behind this repo** (`cec766a`),
with migrations 198/199 untracked there — **sync it before Phase 1 runs `sql/migrate.sh`**.
(3) **D-1 is closed by Scott, 2026-07-29:** `ministral-3:14b` on the Mac does all character work
except the Editor and the Investigator; `gemma3:4b` is **pinned** on archbox for those two
(`OLLAMA_KEEP_ALIVE=-1`, `MAX_LOADED_MODELS=1`, `NUM_PARALLEL=4`); `mistral-nemo:12b` is installed
but **unused**. See §3 and Appendix B D-1.)*
*(Executors: keep this line current — phase pointer, last commit hash, date — every commit.)*
*(Revised 2026-07-29, pre-execution audit: OLMo removed — measured, it does not hold the 4
slots on the 1070 Ti; front page deferred to Appendix B D-6; teams.kind migration deferred
into D-3; duplicate short-circuit added in 3.4. Capacity corrected in 3.9 — the legacy
~7,400 reads/day was DEMAND-limited under the 2h-on/1h-off rest schedule with the card mostly
idle, not a ceiling.)*
*(Revised 2026-08-01 — THE CHARACTER-FLOW REVISION. Scott's vision session; wiki docs
synthesized: living-database-seeker, Character Contracts, Characters, AI Stage Conventions,
Progressive Refinement Dataflow. What changed: (1) phases REORDERED — box scores are now
Phase 4, before entity discovery (Phase 5) and storylines/packets (Phase 6): the cord is cut
and the Scout starves on frozen stats. Mapping old→new: 5B→4, 5A→5, 4→6, 6→7, 7→8, 8→9.
(2) D-4 sport CLOSED: FOOTBALL. (3) Investigator enqueue: immediate on person+descriptor
(5.2). (4) `persons.kind` gains `player` (1.4, D-2). (5) Phase 7 gains the memory loop, the
voice diet (RAIL-scoped prompt versions), the storyline memory lens, and the graph seam —
graph's mig-193 enqueue dies with `article_read` at flip and would have silently starved the
archivist; demolition would likewise have killed the episode rollups every memory card reads
(9.5 now SPLITS cron-narrative-links.sh). (6) Four new §4 rulings: memory is characters-only;
stats before the Scout; graph rides Gemma; empower the junction — Google does relevancy up
front. Flow diagram rewritten with machines + tables.)*

**THE GOAL (Scott, 2026-08-03 — read this before any phase):** We are dramatically
simplifying so we can empower our models better. The deliverable of this plan is the
PLUMBING — a pipeline as simple, lean, nimble, and durable as possible, where data flows
naturally and frictionlessly and the models are trusted for the work at their junctions.
**Tuning is the next step, AFTER the rail stands** — junction quality items go to the
Appendix D tuning ledger and are refined on clean plumbing later, not litigated mid-build
(§4 ruling). The old rails drowned in scar tissue precisely because quality was battled
in place instead of tuned on a clean substrate. Do not rebuild that habit here.

Written 2026-07-28. This is the build order for the greenfield rail decided in
[`HANDOFF-newsroom.md`](HANDOFF-newsroom.md). Read that file's §1–§3 before touching anything —
it is the case for this plan and the map of the rot this plan must not rebuild.
[`PLAN-ingest-simplification.md`](PLAN-ingest-simplification.md) stays the reference for the
**traps (T1–T13)** and the measurements; its build order is dead.

The two rails — news and stats — collide into one:

```
LUNGS · archbox · Go · 02:00 cron
  Google News RSS, one ranked query per team — Google does relevancy up front
  → news_articles (query saved in raw.q)                    ⇒ enqueue editor
     |
     v
HEART · archbox 1070 Ti · gemma3:4b PINNED · 4 slots (Editor + Investigator + graph)
  THE EDITOR (stage editor) — fetch body → describe (ep1) → code derives
  → news_articles.full_text, editor_reads
  forks, in-handle:
    result_line parses           ⇒ fixtures upsert → trigger → fixture_boxscore
    unknown person + descriptor  ⇒ entity_candidates → investigate_entity
    every successful read        ⇒ graph
  THE DESK (pure code, zero tokens) — assemble + tag the stories of the day
  → storylines / storyline_articles / storyline_entities → packets (append-only)
  tags fan out ⇒ narratives (always) · transfers ('transfer') · vibe ('charged')

  THE INVESTIGATOR (same pinned gemma, below the Editor)
  A · box scores (fixture_boxscore): boxscore_sources URL → budgeted fetch
      → DOM/JSON parse (never LLM numbers) → source_documents + landing row
      → promote in ONE tx: event_box_scores + event_team_stats + finalize_fixture()
      → percentiles → LISTEN percentile_changed ⇒ peak      ← stats before the Scout
  B · mystery entities (investigate_entity): Wikipedia/Google discovery
      → budgeted fetch → gemma DESCRIBES → code GATES
      → persons / entity_aliases / entity_facts / entity_relationships
      → tomorrow the resolver links the name (the living database compounds)

  GRAPH (stage graph, gemma) — the archivist banks typed relations + people
  → narrative_events / narrative_persons → nightly rollups → episodes
  (these feed every character's memory card)
     |
     v
BRAIN · Mac 192.168.1.77 · ministral-3:14b · 3 concurrent · num_ctx 4096 uniform
  each voice's window: prompt ~550 + memory ~700 + packet ~2000 + output ~800
  Journalist  narratives  every packet             → news_summaries
  Insider     transfers   'transfer' packets       → transfer_rumors, insider_scores
  Influencer  vibe        'charged' packets        → vibe_scores
  Scout       peak        STATS ONLY, never prose  → stat_summaries
  Analyst     momentum    peak/vibe moved          → momentum_summaries
  Oracle      sigil       BLIND: 5 cards + own trail → sigil_synthesis
  every voice reads its memory card in, banks its read back — memory never
  enters input_hash; packets re-fan only when a voice's slice fingerprint moves
```

**Naming ruling (Scott, 2026-07-28): the acquisition character is THE INVESTIGATOR.** Older docs
(wiki Living Database, the 2026-07-27 planning docs) say "the Seeker" — same character, old name.
Code, tables, and stages use `investigator`/`investigate_*`. Wiki updates land in Phase 9, not
before.

---

## §0 — How to work this plan

This plan is executed one phase per session, by smaller models, on mobile. The protocol:

1. **Read first:** the **GOAL** block at the top (simplify to empower the models; tuning
   comes AFTER the rail stands), this section, the **STATE** line, the phase you are
   executing, and any appendix that phase names. Do not read the whole repo. `HANDOFF-newsroom.md` §3 and the Traps section of
   `PLAN-ingest-simplification.md` are the background; consult them when a step cites T-numbers.
2. **Work the phase top to bottom.** Checkboxes are ordered by dependency. Do not reorder, do not
   start the next phase.
3. **STOP on surprise.** If a Verify step returns a number outside its stated band, or a named
   file/function/table does not match what the step claims: stop, write what you found in the
   phase's Log, commit, and emit the resume block with a `BLOCKED:` line prepended. Do not
   improvise a fix. Only measurement has ever settled anything here — do not ship a theory.
4. **One change, one measurement** (ar4/ar5 lesson). Never bundle two behavior changes in a step.
5. **Migrations:** follow `sql/migration_template.sql` (BEGIN; DDL; self-record into
   `schema_migrations`; COMMIT). Apply with `sql/migrate.sh`. Then run
   `scripts/hosting/snapshot-schema.sh` and **commit the migration + snapshot together**. Current
   highest migration is **199**; number from 200 up (176 is a known gap — leave it).
6. **Deploys are explicit.** Building into `go/bin/` or `rust/bin/` trips the `.path` watchers and
   restarts services, overriding the rest windows (harness pauses 00/03/06/09/12/15/18/21:00 +1h).
   For tests and rehearsals build to `target/debug/`. Only steps marked **[DEPLOY]** place
   binaries. Services are user systemd units on archbox (`systemctl --user`).
7. **Data writes are rehearsed** in a rolled-back transaction first, with the invariant asserted
   inside the transaction (the `remap -rollback` habit). Any write that touches
   `news_article_entities.vetted` must suppress triggers with
   `SET LOCAL session_replication_role = 'replica'` (T10) — and never
   `ALTER TABLE ... DISABLE TRIGGER` (ACCESS EXCLUSIVE against a live pipeline).
8. **DB access — from archbox, not the Mac** (corrected by Phase 0). Postgres 18.4 (`scoracle`,
   datadir `/mnt/data/postgres/data`) runs on archbox; the Mac has **no `psql` installed** and its
   `.env` carries **empty** `DATABASE_URL`/`DATABASE_PRIVATE_URL` (no `.env.local` on the Mac — only
   a `.bak`). The working incantation from a Mac session is:
   ```
   ssh archbox 'cd ~/scoracle/scoracle-backend && set -a; . ./.env.local; set +a; \
     psql "${DATABASE_PRIVATE_URL:-$DATABASE_URL}" -c "select 1"'
   ```
   (Phase 1 correction: archbox no longer has a `.env` at all — the env consolidation, commit
   `94a8a61`, left `.env.local` as the one file carrying the DB URLs. Sourcing `./.env` there
   now errors harmlessly; don't.)
   `~/.ssh/config` already defines `archbox` (192.168.1.92, user `sheneveld`, key auth — BatchMode
   works). Prefer `DATABASE_PRIVATE_URL`, as `sql/migrate.sh` does.
9. **Finish ritual, every session:** tick the checkboxes you completed, fill the phase Log with the
   numbers you measured, update the STATE line, commit everything (plan file included) as
   `rail: phase <N> — <short description>`, then print the next phase's **resume block** from the
   phase's Handoff fence, verbatim, as the last thing in your reply so it can be copied with one
   tap.
10. Long-poll background watchers get killed in this environment — use point-in-time checks.

**The non-negotiable design law (T2): describe, then derive.** A local model never renders a
verdict, a routing decision, or an identity match as a bare field. The model describes what the
text/page says; code computes the judgment. Every contract in this plan is shaped by this.

---

## §1 — The packet (the contract; everything else is knobs)

Per HANDOFF §7, the contract comes before any schema. The **packet is the product**: one
storyline, multi-tagged, entity-indexed, append-only.

### 1a. `editor_reads` — one row per article, the Editor's read (contract `ep1`)

The greenfield successor to `news_article_readings` (which the legacy `article_read` stage keeps
writing until cutover; two writers must never share one table, so this is a new table).

| column | meaning |
|---|---|
| `article_id bigint PK` → news_articles | |
| `status text` | same taxonomy as `news_article_readings.status` (success/irrelevant/paywall/blocked/empty_body/fetch_failed/parse_failed/duplicate + `not_sport`) |
| `contract_version text` | `'ep1'` — a cache key, not a label (T1) |
| `model_version, parser_outcome, last_error` | as in legacy |
| `final_url, final_domain, content_hash, extracted_words` | fetch provenance |
| `read jsonb` | the full ep1 model envelope (below) |
| `resolved jsonb` | resolver outcome, written by code: `{links:[{entity_type,entity_id,sport,via_surface}], unresolved:[{name,kind_hint,descriptor}], refused_ambiguous:[...]}` |
| `storyline_id bigint NULL` | attach result (Phase 6 writes it) |
| `fetched_at, updated_at` | |

**The ep1 model contract** (constrained decoding; property order IS the contract — extraction
before anything derived, the ar4 lesson). Fields, in schema order:

1. `source_language`
2. `page_kind` — `article|score_table|listing_or_schedule|video_clip|roundup|other`
3. `names[]` max 12 — **the discovery channel.** Every person and club the TEXT involves:
   `{name, kind_hint: person|club|national_team|other, descriptor}` where `descriptor` is ≤6 words
   **from the text** naming role/club/context ("Real Madrid manager", "PSG sporting director",
   "city hosting the finale"). This merges ar7's `relevant_entities` with F7's describe-facts: the
   descriptor is what lets code refuse `Paris`-the-city → Paris-the-club and route
   `kyle shanahan` → person-candidate instead of a fuzzy player match (T9).
4. `entity_roles[]` — `{entity, role: subject|opponent|passing_mention|absent}` over the provided
   hypothesis list (the query entity + any pre-linked entities). Unchanged from ar7.
5. `story_type` — `transfer|injury|performance|fixture|roster|contract|general` (ar7 enum, kept
   for fixture continuity; hirings are `roster`).
6. `result_line` — the **verbatim** final-score line if the text states a completed result
   (`"Real Madrid 2-1 Arsenal"`), else empty string. Verbatim-or-empty is the describe-then-derive
   shape: code parses it; the model never says "a game happened."
7. `register_phrase` then 8. `register` — phrase BEFORE label (describe → label), enum
   `celebration|outrage|resignation|anticipation|neutral`. The Influencer owns the number; the
   Editor never scores.
9. `key_facts[]` max 8 — one claim per fact, attributable ("The Athletic: deal not agreed").
10. `caveats`, 11. `evidence_blurb`

**Dropped from ar7:** `co_mentions[]` — the headline-candidate voting loop is the redundancy
HANDOFF §1 names; the numbered-candidate mechanism dies with the rail. Derivations (all in code,
none asked of the model): relevance; entity links (exact `nrm()` match on
`entity_name_surfaces` only — trigram ranks for review, never writes, T9); Investigator
nominations (unresolved `names[]` — a person-kind mention WITH a descriptor enqueues on first
sight, Scott 2026-08-01; descriptor-less bare names wait for the 2-mention floor; refused ties
always nominate — the full rule is 5.2); routing tags (from `story_type` + non-neutral `register` →
tag `charged`); box-score nomination (`result_line` parses + `page_kind` ∈ article|score_table);
mention quotes (code slices ±160 chars around the name's first occurrence in the **stored**
`full_text` — the model never emits quotes; bodies are retained now precisely so evidence can be
sliced deterministically).

### 1b. Storylines — assembled in code, never compiled by a model

| table | columns |
|---|---|
| `storylines` | `id bigserial PK, sport, title text` (display only — first member's headline; never used for matching), `status open|resolved|dormant`, `first_seen_at, last_seen_at, resolved_at, resolution jsonb` |
| `storyline_articles` | `storyline_id, article_id` PK pair, `attached_at`, `attach_method auto|backfill` |
| `storyline_entities` | `storyline_id, entity_type, entity_id, sport, role, joined_at, last_seen_at, left_at, exit_reason` — D5: the entity's part has its own lifespan; when a storyline resolves, name who it resolved for and close every other edge as `not_the_outcome` in one stroke, in code |

**Attachment rule (deterministic, logged, no model call):** candidate set = open storylines in the
article's sport sharing ≥1 resolved entity, `last_seen_at` within 14d. Score = entity overlap
count + 1 if `story_type` matches the storyline's dominant type + recency bonus. Attach to the top
scorer above threshold 2; else open a new storyline. Free-text story names are banned from
matching (D3's lesson) — entities + type + time ARE the join key. BGE/cosine is not rebuilt (F2).

### 1c. `packets` — the compiled brief, append-only

A packet is a **snapshot of a storyline at compile time**. Compiled **in code** from member
`editor_reads` (zero model tokens — C5: the Editor's output budget is coverage). A new packet
supersedes the prior one; packets are never edited (archive-as-moat).

| column | meaning |
|---|---|
| `id bigserial PK, storyline_id, day date, sport, compiled_at` | |
| `headline text` | best member title (lowest `feed_rank`) |
| `story_types text[], register, register_phrase` | rollups (register = strongest non-neutral among members, with its phrase) |
| `claims jsonb` | `[{article_id, source, fact, published_at}]` — **contested state is preserved**: "agreement reached" and "yet to reach agreement" both stay, attributed (T3/D6 — the disagreement IS the story; 0.5–0.75 similarity attaches, never collapses) |
| `facts jsonb` | thin, structured only: `{story_types, result_line?, entities}` — the Scout never reads packet prose (T4); confirmed facts reach it by other roads (§4) |
| `quotes jsonb` | code-sliced from stored bodies |
| `routing_tags text[]` | derived: story types + `charged` |
| `slice_fingerprints jsonb` | per-voice hash of that voice's slice (E2): `{narratives: h, vibe: h, transfers: h}` — a packet re-fans only to voices whose slice moved. **Keys are STAGE strings, pinned by mig 206** (the trigger reads `slice_fingerprints ->> stage`; a missing key fails OPEN — re-fans every packet, never starves). Phase 6's compiler writes stage-keyed entries. |
| `unresolved_mentions jsonb` | B3's census, rolled up onto the packet |
| `supersedes_packet_id bigint NULL, contract_version 'pk1'` | |

*(A ranked "front page of the day" model call was audited out pre-execution: the packets ARE the
compiled stories of the day, and a ranking product with no client surface is decoration.
Appendix B D-6 holds the half-day of work for the day a surface exists.)*

---

## §2 — The cutover (defined now, measured later; HANDOFF §7)

**The single condition.** The Journalist reads packets instead of
`load_vetted_corpus_with_exclusions` (`rust/src/junctions/journalist/mod.rs:380`) when, for **7
consecutive days**, all five clauses hold (SQL for each lives in Phase 8):

1. **Coverage:** ≥95% of articles ingested each day have an `editor_reads` row within 24h.
2. **Packet presence:** every (entity, day) that legacy produced a narratives corpus for (≥3
   vetted canonical articles) appears in ≥1 packet's `storyline_entities` that day.
3. **Precision:** a daily 50-link sample from `editor_reads.resolved.links` audits ≥95% correct
   (the B4 flip standard; the legacy rail measured ~95% on flips, ~75% on brand-new).
4. **Gates green:** `eval --task editor --fixtures` passes 100%, and Editor/Investigator
   dead-letter count (attempts ≥ 5) is 0 over the window.
5. **Accounting:** every packet's claims reference member articles only; ledger reconciliation
   finds 0 unaccounted drops (the A5 rule: an article dropped from evidence must be named).

**The flip is one act:** `RAIL=packet` in both machines' env + the Phase 8 [DEPLOY]. Scott flips
it; the harness never auto-promotes.

**What happens to the old rail that day: it stops.** Same session, in order: legacy triggers
dropped, Go stops enqueueing `scrub`, `COGNITION_STAGES` drops `article_read` + `scrub`. Deleted
— not left running in parallel forever. The graph seam moves in the same act: the Editor's
RAIL=packet path takes over `enqueue_graph_for_article` as `article_read` stops claiming (wired
7.13, verified 8.5) — the archivist never starves. Source excision follows in Phase 9 (Appendix
A is the inventory); rollback stays possible for 7 days (env flip back + revert migration in
Appendix A).

**The old corpus is archive.** 150,566 articles, 265,204 links, and every `news_article_readings`
row keep their state forever. No backfill of packets over history. The new rail is forward-only
from flip day.

---

## §3 — Topology

| organ | host | model | concurrency | notes |
|---|---|---|---|---|
| Lungs | archbox (Go) | none | — | Google News RSS, teams-only sweep, daily 02:00 cron. The query is the hypothesis; Go decides nothing. |
| Heart: the Editor (module `rust/src/junctions/editor/` — greenfield; stage `editor`) | archbox GTX 1070 Ti | `gemma3:4b` — **pinned** (the engine — §4 ruling; OLMo is out) | shares the 4-slot group | `ARCHBOX_GEMMA_SLOTS` (`rust/src/stage.rs:84`) is the pool. The legacy seat is renamed `junctions/article_reader/` (Phase 3.0) and dies in Phase 9. |
| Heart: the Investigator (module `rust/src/junctions/investigator/`; stages `investigate_entity`, `fixture_boxscore`) | archbox, same card | `gemma3:4b` — **the same pinned instance**, not a second load | same 4-slot group | Scott's call: the Investigator rides the Editor's card |
| Heart: the archivist (module `rust/src/junctions/graph/`; stage `graph`) | archbox, same card | `gemma3:4b` — the same pinned instance (via `Role::EmotionalNews` default route — utility, not identity) | same 4-slot group, rotation batch 8 | num_ctx 8192 matches the Editor (no runner reload). Enqueue seam today: the legacy article_read handler (mig 193); moves to the Editor's RAIL=packet path at flip (7.13/8.5). Scott 2026-08-01: graph rides Gemma — high effort, low thought. |
| Brain: 6 voices | Mac (192.168.1.77) | **`ministral-3:14b`** (13.9B Q4_K_M — Scott, 2026-07-29: all character work except Editor/Investigator; D-1 is now closed) | 3 concurrent to start | `num_ctx` 4096 uniform across all six (mixed num_ctx forces runner reloads, route.rs:52-75) — the window budget in 7.2 exists because of this |

**Gemma's pin is enforced by Ollama's own config, not by convention** (measured Phase 0.11):
archbox's `ollama.service` runs `OLLAMA_KEEP_ALIVE=-1`, `OLLAMA_MAX_LOADED_MODELS=1`,
`OLLAMA_NUM_PARALLEL=4`. Three consequences the rest of this plan depends on:

1. `gemma3:4b` is resident permanently (`/api/ps` shows `expires_at` in the year 2318, 5.34 GB in
   VRAM, `context_length` 8192). No cold-start cost on the Editor's hot path.
2. `MAX_LOADED_MODELS=1` makes "the Investigator rides the Editor's model" a **hardware
   constraint, not a style choice** — routing the Investigator to any other tag would evict Gemma
   on every call and thrash the card. Same reason `COGNITION_ROUTE_EDITOR_CANDIDATE` (§4's future
   bakeoff hook) must only ever run **offline via `bin/eval`**, never against the live rail: a
   challenger load evicts the incumbent.
3. `NUM_PARALLEL=4` is exactly `ARCHBOX_GEMMA_SLOTS`'s 4 — the harness's slot count and Ollama's
   parallel count already agree. If either moves, both must.

The plumbing for all of this **already exists** (verified 2026-07-28): per-role
`COGNITION_ROUTE_<ROLE>_BASE_URL` (`rust/src/config.rs:269`), per-host semaphores via
`COGNITION_BACKEND_CONCURRENCY` `url=permits` (`config.rs:301`, `route.rs:315 governor_for`),
per-call `num_ctx` (`ollama.rs:44`), slot groups (`stage.rs:77`). New roles: `Role::Editor`
(`COGNITION_ROUTE_EDITOR`) and `Role::Investigator` (`COGNITION_ROUTE_INVESTIGATOR`); legacy
`Role::ArticleReader` dies in Phase 9.

Two worker deployments drain one Postgres queue; `COGNITION_STAGES` on each machine decides who
claims what: archbox = editor/graph/investigate_entity/fixture_boxscore (+ legacy stages until
cutover), Mac = the six voice stages.

---

## §4 — Rulings carried into this plan (do not re-litigate)

- **Files are named for characters** (Scott, 2026-07-28). Junction modules carry the character's
  name — `editor/`, `investigator/`, `journalist/`, `insider/`, `influencer/`, `scout/`,
  `analyst/`, `oracle/` — because the wiki, the contracts, and the cast all speak that language.
  Queue stage strings keep product names (`narratives`, `vibe`, `peak`, …; the new `editor` and
  `investigate_entity` stages happen to align). **Renaming a live identifier is still forbidden**
  (T1/F1): stage strings, env keys, prompt versions, and table names never change as part of a
  file move. Files are safe to rename; identities are not.
- **Gemma 3 4B is the Editor's engine — settled by hardware.** OLMo was tried and does not hold
  the 4-slot concurrency on the 1070 Ti (Scott, 2026-07-29). Concurrency IS coverage, so holding
  4 slots on the card is a precondition any future challenger must meet before quality is even
  measured (`COGNITION_ROUTE_EDITOR_CANDIDATE` exists for that day). No bakeoff is scheduled.
- **The regex tier goes entirely** — not demoted to clerk. The Google query is the only
  hypothesis Go contributes, recorded as the primary link (`match_confidence 0.95`,
  `go/internal/thirdparty/news.go:350`). HANDOFF §1.
- **Known consequence of that deletion** (recon 2026-07-28): the regex secondary pass
  (`news.go:363-392`) is today the sole producer of player↔article links and `title_pos`. After
  cutover the Editor's resolver writes player links; `title_pos` stays NULL on new links, which
  the co-mention/heat proximity gates treat as lenient. Co-mention refresh dies at demolition;
  `compute_transfer_heat` survives on Editor links — its volume gets a T7-style rebaseline in
  Phase 9.
- **`vetted` becomes one fact: the Editor linked it.** Scrub-as-judge dies. The two-writer
  tri-state dies.
- **Exact match on `nrm()` surfaces is the only automatic link path** (T9). Trigram ranks for
  review. Ambiguity is refused, recorded, and nominated to the Investigator.
- **The Editor nominates; the Investigator verifies; search discovers; sources prove.** A model
  mention is never a database write. Every accepted fact cites a `source_documents` row.
  (Living-database doctrine, planning doc 2026-07-27.)
- **Maintenance is demand-led, like growth.** The story that makes a fact stale is the same story
  that re-arms its verification: a new mention of a decided candidate reopens it (5.6),
  re-verification supersedes changed relationships with dated validity, box scores revive
  `player_team_history` and tiers through the existing triggers, and the transfer chain keeps
  adjudicating movement. Nothing polls the world on a schedule to stay fresh; relevance drives
  refresh.
- **No prose reaches the Scout** (T4). The Scout's inputs stay: the stats platform (now fed by
  scraped box scores) + threshold-gated confirmations (`transfer_identity_applications` pattern).
- **Voices read packets; packets are compiled in code.** The Editor's model budget goes to
  reading articles (coverage), not to summarizing summaries.
- **`momentum_scores` (deterministic, ±100 scale) ≠ `momentum_summaries` (voice output, −5..+5).**
  Conflating them caused the 715-WARN storm. Decided facts are computed in code; models narrate.
- **ar7/C2 must never deploy onto the legacy rail** (HANDOFF §5). Its contract IS ep1's ancestor;
  it ships as the greenfield `editor` stage, not as an `article_read` bump.
- **Memory is characters-only** (Scott, 2026-08-01). The Editor is stateless — consistency comes
  from a fixed contract, frozen evals, and deterministic gates, never hidden memory. The
  Investigator writes facts and provenance, never memories. Each voice reads and banks its OWN
  memory, provenance-labeled (`Ground truth:` / `Prior story:` / `Our prior read:` — the
  echo-chamber rule from Progressive Refinement Dataflow). The Oracle is blind to evidence: five
  cards + its own verdict trail, nothing else. Memory blocks stay OUT of `input_hash` (the
  existing discipline, journalist/mod.rs:614 et al.); slice fingerprints (E2) are the only
  re-fan gate.
- **Stats land before the Scout reads** (Scott, 2026-08-01). The only roads to a `peak` enqueue
  are the percentile chain — `finalize_fixture()` inside the promotion tx → percentile recompute
  → `trg_percentile_changed_*` → `pg_notify('percentile_changed')` → Go listener (≥10 pts) —
  plus the nightly statcommentary reconcile cron. Never packets, never prose, never a
  pre-promotion fixture. The chain IS the enforcement (4.7).
- **Graph rides Gemma on archbox** (Scott, 2026-08-01). Typed-relation extraction is high
  effort, low thought — exactly the pinned card's work. The archivist shares
  `ARCHBOX_GEMMA_SLOTS` today and keeps it; only its enqueue seam moves at flip (7.13/8.5).
- **Empower the junction; Google does relevancy up front** (Scott, 2026-08-01). Every junction
  is budgeted to produce RICH output — the three-audience read: downstream prompt, user surface,
  archive. The ranked Google query is the relevancy filter at the front door; the rail never
  re-derives what the query already decided.
- **Plumbing gates phases; junction quality is a tuning knob** (Scott, 2026-08-03). The plan's
  point is the pipeline — simple, lean, nimble, durable — with models bolted into junctions to
  refine LATER. A phase closes when its plumbing is proven (flow, writes, purity, throughput
  mechanics); model-quality numbers (link rates, under-fill, registers, parse rates) are
  RECORDED and appended to **Appendix D (the tuning ledger)** as follow-ups, never a reason to
  halt scaffolding. Don't fully refine as you go. A junction quality measurement can still
  block at cutover-shaped decisions (Phase 8) — but not at build phases.

---

# The phases

## Phase 0 — Ground truth (read-only; no writes, no deploys)

Purpose: verify this plan's recon against the live system before any DDL is authored, and prove
the executor toolchain works from a fresh session.

- [x] **0.1** `set -a; source .env; set +a; psql "$DATABASE_URL" -c "select 1"` works. If the
      variable is named differently, fix §0.8 and note it in the Log.
      → **DELTA (§0.8 rewritten):** not the variable name — the *host*. No `psql` on the Mac; Mac
      `.env` has empty DB URLs. Works from archbox over ssh. See Log A.
- [x] **0.2** Confirm migration state: `select max(version) from schema_migrations` → expect
      `199_refresh_surfaces_analyze` band; `ls sql/migrations | tail -3` matches. Confirm
      `sql/schema/schema.sql` snapshot is current (no unapplied files).
- [x] **0.3** Confirm the queue substrate: `\d pipeline_work` shows `stage text` with **no** CHECK
      on stage; `entity_type` CHECK is `player|team|article|fixture`; PK
      `(stage, entity_type, entity_id, sport)`.
- [x] **0.4** Confirm `news_article_entities.entity_type` CHECK is `('player','team')` and
      `entity_name_surfaces` CHECK likewise; count rows in `entity_name_surfaces` (expect ~16,700).
- [x] **0.5** Confirm `news_articles.full_text` is NULL for all rows
      (`select count(*) from news_articles where full_text is not null` → 0).
- [x] **0.6** Confirm live triggers by name: `enqueue_derive_on_vetted`,
      `enqueue_transfers_if_transfer_related`, `enqueue_voices_on_routing_tags` on the news
      tables; `stage_routing_subscriptions` is empty.
- [x] **0.7** Confirm stage wire names in `rust/src/work.rs:42-55` = `scrub, article_read,
      fixture_boxscore, graph, peak, momentum, transfers, narratives, vibe, sigil`; and
      `ARCHBOX_GEMMA_SLOTS = ("archbox-gemma3", 4)` at `rust/src/stage.rs:84`.
- [x] **0.8** Confirm per-role base-url plumbing exists: `COGNITION_ROUTE_<X>_BASE_URL` in
      `rust/src/config.rs` (~:269) and `COGNITION_BACKEND_CONCURRENCY` (~:301).
- [x] **0.9** Confirm `fixture_boxscore_fetches` exists (mig 189) and
      `rust/src/boxscore_fetch.rs` fetches **provider JSON** (BallDontLie pagination const) — the
      landing table is reusable, the sources are dead (subscriptions cancelled 2026-07-27/28).
- [x] **0.10** Record baseline numbers in the Log: articles/day by sport (7-day avg), current
      `article_read` queue depth, `news_article_readings` rows, `narrative_persons` count,
      current disk free on the DB volume (bodies will add ~40–80 MB/day).
- [x] **0.11** Confirm the Mac's Ollama is reachable from archbox and note the exact installed
      voice-model tag (`curl http://<mac>:11434/api/tags`). Record hostname/IP in the Log. If the
      Mac is unreachable, note it — Phase 7 is the first phase that needs it.
      → Reachable. Mac = 192.168.1.77. **Scott closed D-1 on 2026-07-29: `ministral-3:14b` for all
      character work except Editor/Investigator; `gemma3:4b` pinned for those two; Nemo unused.**

**Verify:** every box above either matched or has a Log entry explaining the delta and a plan
edit. **Commit:** `rail: phase 0 — ground truth verified`.

✅ **VERIFY SATISFIED — Phase 0 is CLOSED (2026-07-29).** All 11 boxes ticked. 9 of 11 matched the
recon exactly with no edit needed; 2 produced deltas, each with a Log entry **and** the plan edit it
required: **0.1** → §0.8 rewritten (DB access is archbox-over-ssh, not the Mac), and **0.11** → §3
topology + Appendix B D-1 rewritten (Scott's model ruling). One additional finding not tied to a
box — the **archbox checkout is behind this repo** — is recorded as Log entry B and carried into the
STATE line as a Phase 1 precondition. **No STOP condition was hit:** every named file, function,
table, trigger and constant matched, and every number landed in its stated band. Phase 0 wrote
nothing to the database, built no binaries, and deployed nothing, as specified. Committed at
`91c55b5`; this closing amendment follows it.

### Log (phase 0)

Executed 2026-07-29 (session start 06:40 EDT), read-only. **No STOP condition hit** — every named
file, function, table, trigger and constant matched the recon; every number landed in its stated
band. Two deltas, both about *how the executor reaches the machines*, not about what the plan
claims: **A** (DB access host — §0.8 rewritten) and **B** (archbox checkout is behind — a Phase 1
precondition). Recorded below.

**A. DB access is via archbox, not the Mac (0.1 — §0.8 rewritten).** The Mac has no `psql` binary
(no libpq, no Postgres.app) and its `.env` ships `DATABASE_URL=` / `DATABASE_PRIVATE_URL=` **empty**;
`.env.local` does not exist on the Mac (only `.env.local.bak.20260726-111052`). The live DB is
**PostgreSQL 18.4**, database `scoracle`, datadir `/mnt/data/postgres/data`, on archbox
(`192.168.1.92`, user `sheneveld`, already in `~/.ssh/config`, key auth works under `BatchMode`).
Credentials live in `~/scoracle/scoracle-backend/.env.local` **on archbox** (17,564 B, 2026-07-27).
§0.8 now carries the working incantation. Every measurement below was taken through it.

**B. The archbox checkout is behind this repo — Phase 1 must sync it first.** archbox
`~/scoracle/scoracle-backend` is on `main` at **`cec766a`** ("Schema snapshot: 195/196/197"), while
this repo is at `90582f3`. On archbox, migrations **198 and 199 are untracked files** (applied to
the DB, never committed there) and `sql/schema/schema.sql` + `schema_migrations.txt` are
**modified-uncommitted**. The DB itself is fully correct (0.2 below); only the checkout drifted.
Phase 1 authors migrations 200+ in *this* repo but must apply them from archbox — **sync archbox to
this HEAD (or copy the migration file over) before running `sql/migrate.sh`**, or the runner will
also sweep up 198/199's stale working-tree state into the snapshot. Not a blocker; a first step.

**0.2 — migrations ✅.** `max(version) = 199_refresh_surfaces_analyze` (applied 2026-07-28
15:28:41 EDT), exactly the expected band. **201 files / 201 recorded, zero drift in both
directions** (`comm` both ways empty) — nothing unapplied. The **176 gap is confirmed real**
(174, 175, 177, 178 present; no 176) — leave it; number from 200 up. `sql/schema/schema.sql`
(537,374 B) was regenerated 15:29, one minute after 199 landed, and is committed clean
(`git status sql/` empty here). Snapshot is current.

**0.3 — queue substrate ✅ exact.** `pipeline_work`: `stage text NOT NULL` with **no CHECK on
stage** (so a new `editor` / `investigate_entity` stage needs no DDL — as the plan assumes);
`entity_type` CHECK = `player|team|article|fixture`; PK `(stage, entity_type, entity_id, sport)`.
Also present, unclaimed by the recon but worth knowing: `status` CHECK `pending|running|failed`,
partial claim index `(stage, available_at) WHERE status IN (pending, failed)`, and statement-level
`notify_pipeline_work_ready()` triggers on INSERT/UPDATE (LISTEN/NOTIFY wakeup, not polling).

**0.4 — entity CHECKs ✅.** `news_article_entities_entity_type_check` = `('player','team')`;
`entity_name_surfaces_entity_type_check` = `('team','player')` (same set). Bonus:
`entity_name_surfaces_surface_kind_check` = `('name','alias')`.
**`entity_name_surfaces` = 16,690 rows** — in band (recon said ~16,700).

**0.5 — bodies ✅.** `select count(*) from news_articles where full_text is not null` → **0**, out
of **159,601** total articles. Body retention is genuinely greenfield; nothing to migrate.

**0.6 — triggers ✅ all three, by exact name, all enabled (`tgenabled='O'`).**
`enqueue_derive_on_vetted` — AFTER UPDATE OF `vetted` ON `news_article_entities`, FOR EACH ROW,
WHEN `new.vetted IS TRUE AND old.vetted IS DISTINCT FROM new.vetted`.
`enqueue_transfers_if_transfer_related` — AFTER UPDATE OF `bucket` ON `news_articles`,
WHEN `new.bucket = 'transfer'`. `enqueue_voices_on_routing_tags` — AFTER UPDATE OF `routing_tags`
ON `news_articles`, WHEN `routing_tags IS DISTINCT FROM`. These are the three the T10 rule is
about. **`stage_routing_subscriptions` = 0 rows** ✅ (empty, as claimed).

**0.7 — stage wire names ✅ exact.** `rust/src/work.rs:44-53` is verbatim `scrub, article_read,
fixture_boxscore, graph, peak, momentum, transfers, narratives, vibe, sigil` (10 stages; the
`as_str` match spans 42–55 as the recon said). `rust/src/stage.rs:84`:
`pub const ARCHBOX_GEMMA_SLOTS: (&str, usize) = ("archbox-gemma3", 4);` ✅ — and its doc comment
already reads "shared by The Editor and graph", so the slot-group seat the Editor takes is real
and documented today.

**0.8 — per-role plumbing ✅, line refs dead on.** `COGNITION_ROUTE_<ROLE>_BASE_URL` resolved at
`config.rs:269-271` (via `normalize_base_url`, trailing slash trimmed so one host is one backend);
role key built at `:265`; `COGNITION_BACKEND_CONCURRENCY` parsed at `:301-304`, parser at `:316`;
`governor_for` at `route.rs:312-315`; per-call `num_ctx` at `ollama.rs:44` (applied `:153-154`,
omitted when `<= 0`). **`COGNITION_ROUTE_<ROLE>_CANDIDATE[_BASE_URL]` also exists** (`config.rs:281-295`),
defaulting a challenger to its role's own host — the §4 `COGNITION_ROUTE_EDITOR_CANDIDATE` hook is
already built. Nothing new is needed to put the Editor on archbox and the voices on the Mac.

**0.9 — box-score landing table ✅.** `fixture_boxscore_fetches` exists (mig
`189_fixture_boxscore_fetches.sql`), PK `fixture_id`, FKs to `fixtures`/`sports`, payload columns
`score / period_scoring / team_stats / player_stats / raw_labels` (jsonb) **plus
`model_version, prompt_version, parser_version, output_contract_version, parser_outcome`** — the
table was already shaped for a model-parsed path, which is exactly what Phase 4 needs; it is reusable
as claimed. Status CHECK already includes `not_supported|not_final|not_found|blocked|fetch_failed|
parse_failed|validation_failed`. **1 row** in it (the dead-provider era). `rust/src/boxscore_fetch.rs`
(57,785 B) fetches **provider JSON** from `https://api.balldontlie.io` with cursor pagination and
`const MAX_BDL_PAGES: usize = 20` (`:26`), `per_page=100`, key from `BALLDONTLIE_API_KEY` — dead
sources, live landing table. Confirmed.

**0.10 — baselines.**

| baseline | value |
|---|---|
| Articles ingested/day, steady state (07-25 → 07-28, 4 full days) | **5,584/day** (22,337 total) |
| — FOOTBALL | 3,467/day |
| — NFL | 1,365/day |
| — NBA | 763/day |
| `article_read` queue depth at 06:40 EDT | **7,426 pending**, 2 failed |
| `news_article_readings` rows | **20,970** |
| `narrative_persons` | **2,405** |
| `entity_name_surfaces` | 16,690 |
| `news_articles` total | 159,601 |
| DB size | **11 GB** |
| Disk free on DB volume (`/mnt/data`, 1.9 T) | **1.8 T free, 2% used** |

Notes that change how these read:
- **Ingest has no `sport` or `created_at` column.** `news_articles` is keyed by `fetched_at`, and
  sport arrives only via the `news_article_entities` join. Per-sport rates above are
  `count(distinct article)` through that join, so an article linked to two sports counts in both;
  the 5,584/day total is the un-joined truth.
- **2026-07-24 was a 32,160-article backfill spike** — a naive 7-day average reads 9,286/day and is
  wrong. The 4-day window above excludes it. (07-22: 871, 07-23: 1,360, 07-24: **32,160**,
  07-25: 5,117, 07-26: 2,818, 07-27: 6,046, 07-28: 8,356, 07-29 partial: 9,035.)
- **Body retention is a non-issue on disk.** At the plan's 40–80 MB/day, 1.8 TB free is ~60+ years.
  The DB volume is a separate 2 TB NVMe; note the **root volume `/` is at 86% (15 G free)** — the
  DB is not on it, but archbox's root is the tighter resource.
- **The 7,426 depth is a rest-window reading, not a backlog.** Sampled 38 min into the scheduled
  pause. `logs/queue-depth.csv` (10-min sampler, `scoracle-qsample.timer`, has a `harness_active`
  column) shows `article_read` pending per day as **min → max → end-of-day**: 07-27 `70 → 6,095 → 70`;
  07-28 `1 → 8,133 → 20`; 07-29 (through 06:40) `4,197 → 8,754 → 7,426`. **The legacy rail drains
  its day to ~zero every day.** This is direct evidence for the pre-execution audit's correction:
  ~7,400/day is *demand met*, not a ceiling — 3.9's capacity math stands.
- **Harness health: normal, mid-rest.** `scoracle-cognition.service` was `inactive dead` at check
  time with `Result=success, NRestarts=0`, `ExecMainStartTimestamp=04:00:47`,
  `ExecMainExitTimestamp=06:02:02`; `scoracle-cognition-pause.timer` fired 06:00, resume fires
  07:00. That is the documented 2h-on/1h-off duty cycle (§0.6), not an outage. Running units:
  `scoracle-api.service`; watchers `scoracle-api.path` + `scoracle-cognition.path` **active
  (waiting)** — the §0.6 warning about builds into `go/bin/` and `rust/bin/` tripping a restart is
  live and real. Postgres and Ollama are **system** units; the harness is a **user** unit.

**0.11 — the Mac, and the model assignment ✅.** Mac = **`192.168.1.77`** (this box; en0). Ollama
reachable **from archbox**: `curl http://192.168.1.77:11434/api/tags` run *on archbox* returns the
list, so Phase 7's cross-host routing has a clear path.

Measured inventory, both boxes:

| host | installed tags | loaded at check time | role |
|---|---|---|---|
| Mac `192.168.1.77` | **`ministral-3:14b`** (13.9B Q4_K_M, 9.08 GB) | **loaded** (`expires_at` 2026-07-30) | **the six voices** |
| | `mistral-nemo:12b` (12.2B Q4_0, 7.07 GB, native ctx 1,024,000) | not loaded | **unused** — installed, unrouted |
| | `mistral-32k:latest`, `mistral:latest` (both 7.2B) | not loaded | unused spares |
| archbox `192.168.1.92` | **`gemma3:4b`** (4.3B Q4_K_M, 3.34 GB) | **loaded, pinned** — 5.34 GB VRAM, `expires_at` **2318**-11-08 | **Editor + Investigator** |
| | `qwen3:8b`, `mistral:7b` | not loaded | unused spares |

**Scott's ruling, 2026-07-29 (closes D-1):** `ministral-3:14b` does **all character work except the
Editor and the "seeker"** (= the Investigator); `gemma3:4b` is **pinned** for those two; **Nemo is
present but not used**. One factual correction for the record: Nemo is installed on the **Mac**, not
on archbox — archbox has no Nemo at all, and its unused spares are `qwen3:8b` and `mistral:7b`. The
ruling's substance (Nemo unrouted) is confirmed either way; the box matters only so a future
executor doesn't hunt for it on the wrong machine.

**The pin is real config, not habit:** archbox `ollama.service` carries `OLLAMA_KEEP_ALIVE=-1`,
`OLLAMA_MAX_LOADED_MODELS=1`, `OLLAMA_NUM_PARALLEL=4`. See §3 for the three consequences — most
importantly that `MAX_LOADED_MODELS=1` turns "the Investigator shares the Editor's model" into a
hardware constraint, and that `NUM_PARALLEL=4` already equals `ARCHBOX_GEMMA_SLOTS`'s 4.

`num_ctx 4096` remains a deliberate budget, not a model limit (Ministral's native context is far
larger). Note `rust/src/route.rs:620` names `mistral-nemo:12b` on MAC **in a test fixture only** —
a test constant, not a route; Phase 7.12 sets the real route to `ministral-3:14b`, and 7.11
captures the voice fixtures against it. **No OLMo on either box**, consistent with the
pre-execution audit removing it.

### Handoff (phase 0 → 1)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phase 0 (ground truth) is committed, and the plan was REVISED 2026-08-01 (the character-flow
revision — read the revision note under STATE first; phases were renumbered). See the
Phase 0 Log for measured baselines.
PRECONDITION: sync the archbox checkout to this repo's HEAD before running sql/migrate.sh
(Phase 0 Log entry B — archbox is behind, with migs 198/199 untracked there).
Read §0 (protocol) and §1 (the packet contract), then execute Phase 1 (substrate migrations)
top to bottom. Migrations 200+; template sql/migration_template.sql; apply with sql/migrate.sh;
snapshot with scripts/hosting/snapshot-schema.sh; commit migration+snapshot together.
Note 1.4: persons.kind now includes 'player'. Everything in Phase 1 is inert — no code reads
the new tables yet. Do not deploy anything. Do not touch the legacy rail. STOP on surprise
per §0.3.
```

---

## Phase 1 — Substrate (migrations 200+; all inert; no deploys)

Everything here is DDL that nothing reads yet. Safety comes from inertness, not caution.
Follow §1's column specs exactly; conventions from neighboring migrations (timestamptz,
`DEFAULT now()`, snake_case, COMMENT ON for every table).

- [x] **1.1** Mig 200 `one_rail_storylines`: `storylines`, `storyline_articles`,
      `storyline_entities` per §1b. Indexes: `storyline_articles(article_id)`,
      `storyline_entities(entity_type, entity_id, sport, last_seen_at)`,
      `storylines(sport, status, last_seen_at)`.
- [x] **1.2** Mig 201 `one_rail_editor_reads`: `editor_reads` per §1a (PK `article_id`, status
      CHECK = legacy taxonomy + `not_sport`), index on `(status, updated_at)`, GIN on
      `resolved` (jsonb_path_ops).
- [x] **1.3** Mig 202 `one_rail_packets`: `packets` per §1c. Indexes:
      `packets(storyline_id, compiled_at DESC)`, `packets(day, sport)`, GIN `routing_tags`.
- [x] **1.4** Mig 203 `persons`: `persons` table — `id serial PK, sport text NULL, full_name text
      NOT NULL, kind text CHECK (player|coach|executive|owner|agent|family|official|other), team_id
      int NULL, search_aliases text[] DEFAULT '{}', meta jsonb DEFAULT '{}', created_at`. (`player`
      added by Scott 2026-08-01 — story-relevant players OUTSIDE the stats platform: rookies
      pre-debut, retired, foreign-league; the `players` table stays box-score-owned, see D-2.
      Kinds are a superset of `narrative_persons.kind`; that graph-layer table is unaffected and
      reconciles later — Appendix B.)
- [x] **1.5** Mig 204 `person_entity_type`: extend `entity_type` CHECKs to admit `'person'` on
      `news_article_entities` and `entity_name_surfaces`; add `'candidate'` + `'fixture'` to
      `cognition_ledger`'s CHECK; add `'candidate'` to `pipeline_work`'s. (Only what v1 writes —
      no speculative admissions.) Pattern per CHECK: DROP CONSTRAINT, ADD CONSTRAINT ... NOT VALID,
      VALIDATE CONSTRAINT (row counts here validate in ms, but the pattern is the habit).
      **Do not** touch other entity_type CHECKs — only these four tables are on the rail.
- [x] **1.6** Mig 205 `investigator_substrate`: the eight acquisition tables per the 2026-07-27
      planning doc, backend-named: `entity_candidates` (with `state` CHECK: `pending, accepted,
      rejected_not_sport, rejected_insufficient_evidence, rejected_out_of_scope, ambiguous,
      deferred_source_unavailable`; `idempotency_key text UNIQUE`; `norm_name`, `kind_hint`,
      `sport`, `target_entity_type/id`, `mention_count`, `first/last_seen_at`,
      `resolved_entity_type/id`, `decided_at`), `candidate_mentions` (candidate_id, article_id,
      quote, editor_descriptor, observed_at), `acquisition_runs` (candidate_id, status, query_plan
      jsonb, outcome, rejection_reason, model_version, parser_version, started/finished_at),
      `source_documents` (canonical url, final_url, domain, fetched_at, content_hash, http_status,
      title, retained_excerpt, headers jsonb), `entity_aliases` (entity triple, alias, norm_alias,
      source_document_id, state, **append-only** — enforce with a no-UPDATE trigger or a comment +
      review habit), `entity_external_ids` (entity triple, namespace, external_id,
      source_document_id), `entity_facts` (entity triple, fact_type, value_text, value_jsonb,
      valid_from/to, source_document_id, state CHECK active|superseded|conflicted),
      `entity_relationships` (subject triple, predicate, object triple, valid_from/to,
      source_document_id, state). A candidate, an attempt, a source, a fact, and a relationship
      are different things — combining them is how provenance disappears.
- [x] **1.7** Mig 206 `packet_routing`: trigger `enqueue_voices_on_packet` AFTER INSERT ON
      `packets` — for each tag in `routing_tags` joined to `stage_routing_subscriptions`, insert
      `pipeline_work` rows per active `storyline_entities` participant of matching grain, with
      `input_version = 'pk:' || <that voice's slice_fingerprint>` (E2: unchanged slices do not
      reopen). Plus an unconditional `narratives` fan-out per participant (the Journalist reads
      everything). Follow the mig-197 trigger's ON CONFLICT/input_version pattern +
      `pg_notify('pipeline_work_ready','')`. **Ships live but fires into an empty subscription
      table + zero packets — doubly inert.** Do NOT seed subscriptions here (that is Phase 7; the
      mig-197 churn-loop lesson).
- [x] **1.8** Extend `refresh_entity_name_surfaces()` to include `persons`
      (name + search_aliases, entity_type 'person') — mig 207. Run it; person surfaces = 0 rows
      today, which proves it's wired without changing behavior.
- [x] **1.9** `scripts/hosting/snapshot-schema.sh`; commit migrations + snapshot together.

**Verify:** `select count(*) from storylines` etc. all return 0; `\d+ packets` matches §1c;
mig 206's trigger exists and `stage_routing_subscriptions` still has 0 rows; legacy pipeline
unaffected (`article_read` queue still draining — compare depth to Phase 0 Log).
**Commit:** `rail: phase 1 — substrate migrations 200–207`.

✅ **VERIFY SATISFIED — Phase 1 is CLOSED (2026-08-01).** All 9 boxes ticked; migrations
200–207 applied on archbox in one `migrate.sh` run (all self-txn, no errors, no lock waits) and
snapshotted (209 versions in the ledger). Every count returned 0; `\d packets` matches §1c
column-for-column; both new triggers (`enqueue_voices_on_packet`, `entity_aliases_append_only`)
exist enabled; `stage_routing_subscriptions` = 0 rows; all four widened CHECKs are VALIDATED and
the `pair_entity_type` CHECK is untouched. Legacy rail unaffected: `article_read` depth went
3,847 → 3,822 *during* the apply (harness active, draining normally). **No STOP condition hit.**
Two protocol deltas, both recorded below and edited into the plan: §0.8's incantation (archbox
lost its `.env` to the env consolidation) and §1c's `slice_fingerprints` keys (pinned as stage
strings by mig 206).

### Log (phase 1)

Executed 2026-08-01, ~16:20–16:30 EDT, from a Mac session over ssh per §0.8. Harness was
**active** (mid-duty-cycle, not a rest window) throughout — acceptable because everything here
is inert DDL; mig 204, the only step locking live tables, carried `SET LOCAL lock_timeout='5s'`
and applied instantly.

**Precondition (Log B) — already resolved before this session.** archbox arrived at `b3af45b`
with a **clean** tree: the untracked 198/199 + modified-snapshot state Phase 0 found is gone.
The sync was one push + ff-pull: the single unpushed local commit (`7d1859c`, the plan revision
itself) went to origin, archbox fast-forwarded to it. Migration files were then scp'd over,
applied, and the checkout reconverged by pulling the phase commit at the end.

**Delta A — archbox has no `.env` anymore.** The env consolidation (`94a8a61`) left `.env.local`
as the only env file on archbox; §0.8's two-file incantation half-errors ("./.env: No such
file"). Harmless — `.env.local` carries the URLs — but §0.8 is now corrected to source
`.env.local` alone.

**Pre-apply measurements.** `max(version)` = `199_refresh_surfaces_analyze`;
`entity_name_surfaces` = **16,690** (identical to Phase 0); `article_read` depth **3,847**
pending+failed at 16:20 EDT. Prod's `refresh_entity_name_surfaces()` matched mig 199's body
**verbatim** (template rule: derive a rebuilt function from the current prod definition — mig
207 was authored from exactly this).

**Apply.** All 8 migrations applied `[self-txn]` in one run, zero errors. Mig 207's in-txn gate
printed: `refresh rebuilt 16690 surfaces, 0 person rows as expected` — the persons arm is wired
and behavior-neutral, proven in the same transaction (the gate RAISEs and rolls back the whole
migration on any other outcome).

**Post-apply verification (all bands met).** 14 new tables + `stage_routing_subscriptions` all
0 rows. `\d packets` matches §1c exactly. Triggers `enqueue_voices_on_packet` (packets) and
`entity_aliases_append_only` (entity_aliases) both enabled (`tgenabled='O'`). CHECKs:
`news_article_entities` = player|team|**person**; `entity_name_surfaces` = team|player|**person**;
`cognition_ledger` = player|team|article|**candidate**|**fixture**; `pipeline_work` =
player|team|article|fixture|**candidate** — all `convalidated=t`. `article_read` depth 3,822
(draining; Phase 0's intraday max band was ~8.7k, so today's afternoon depth is unremarkable).

**Contract decisions pinned by these migrations** (the substrate is now the authority; later
phases conform):

- **`slice_fingerprints` keys are STAGE strings** (`narratives`, `transfers`, `vibe`, …).
  Mig 206 reads `NEW.slice_fingerprints ->> s.stage`; §1c's old `{journalist: …}` example was
  character shorthand and §1c now says so. A **missing** fingerprint falls back to
  `'pk:' || packet id` — fail-open: the voice may re-read an unchanged slice, it never starves.
- **Mig 206's narratives arm fans only to player/team participants.** `pipeline_work`'s CHECK
  deliberately does not admit `person` (1.5 — only what v1 writes); a person-grain fan-out
  would violate it and abort the packet INSERT. Person participants still live in
  `storyline_entities`; they just don't wake voices.
- **Both fan-out arms `SELECT DISTINCT`** — two tags reaching the same (stage, entity) produce
  identical rows (input_version is stage-keyed, not tag-keyed), and without the collapse
  ON CONFLICT dies with "cannot affect row a second time". (Mig 197's per-tag fingerprints
  dodge this only because no two tags share a stage yet — noted here for the Phase 7 seeder.)
- **`entity_aliases` append-only is trigger-enforced** (the plan offered trigger *or* habit; the
  trigger won). UPDATE raises; supersession is a new row. DELETE is not blocked.
- **Provenance NOT NULLs:** `entity_facts.source_document_id` and
  `entity_relationships.source_document_id` are NOT NULL (every accepted fact cites a source —
  doctrine as DDL). Aliases/external-ids keep it nullable (their operational copies can arrive
  via reconciliation seeds). No CASCADE from `source_documents` — a delete that would orphan
  provenance fails instead.
- **Convention additions beyond the 1.6 letter,** all additive: `created_at` on the four
  provenance tables (write-time vs `valid_from` world-time — two different clocks), entity-triple
  indexes on aliases/external-ids/facts/relationships, FK-side index on
  `acquisition_runs.candidate_id`, and `sports(id)` FKs on every nullable `sport` column.
  Entity triples stay loose (no cross-table FK) by design — they span players/teams/persons.
- **NULL-sport persons carry no name surfaces** (mig 207 skips them): resolution is sport-scoped
  and `entity_name_surfaces.sport` is NOT NULL. A person becomes resolvable when its sport is
  known — until then unresolvable is the honest state.
- Mig 207's filename is `207_surfaces_include_persons` (1.8 named only the number).

**Housekeeping.** Snapshot regenerated on archbox (14,366 lines, 209 ledger versions), scp'd
back, committed here with the migrations; archbox reconverged to the phase commit via
`git pull --ff-only` and ends clean. The snapshot diff was audited: beyond the new objects and
the four CHECKs, the only churn is pg_dump 18's per-dump `\restrict` token and alphabetical
reflow (e.g. `event_box_scores` moved, unchanged).

### Handoff (phase 1 → 2)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–1 committed: substrate tables (storylines/packets/editor_reads/persons/acquisition)
exist and are inert; entity_type CHECKs admit 'person'; packet routing trigger is live but
fires into an empty subscription table.
Read §0 and §3, then execute Phase 2 (lungs — Go provenance, additive only).
One [DEPLOY] step at the end; deploys restart services via the .path watchers — that is
expected. Do not touch match.go or any regex path; deletions happen in Phase 9.
```

---

## Phase 2 — Lungs (Go; additive only; one small deploy)

The lungs mostly exist (teams-only Google News RSS sweep, 02:00 cron, `feed_rank`, the 0.95
primary link). This phase records what has been implicit and changes no behavior.

- [x] **2.1** In `persistArticles` (`go/internal/thirdparty/news.go:319-329`), write query
      provenance into `news_articles.raw` on **insert only** (first-writer wins; on conflict leave
      existing): `{"q": <the literal query term used>, "lane": "primary|alias<N>", "edition":
      <ceid>, "window": "24h", "query_team_id": <id>}`. Thread the term/lane from
      `buildRSSSearchQueries` (`news.go:857`) through the fetch result to persist. The query IS
      the hypothesis — now it is also readable.
- [x] **2.2** Add a funnel counter for articles that arrive with a body-bearing description vs
      empty (no behavior change; feeds Phase 3's fetch expectations).
- [x] **2.3** Confirm (and note in Log) that the sweep is teams-only by design — "gather the broad
      topics of the sport" — and that persons/players are **never** swept; they enter via Editor
      discovery. This is doctrine, recorded here so nobody "helpfully" adds a player sweep.
- [x] **2.4** `go test ./...`; build to a scratch path first; then **[DEPLOY]** `go build -o
      go/bin/pipeline ./cmd/pipeline` (watcher restart expected).
- [x] **2.5** After the next 02:00 ingest (or a manual bounded run), verify:
      `select raw->>'q', count(*) from news_articles where fetched_at > now() - interval '1 day'
      and raw ? 'q' group by 1 order by 2 desc limit 10` returns sane query terms.

**Verify:** provenance present on ≥95% of new arrivals; article volume unchanged vs Phase 0
baseline (±20%); zero new Go errors in logs.
**Commit:** `rail: phase 2 — lungs record the hypothesis`.

### Log (phase 2)

Executed 2026-08-01, ~16:25–16:40 EDT, from a Mac session. **The Mac has no Go toolchain** —
same split as §0.8's psql rule: edits authored locally, vet/test/build run on archbox
(go1.26.4) over ssh, working copies scp'd over first, checkout reconverged via `git pull
--ff-only` at the end. No STOP condition hit; every named file/line matched the plan.

**2.1 — provenance.** Threaded as four *unexported* fields on `Article`
(`queryTerm/queryLane/queryEdition/queryWindow`) so the API JSON response is unchanged;
stamped in `fetchFromRSS`'s query loop (the only place the lane index exists);
written by `persistArticles` as an 8th INSERT column. First-writer wins twice, by
construction: `deduplicateArticles` keeps the first occurrence, so a cross-lane collision
inside one sweep carries the earliest lane (primary runs before aliases); and the
ON CONFLICT branch never touches `raw`, so re-seen articles keep the provenance of the
sweep that found them first. `query_team_id` is written only when `isTeamEntity` —
belt-and-braces for any future non-team caller. **One literal deviation from the step's
example:** `window` records the actual `when:` token sent to Google, and
`rssWhenToken(24)` renders the 24-hour window as **`"1d"`, not `"24h"`** — the recorded
value is the question as asked, which is the point of the field.

**2.2 — funnel split.** `Funnel` gains `DescriptionBearing`/`DescriptionEmpty` (log keys
`desc_bearing`/`desc_empty`), counted where `Matched` is set, so the pair partitions
Matched exactly. Not a drop stage — `Residual()` deliberately unchanged. `Add` extended;
the positional-literal compile-guard test grew to 17 fields; the every-drop test now also
asserts the partition.

**2.3 — doctrine confirmed.** The only sweep path in Go is `cmd/pipeline/main.go:93` →
`corpus.Sweep` → `LoadTeams` (reads the `teams` table, nothing else) →
`GetEntityNews("team", …)`. No player/person sweep exists anywhere; `work.go`'s
`"player"` string is queue metadata, not a sweep. Persons enter via Editor discovery only.

**2.4 — tests + deploy.** `go vet` + `go test ./...` green on archbox; scratch build to
`/tmp/rail-phase2/pipeline` first; **[DEPLOY]** `go/bin/pipeline` rebuilt 16:31 EDT.
`scoracle-api.path` watcher fired as documented (api restart 16:31:52); cognition
untouched (16:04 start retained). The 02:00 cron (`cron-pipeline.sh -mode ingest`) picks
up the new binary tonight.

**2.5 — verified via manual bounded run** (NBA, `-rss-limit 5`, 16:32 EDT, 29s): 30/30
teams ok, `rss_errors=0`, matched 150, `desc_bearing=150 / desc_empty=0`, residual 0,
95 fresh articles. SQL: the step's top-q query returns sane per-team terms
(`"Detroit Pistons" NBA basketball` × 4, …); **provenance on 95/95 = 100% of new
arrivals** (band ≥95% met); sample `raw` carries the full contract
(`q/lane/edition/window/query_team_id`, `lane=primary`, `edition=US:en`, `window=1d`).

**Verify band, remaining third:** article volume ±20% vs the Phase 0 baseline
(5,584/day un-joined) cannot be read from a bounded run — **next session, check the
first post-deploy 02:00 sweep** (`logs/pipeline-ingest.log` + count on `fetched_at`)
before starting Phase 3 work. Provenance ≥95% and zero-Go-errors bands are met above.

### Handoff (phase 2 → 3)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–2 committed: substrate inert, lungs now persist query provenance in news_articles.raw.
Read §0, §1a (the ep1 contract — you are implementing it exactly), §3, §4, then execute
Phase 3 (the Editor's junction, greenfield) top to bottom.
Phase 3.0 renames the LEGACY module junctions/editor/ -> junctions/article_reader/ (files only
— stage strings, env keys, prompt versions untouched, per §4 naming ruling) to free the
character's name for the new module. Key law: describe then derive (T2). Exact nrm() surface
match is the only automatic link path (T9). The new Editor writes editor_reads +
news_articles.full_text ONLY — it must not write news_article_entities, bucket, routing_tags,
or anything the legacy rail reads (shadow purity).
Fixtures: the legacy eval task renames editor -> article_reader WITH its fixtures dir in the
same commit (the dir MUST match the task name — a mismatch was once invisible for two days);
the new task takes the name `editor` with a fresh rust/fixtures/editor/.
```

---

## Phase 3 — Heart I: the Editor (greenfield junction, shadow mode)

New Rust stage `editor` in the character-named module `rust/src/junctions/editor/` — which the
legacy seat currently occupies, so the phase opens by renaming the legacy module to match its
own runtime identity (`article_read` stage, `Role::ArticleReader`). The new Editor fetches,
persists the body, reads on the ep1 contract, resolves in code, and writes **only** greenfield
tables. The legacy `article_read` keeps running untouched. Both share the 4-slot gemma group;
the new Editor registers FIRST so new arrivals outrank the legacy backlog (registration order
is claim order, `rust/src/main.rs:131-173`).

- [x] **3.0** **The rename that frees the name (files only — §4 naming ruling).**
      `git mv rust/src/junctions/editor rust/src/junctions/article_reader` + update `use` paths;
      rename the legacy eval task string `editor` → `article_reader` in `eval_tasks.rs` AND
      `git mv rust/fixtures/editor rust/fixtures/article_reader` **in the same commit** (task
      name and fixtures dir must move together — `fixtures_dir()` resolves `fixtures/<task>`,
      and a mismatch once erred silently for two days). UNTOUCHED, verify by grep after:
      stage string `article_read`, `ARTICLE_READ_*` consts and env keys,
      `COGNITION_ROUTE_ARTICLE_READER`, prompt version `ar7`, table `news_article_readings`.
      Gate: `cargo test` at baseline AND `cargo run --bin eval -- --task article_reader
      --fixtures` passes all 51 checks (proves the renamed gate can still fire).
      Commit separately: `rail: phase 3.0 — legacy editor module renamed article_reader`.
- [x] **3.1** Extract the fetcher into `rust/src/fetch.rs`: move `fetch_article` +
      Google-News URL resolution + headless-Chrome fallback + `clean_html` out of
      `junctions/article_reader/mod.rs` (formerly editor/mod.rs:816-880) into a shared module;
      the legacy module calls the extracted functions (mechanical refactor, behavior identical —
      run the existing test suite).
- [x] **3.2** `Stage::Editor` (`"editor"`) in `work.rs` + `as_str` + claim ORDER BY
      `news_articles.feed_rank ASC NULLS LAST` (copy the ArticleRead arm at `work.rs:65-73`) +
      add to `KNOWN` in `main.rs:188`.
- [x] **3.3** `Role::Editor` in `route.rs` (+ `Role::all()` array length bump + `env_suffix`
      `EDITOR`). Route config on archbox: `COGNITION_ROUTE_EDITOR=gemma3:4b` (§4: settled by
      hardware — no bakeoff scheduled).
- [x] **3.4** `EditorHandler` in `rust/src/junctions/editor/`: `slot_group()` =
      `ARCHBOX_GEMMA_SLOTS`, `max_in_flight()` = 4, `rotation_batch()` = 8. Handle: if
      `duplicate_of` is already set (the mig-196 exact-title sweep), short-circuit — status
      `duplicate`, no fetch, no model call, no attach; the canonical carries the story (~5–12%
      of arrivals, free coverage). Otherwise fetch (via
      fetch.rs) → persist `news_articles.full_text` + fetch metadata → build ep1 prompt (system
      prompt: port the ar7 text minus the co_mentions instructions, plus `names[]` descriptors,
      `result_line`; **property order per §1a**) → gemma call (temperature 0.2, num_predict 900,
      num_ctx 8192, format_schema) → parse → **derive in code** (`editor/derive.rs`): relevance
      (port `derive_relevance` minus the co_mentions arm), resolver (exact `nrm()` lookup against
      `entity_name_surfaces`, ambiguity refused + recorded), nominations, routing tags,
      result_line parse → persist `editor_reads` (one tx) → ledger rows (`data_fetch_ledger` for
      the fetch AND `cognition_ledger` for the model call — closing the legacy seat's ledger gap;
      entity_type 'article').
- [x] **3.5** Enqueue seam: in Go `persistArticles`, alongside the existing scrub enqueue
      (`news.go:412-421`), enqueue `stage='editor'` for every new article (same tx, same
      ON CONFLICT discipline). The Editor drains only where `COGNITION_STAGES` includes `editor`
      (archbox).
- [x] **3.6** Eval task `editor` (fresh) in `eval_tasks.rs`, fixtures dir `rust/fixtures/editor/`
      (empty until this step fills it): port the 7 legacy fixtures from
      `fixtures/article_reader/` to ep1 expectations, then add: a coach-discovery case
      (kyle-shanahan shape), a place-collision case (Paris/Moulin Rouge — expect `descriptor`
      prevents the club link), a hallucinated-parent case (Fortuna Düsseldorf — expect
      exact-match refusal), a result-line case (verbatim score), an opponent-only case with the
      KEPT-since-ar6 expectation (`relevant=true` — the stale fixture trap), and a namesake tie
      (Vinicius — expect `refused_ambiguous`). Target ≥12 fixtures. The evaluator must run the
      **production** parser + derive path, as the legacy task's evaluate does
      (`eval_tasks.rs:1677`).
- [x] **3.7** Tests + clippy at baseline; build target/debug; run
      `cd rust && cargo run --bin eval -- --task editor --fixtures` → 100% **on the required
      axes** (re-scoped by Scott, 2026-08-01 — see Log): every `relevant`, `resolver_never_links`,
      `resolver_refused`, `resolver_links`, `result_line_*`, `story_type`, `name_absent`,
      `key_fact_*`, and `blurb_*` check must pass (33/33); `name_found`/`name_kind`/
      `descriptor_nonempty`/`register` and their non-emission cascades (`resolver_unresolved` on
      a name the model did not emit) are WAIVED here and judged by resolved-link rate on live
      traffic in 3.9(d) (the ar7 lesson: judge the discovery channel by the links it produces).
- [x] **3.8** **[DEPLOY]** rust binary to archbox with `COGNITION_STAGES` += `editor`, the new
      Editor registered before article_read. **[DEPLOY]** the Go enqueue change. (Two
      watch-triggered restarts; do them together, outside a rest boundary.)
- [x] **3.9** *(RE-SCOPED by Scott 2026-08-03 — see §4 tuning ruling and the Log: plumbing
      readings taken day-1; quality readings recorded and moved to Appendix D, not gates;
      the 48h wait cancelled in favor of the corpus replay, whose verdict lands in D-T1.)*
      Shadow measurement, 48h minimum, recorded in the Log:
      (a) throughput and coverage: editor_reads/day vs arrivals/day. Capacity context (Scott,
      2026-07-29): the legacy ~7,400/day was DEMAND-limited, not a card ceiling — the harness
      runs 2h on / 1h off (HANDOFF §10; a 16h duty day) and Gemma sits mostly idle inside it,
      churning its queue fast. Arrivals (~8,140/day) should clear comfortably on the existing
      schedule, with idle headroom left over for the Investigator. Measure anyway — assumed
      headroom is how rails rot: record reads/day, p50/p95 queue wait, and GPU-busy fraction
      across two duty days. If coverage still lands <95% within 24h: (1) confirm the 3.4
      short-circuit is firing, (2) trim num_predict 900→750 (ep1 dropped co_mentions' output
      share), (3) STOP and surface — never silently accept the miss (§2 clause 1);
      (b) coverage: % of arrivals read within 24h (goal ≥95%);
      (c) status distribution (fetch_failed/paywall/blocked bands vs legacy's);
      (d) discovery: on a 50-article sample of players with known bleed (Olise-class), linked-in-
      `resolved` rate vs the legacy 39/182 Vinicius baseline;
      (e) resolver refusals/day (goes to the Investigator's future queue);
      (f) legacy rail health unchanged (narratives/day vs Phase 0 baseline — shadow purity).
- [x] **3.10** `full_text` growth check vs disk headroom from Phase 0 Log. *(Closed 2026-08-03:
      ~23 MB/day toast-measured vs 1.8T free — ~8.4 GB/yr, a non-issue; Log 17:04 entry.)*

**Verify:** 3.9 bands hold; fixture gate 100%; zero greenfield-Editor writes to any legacy-read
table (`news_article_entities`, `news_articles.bucket/routing_tags` untouched by the new stage —
assert by column-diff on a sampled day).
**Commit:** `rail: phase 3 — the Editor reads in shadow`.

### Log (phase 3)

Executed 2026-08-01, ~16:37–18:00 EDT, from a Mac session. **STOPPED at the 3.7 fixture gate**
(§0 rule 3): the ep1 set holds at **44/53 property checks (83%)**, stable across repeat runs,
against the step's 100%. Steps 3.0–3.6 are complete and committed (449edec, 1b00776, f1d2881,
904447b, e5896fa); **nothing is deployed** — 3.8 was already gated on the Phase 2 volume-band
check (first post-deploy 02:00 sweep had not fired when this session ran), and now also on the
gate. Legacy rail untouched throughout.

**Build pattern** (Mac has no Go toolchain; classifier blocks writes into the live archbox
checkout): Rust authored + unit-tested on the Mac; model-backed evals and Go vet/test run on
archbox in a scratch copy `~/rail-phase3/` (own `CARGO_TARGET_DIR=~/rail-phase3/target`,
env sourced from the live checkout's `.env.local`) — the live checkout stays clean at ec302be
until it can `git pull --ff-only`.

**3.0 — rename.** Files only; identities grep-verified untouched (stage `article_read`,
`ARTICLE_READ_*`, `COGNITION_ROUTE_ARTICLE_READER`, ar7, `news_article_readings`). The
fixture-gen example moved too (its emitted `task` field was stale "reader" — fixed to
`article_reader`). Gate: 284 lib tests green; `eval --task article_reader --fixtures` on
archbox = 44/44 properties + 7/7 parses — **the plan's "51 checks" = 44 + 7**. First run
flapped one check at the fixtures' frozen temp 0.2 (43/44), clean on re-run — that flakiness
is why the new ep1 set freezes 0.0 instead.

**3.1 — fetch.rs.** fetch_article + GN resolution + Chrome fallback (env key untouched) +
clean_html + body utilities (count_words, looks_paywalled, content_hash, domain_of,
normalize_space, ARTICLE_MIN_WORDS) moved to crate::fetch; legacy re-imports; fetch-only tests
moved with the code; 284 tests green at each step.

**3.2–3.5 — inert plumbing.** Stage::Editor (claims by feed_rank like the legacy arm),
Role::Editor (`COGNITION_ROUTE_EDITOR`, Role::all() → 11), EditorHandler (slot group
ARCHBOX_GEMMA_SLOTS, max_in_flight 4, rotation 8, registered before article_read; mig-196
duplicate short-circuit; blocked vs fetch_failed split on HTTP 401/403; T1 cache key =
contract_version + content_hash; writes editor_reads + news_articles.full_text ONLY, resolver
runs only on relevant reads, no downstream enqueues), derive.rs (relevance port, exact-nrm
resolver with kind gate + refused ties, routing_tags, parse_result_line, 5.2
nominates_immediately), Go StageEditor enqueued once per fresh INSERT via `(xmax = 0)`.
26 new editor unit tests; go vet + go test green on archbox.

**Two findings the plan should carry forward:**
1. **`news_articles.full_text` is legacy-visible by design**: the Journalist's
   `article_context` falls back to full_text (truncated to blurb length) for articles whose
   legacy read is not `success` — so shadow full_text writes will enrich some legacy
   narratives prompts. The plan names full_text as an allowed write; noted so 3.9(f)'s
   narratives-volume check is read with this in mind (volume should hold; content enriches).
2. **No stage has ever physically shipped its documented schema field order.**
   `serde_json::Value` is BTreeMap-backed: every `format_schema` reaches Ollama with
   properties ALPHABETIZED, and a live probe against archbox gemma confirmed Ollama's grammar
   forces emission in exactly the received property order. ar4–ar7's documented orders were
   therefore never the wire orders (ar7 emits `register` BEFORE `register_phrase` — label
   before phrase, the C2 anti-pattern — and its measured-good numbers were earned in that
   accidental order). `GenerateOptions.format_schema_raw` (new) POSTs a verbatim schema
   string; the ep1 editor pins the true §1a order through it; every legacy stage keeps its
   measured alphabetical bytes byte-for-byte.

**3.6/3.7 — the gate, measured.** Task `editor` + 12 fixtures (legacy 7 ported to ep1 + the
five named cases), frozen at temp 0.0, evaluator runs the production parser + derive path
(group_hits scored against fixture-declared surfaces). Seven measured iterations:

| config | score |
|---|---|
| initial prompt, alphabetical wire (accidental) | 40/53, 41/53 |
| §1a order via format_schema_raw | 34/53 |
| + worked example, roles/city rules | 36/53 |
| + quoted-people rule, fixture fixes (hiring, banner pin) | 44/53 |
| A/B same prompt on alphabetical order | 45/53 |
| final config (§1a order, temp 0.0), run twice | **44/53, 44/53** |

Order A/B is a wash (44 vs 45, inside noise); the §1a order is KEPT — it is the written
contract and now physically real. Temp 0.0 stabilizes the score but not every check detail
(Ollama batched decode). **Stable failure classes, all four model-behavior-shaped:**
(a) secondary-person under-fill — gemma emits the headline person + clubs but drops quoted
managers/scorers (Moyes, Arteta, Bellingham, Rangers-as-club): ~4-5 checks;
(b) register `outrage` → neutral under phrase-first emission (label-first — the accidental
legacy order — passes it; the C2 phrase-before-label doctrine measurably costs this on 4B);
(c) name-collision roles: the Ravens youth page gets `passing_mention`/`subject`, not
`absent`; (d) Paris-the-city labeled `subject`/kind `club` — descriptor does not prevent the
link when the model itself calls the city a club. (a) improved from 1-2 names to 2-7 via the
worked example; (b)–(d) never passed under the §1a order.

**Options for Scott** (next session executes the ruling): (1) keep iterating ep1 prompt text
against the frozen set; (2) relax the 3.7 bar to a named subset (e.g. 100% on relevance +
resolver axes, waive the under-fill/register checks) and let 3.9's live shadow measure the
rest; (3) revisit the §1a emission order now that order is controllable (label-first register
measured better); (4) judge names[] coverage by resolved links on live traffic (the ar7
lesson: "judge this field by the links it produces") rather than by fixture name lists.

**Still open before 3.8 (both gates):** the Phase 2 volume band (±20% vs 5,584/day) against
the first post-deploy 02:00 sweep (`logs/pipeline-ingest.log` + `fetched_at` count), AND a
green (or re-scoped) 3.7 gate. `COGNITION_STAGES` on archbox does not yet include `editor`;
`COGNITION_ROUTE_EDITOR` is unset there (eval runs exported it ad hoc).

---

**Resumed 2026-08-01, ~20:45–22:00 EDT (same Mac-session pattern). Scott ruled; the gate is
GREEN re-scoped; 3.8 waits only on tonight's sweep.**

**Scott's rulings (2026-08-01 evening), three of the four options, jointly:** (1) keep
iterating the ep1 prompt; (2) re-scope the 3.7 bar to the axes code depends on; (4) judge
`names[]` coverage by resolved links on live traffic (3.9(d)). Option (3) — label-first
register — NOT taken: the §1a phrase-first order stands; the register misses stay waived, a
known cost re-measured at cutover. Scott also ruled on the stale Phase 2 band — see the
volume-band note at the end of this entry.

**The re-scoped bar, precisely** (now also in the 3.7 step text): REQUIRED = every `relevant`,
`resolver_never_links`, `resolver_refused`, `resolver_links`, `result_line_*`, `story_type`,
`name_absent`, `key_fact_*`, `blurb_*` check — 33 of the 53. WAIVED = `name_found`,
`name_kind`, `descriptor_nonempty`, `register`, and `resolver_unresolved` where the name was
never emitted (a cascade of under-fill, not a resolver defect) — 20 of the 53. `name_absent`
is deliberately REQUIRED (over-emission/invention is not under-fill).

**Iterations 8–13 (numbering continues the first session's seven):**

| iter | change | score | required axes |
|---|---|---|---|
| 8 | listing_or_schedule broadened; city rules; Vikings+Monaco `absent` examples | 39/53 | youth ✓, paris ✗✗, fortuna ✗ (new), under-fill worse |
| 9 | place rule scoped; Santos identical-string example; passing_mention prose; quoted-people re-scan | 41/53 | paris ✗✗ (kind still `club`), fortuna flapped ✓ |
| 10 | **descriptor place gate in derive.rs (code)** | 42/53 | paris ✓✓ FIRST TIME; fortuna flapped ✗ |
| 11 | **supported-vote rule in derive.rs (code)**; prompt trimmed | 42/53 | all required ✓ except blurb_absent[confirma] (fixture bug) |
| 12 | fixture token confirma→confirmó; more prose trimmed (Shanahan recovered) | 47/53 ×2 | all ✓ except name_absent[Gwladys] flap |
| 13 | FIELD 2 exclusions name stands/ends/streets | **48/53 ×2, identical reds** | **33/33 ×2** |

**The finding that settled it (probe, 2026-08-01):** gemma3:4b on the Paris page emits kind
`club`, role `subject` — and descriptor **"capital city"**. The model writes the truth in the
descriptor while guessing the labels. Seven first-session iterations plus three more here
could not move the labels; §1a always said "the descriptor is what lets code refuse
Paris-the-city → Paris-the-club" — that arm was designed but never implemented. It is now CODE
(T2: the description is the model's; the judgment is ours):
* `derive::descriptor_names_place` — place words (city/capital/town/village/municipality)
  with a club-sense veto list ("city rivals" is club language and never flags). Unit-tested
  both directions.
* group_hits: a place-described mention never takes a `team` link, whatever the kind_hint
  claims (Paris flows to `unresolved`, recorded, not dropped). Person links untouched.
* derive_relevance: a role vote whose own `names[]` entry describes a place is retracted —
  the descriptor (copied from text) outranks the role label (a guess).
* derive_relevance: a `passing_mention` vote with no matching `names[]` entry is a label with
  no referent and does not count (the Fortuna flap: hypothesis "Fortuna" string-associated
  onto "Fortuna Mining Corp"; the model's own names list holds no "Fortuna"). Subject/opponent
  votes still stand alone — an unlisted principal is names under-fill, not absence.

Five new editor unit tests (313 lib tests green); clippy at baseline (both remaining warnings
pre-date this session, in untouched files). Eval display now prints descriptors
(`Paris<club "capital city">`) — that one-line change is what exposed the finding.

**Fixture fix:** non-english `blurb_excludes` token "confirma" → "confirmó" — the bare stem is
a substring of the English "confirmation", which a correct English blurb legitimately uses
(measured false red, iter 11). Spanish-leakage detection keeps the accented forms.

**Prompt (ep1 text, contract unchanged):** net additions that survived measurement:
listing_or_schedule covers single-broadcast pages; Santos + Vikings worked `absent` examples;
quoted-people re-scan (recovered Moyes/Clement/Barrett-Baxendale in most runs); FIELD 2
exclusions name stands/ends/streets (killed the Gwladys venue over-emission). Additions that
measurably HURT extraction were reverted (the iter-8 place paragraph and iter-9 subject/
passing_mention prose each suppressed people — Shanahan vanished for four runs until the trim
brought him back). Lesson repeated from ar4–ar7: on a 4B, every added rule taxes extraction
somewhere else; put judgments in code and keep the prompt about describing.

**Waived-class cost, stated honestly:** register `outrage` still reads neutral under
phrase-first order (known C2 cost, Scott declined the reorder); surname-only mentions
(Moyes/Arteta/Rangers/Bellingham) flap in and out of `names[]` run to run. Both are exactly
what 3.9(d)'s live measurement is for; if linked-rate on the Olise-class sample lands under
the legacy 39/182 baseline, the under-fill class comes back on the table.

**Volume-band ruling (Phase 2 carry-over):** Scott re-baselined the ±20% band to the
immediate pre-deploy steady state **8,356–9,035/day** (band midpoint ~8.8k → accept
6,956–10,556). The written 5,584/day averaged 07-25→07-28 across two low days (2,818, 5,117)
and was already exceeded by +58% BEFORE the Phase 2 deploy (07-28: 8,356 → 08-01: 8,877,
DB-counted on `fetched_at`); the band's intent — the deploy changed nothing — is what tonight's
02:00 sweep must confirm. One oddity to re-check there: the 07-31 and 08-01 sweeps both logged
exactly `fresh_articles=8823` with different matched/dedup counts; a third identical value
would mean a cap is binding somewhere, not coincidence.

---

**2026-08-02, 02:28–04:20 EDT: both gates closed; 3.8 DEPLOYED and smoke-verified.**

**Volume band (Phase 2 verify, third clause — CLOSED).** First post-deploy 02:00 sweep
(2026-08-02, completed 02:28:12, 28m12s): `ok=204 fail=0`, **`fresh_articles=7259`** — inside
the re-baselined band 6,956–10,556. DB cross-check: 7,176 rows on `fetched_at` in the sweep
hour (delta vs 7,259 = ON CONFLICT collapses; normal). Zero `level=ERROR` lines in the ingest
log, ever. The `8823` twice-coincidence did NOT repeat — coincidence, not a cap. Phase 2's
verify is now green on all three clauses.

**3.8 deploy (04:05–04:10 EDT, inside the 04:00–06:00 clean window).** Live checkout
`git pull --ff-only` → 4871fbe (which adds `editor` to the unit template's documented set —
the unit's `Environment=` line is overridden by `.env.local` per its own NOTE, both kept in
sync). `.env.local`: `editor` inserted into `COGNITION_STAGES` (before `article_read`,
cosmetically — registration order is hardcoded in main.rs), `COGNITION_ROUTE_EDITOR=gemma3:4b`
added (backup at `/tmp/env.local.bak-38`). Then one `scripts/hosting/release.sh` — Go + Rust
built at one commit, watchers masked across placement, units re-rendered, API + cognition
restarted, health probe green. Worker log confirms: `stages=["scrub","graph","editor",
"article_read",...]` — **the Editor claims before the legacy seat**; route
`editor=gemma3:4b@localhost`.

**Smoke test (the Phase-2 pattern: bounded manual run, NBA `-rss-limit 5`, 04:10, 28s).**
15 fresh inserts (sweep logged 17 pre-conflict) → 15 `editor` work items enqueued by the new
Go binary → queue drained in ~7 min → **15/15 editor_reads: 13 success, 1 duplicate (the
mig-196 short-circuit fired: no fetch, no model call), 1 blocked (the 401/403 split), 0
failed**. First row inspected: ep1 envelope well-formed; resolver linked "Nolan Traore" →
player and "Brooklyn Nets" → team via exact surfaces. Ledger rows present both sides
(`data_fetch_ledger`, `cognition_ledger` stage=editor — the duplicate/blocked correctly skip
the model). **Shadow purity spot-check: 0 of the new articles had `bucket` set; `full_text`
present on fetched ones — exactly the allowed writes.** Legacy rail: article_read backlog
7.4k pending at 04:15 (normal overnight depth), draining, zero journal errors.

**3.9 clock:** arrivals come only from the 02:00 cron (plus manual runs), so the first full
organic day starts with the **2026-08-03 02:00 sweep**; the 48h minimum window is Aug 3
02:00 → Aug 5 02:00 EDT, readings due in the Aug 4 and Aug 5 sessions (reads/day, p50/p95
queue wait, GPU-busy fraction, ≥95% coverage in 24h, status bands, the Olise-class discovery
sample vs 39/182, refusals/day, narratives/day vs baseline). 3.10 (full_text growth vs disk)
rides the same readings.

---

**2026-08-02, 08:17 EDT: interim health check (+4h post-deploy) — all green; window-open
baselines recorded. 3.9 NOT taken (window opens Aug 3 02:00; nothing to measure yet).**

A resume session fired before the window opened; instead of readings, it verified the shadow
deploy is holding and froze the baselines the Aug 4/Aug 5 sessions will diff against:

* **Editor state:** `editor_reads` still exactly the smoke test — 13 success / 1 duplicate /
  1 blocked, 0 failed. `pipeline_work` has zero `editor` rows (completed work is removed) —
  queue fully drained. As expected: the 02:00 Aug 2 sweep pre-dated the Go enqueue deploy
  (04:10), so no organic arrivals reach the Editor until the Aug 3 sweep.
* **Services:** `scoracle-cognition`, `scoracle-api`, `scoracle-qsample.timer` all active;
  `journalctl -p err` since 04:05 is EMPTY. Legacy `article_read` backlog 5,651 pending at
  08:17 (down from 7.4k at 04:15), draining normally.
* **Instrumentation for 3.9:** queue-depth.csv sampling with `harness_active` column confirmed
  live (last sample 08:10). Queue-table caveat for the wait math: the table is named
  **`pipeline_work`** (not work_queue) and completed rows are DELETED — p50/p95 queue wait must
  come from `cognition_ledger` timings joined to `news_articles.fetched_at`, not from the queue
  table.
* **3.10 window-open baseline:** `news_articles` total relation 188 MB; `full_text` = 13 rows /
  71 kB (smoke only — effectively zero). `/mnt/data`: 26G used, **1.8T free (2%)**, matching
  Phase 0.
* **3.9(f) baseline, table name pinned:** legacy narratives = **`narrative_episodes`** rows/day:
  07-30: 649, 07-31: 609, 08-01: 646 (pre-window band ~609–649/day); 08-02 partial at 08:17
  already 355 — on pace, deploy visibly changed nothing.

---

**2026-08-02, 21:45–21:55 EDT: second pre-window health check (+17.7h post-deploy) — all
green; two instrumentation pins corrected for the Aug 4/5 sessions. 3.9 still NOT taken
(window opens Aug 3 02:00, ~4h from this check).**

* **Rest-window false alarm, resolved:** `scoracle-cognition` showed `inactive` at 21:47 —
  this is the §0.6 harness schedule, not a crash: `scoracle-cognition-pause.timer` stopped it
  cleanly at 21:00 (SIGINT, drain at item boundary, stopped 21:02:02);
  `scoracle-cognition-resume.timer` fires 22:00. Worker log at its 19:00 start confirms the
  deployed order still holds: `stages=["scrub","graph","editor","article_read",...]`,
  commit 4871fbe. `journalctl --user -p err` since deploy (04:05): still ZERO lines. Only
  WARNs: two google-news URL-resolution timeouts (legacy article_read fetches via the shared
  fetch.rs — expected noise).
* **Editor state:** `editor_reads` still exactly the smoke test (13 success / 1 duplicate /
  1 blocked, 0 failed); zero `editor` rows in `pipeline_work`. Correct — no arrivals since
  the enqueue deploy; first organic items come with the Aug 3 02:00 sweep, whose crontab
  entry is confirmed present (`0 2 * * * cron-pipeline.sh -mode ingest`).
* **Legacy rail:** article_read backlog 1,043 pending at 21:47 (5,651 at 08:17; 7.4k at
  04:15) — draining normally. `narrative_episodes` 08-02 partial: 573 at 21:47 vs band
  609–649/day — plausible pace with the 22:00–24:00 duty block remaining; final Aug 2 count
  gets read in the window sessions.
* **PIN for the wait math (path correction):** the sampler CSV is
  **`~/scoracle/scoracle-backend/logs/queue-depth.csv`** on archbox (the handoff block said
  `queue-depth.csv` bare — there is no such file at repo root). Live and sampling: header
  `ts,harness_active,stage,status,count`, last sample 21:40:47 with `harness_active=0`
  (correctly 0 inside the rest window), 14,532 lines.
* **PIN for 3.10 (metric definition):** the 71 kB window-open baseline is
  `sum(pg_column_size(full_text))` (post-TOAST/compressed); the same 13 rows measure 128 kB
  by `octet_length` (raw). Both re-measured tonight — 13 rows, 71 kB toast / 128 kB raw, i.e.
  zero growth since 08:17, as expected. The Aug 4/5 growth check must diff **pg_column_size**
  against 71 kB (or state its own metric); disk unchanged: /mnt/data 26G used, 1.8T free (2%).

**2026-08-03, 17:04–17:30 EDT: DAY-1 INTERIM READINGS (+15.1h into the window). The Editor
is green on every axis it owns; the legacy seat is being starved of the card. 3.9 NOT
closed (window runs to Aug 5 02:00; these are the interim numbers the Aug 4/5 sessions
diff against).**

**(a) throughput, day-1 partial.** Aug 3 sweep: **6,905 arrivals** (02:01–02:26). Enqueue
seam: 100% — editor_reads 5,328 + pending 1,573 + running 4 = 6,905, zero leaks. Reads by
17:05: **5,328 (77.2% at +15.1h)**, drain ~355/h against the 02:00 bulk dump; remaining
~1,577 projects done ~21:00–23:00 — **≥95%-in-24h (b) is on pace**, formal read at the Aug 4
session. Queue wait (cognition_ledger.generated_at − news_articles.fetched_at, n=3,995):
**p50 7.77h / p95 14.42h** — shaped by the once-daily 02:00 dump, not by slot scarcity for
the Editor itself. Model calls: 4,009 (duplicate/blocked/fetch_failed correctly skip the
call), call wall **p50 33.4s / p95 56.2s / avg 34.1s** (~349 output tokens on the sampled
call, ~10 tok/s/slot on the 1070 Ti at 4-parallel).

**(c) status distribution, day-1 vs legacy 7-day (Jul 27–Aug 2, news_article_readings):**
editor success 2,999 (56.3%) vs legacy 53.5%; irrelevant 858 (16.1%) vs 23.8% (partial-day +
ep1-derive difference — re-read on the full window); **duplicate 496 (9.3%) — the 3.4
short-circuit is firing, inside the predicted 5–12%**; blocked 593 (11.1%) + fetch_failed
137 (2.6%) = 13.7% vs legacy's fetch_failed 12.3% (legacy has no blocked status — the
401/403 split is new; combined bands match); empty_body 107 (2.0%) vs 2.2%; paywall 1 vs ~0.
**Watch item: parse_failed 137 (2.6%) vs legacy 0.1%** — one work-item failed outright.
Re-read on the full window before judging.

**(e) refusals, day-1 partial:** 88 refused_ambiguous names across 88 reads (~1.7% of reads).
Resolved shape confirmed: `resolved.links[] / unresolved[] / refused_ambiguous[]`.

**(f) THE FLAG — legacy starvation under strict claim priority.** `news_article_readings`
written today: **ZERO**. article_read pending 6,976 = **6,925 of today's arrivals + 51
stale stragglers** (oldest fetched_at 2026-05-11 — pre-existing junk, not new). graph: 0
ledger rows today (its enqueue seam is the legacy handler — starved downstream). Cause is
arithmetic, not a bug: editor wall today = **38.05 slot-hours of the ~40 available**
(15.1h × ⅔ duty × 4 slots) — registration order is strict priority, so article_read claims
only when the editor queue is empty, which it never was. Steady-state projection: editor
demand ≈ 5,200 calls/day × 34.1s ≈ **49 of the 64 daily slot-hours**, leaving ~15 for
article_read + graph, which previously consumed most of the idle card to make 7,400
reads/day. Unless legacy's per-call wall is ≲10s, combined demand exceeds the card:
the legacy backlog compounds daily and narratives/day decays. This is precisely the
"assumed headroom is how rails rot" scenario 3.9 exists to catch — and it is NOT covered
by 3.9's remedy list (the short-circuit IS firing; num_predict trim shaves ~15% off editor
wall, not enough to hand the card back). Not a purity violation: bucket untouched on all
of today's editor-read articles (0 set); editor writes remain greenfield-only.
**Decision: no mid-window knob turns (§0.4 — one change, one measurement). The Aug 4
session reads (f) FIRST: Aug 3 final narratives, overnight legacy catch-up, article_read
backlog at the 02:00 sweep. If the backlog did not clear, 3.9(f) is breaching → log,
commit, BLOCKED per §0.3. Options to surface to Scott (not execute): registration-order
swap, slot split (2+2), editor num_predict 900→750, duty-schedule change.**

**(f) narratives:** Aug 2 FINAL = **573 vs band 609–649** (−6% below band low; the
22:00–24:00 duty block added nothing after 573 @ 21:47). Aug 2 pre-dates today's
contention (editor was idle post-smoke), so this is NOT starvation — plausibly a Sunday
dip (the band was built from Thu/Fri/Sat). Aug 3 partial: 317 @ 17:04. Judge on full days.

**(3.10 interim):** full_text **3,875 rows / 17.4 MB** pg_column_size-toast (baseline 71 kB)
≈ ~23 MB/day steady-state ≈ ~8.4 GB/yr. news_articles total relation 188→211 MB. /mnt/data
27G used, **1.8T free (2%)**. No risk; formal close with the window.

**Services:** cognition/api/qsample all active; `journalctl --user -p err` since deploy
(Aug 2 04:05): still ZERO lines. qsample live, last 17:00:47 `harness_active=1`.

**PINs for the Aug 4/5 sessions:** (1) GPU-busy fraction = sum(`context_budget->>'wall_ms'`)
from cognition_ledger over duty-hours × 4 slots — **filter stage: only editor (+graph) run
on the archbox card; narratives/rating/sigil/vibe/momentum in the same ledger run on the
Mac's ministral.** article_read writes NO ledger rows (the legacy gap persists until flip),
so card-busy is a lower bound and legacy throughput must be counted from
`news_article_readings.updated_at` (no created_at column). (2) editor_reads timestamps:
`fetched_at`/`updated_at` (no created_at). (3) cognition_ledger timestamp: `generated_at`.

---

**2026-08-03, 17:45–20:15 EDT: SCOTT'S RULING — stop waiting for live accumulation; judge
the Editor on the existing corpus, threshold-gate the move to production, then set a new
debugging window post-flip. The 48h wall-clock window is no longer the gate; the CORPUS
REPLAY is.**

Scott (in session, after the day-1 GPU/starvation briefing): two passes are enough; we have
an entire corpus to test with; train/test on existing data rather than waiting for new
arrivals; once a high-enough threshold is crossed, move to production and set a new timer
for debugging there. He also ruled: do NOT try to fit both rails on the card (no 24h duty
day — no extra hardware stress), and production delay during the window is acceptable.

**3.9(d) taken early, on day-1 organic data — with the definitional correction that
re-frames the baseline.** The HANDOFF 39/182 was measured on an unrecorded mention
definition; re-measured with ONE definition (title-mention, per-name) on both rails:
legacy 7-day (Jul 27–Aug 2, news_article_entities): Vinicius 157/286 = **54.9%**, Olise
27/47 = **57.4%**, Diomande 208/421 = **49.4%**. Editor day-1 organic: Vinicius 15/25 =
**60.0%** (BEATS legacy), Diomande 10/26 = 38.5% raw — but the gap is design, not bleed:
of the 26, 6 were `duplicate` (short-circuit; canonical carries the story), 4 `irrelevant`,
1 `blocked`; of 15 actual reads Yan linked in 10 = **66.7% per-success (beats legacy)**.
Olise: **zero title-mentions on Aug 3** — the sample-starvation problem in person, and
what the corpus replay fixes. The 5 Diomande misses are ALL the waived names[]-under-fill
class: the model emitted clubs/agents but not the player (one title literally "Yan
Diomande's dream Real Madrid transfer…" → names[] had Jay-Z, two agents, RB Leipzig, no
Yan). Refusals in the bleed sets: 0. Sport-scope note for anything replayed by hand:
`resolve_names` filters `entity_name_surfaces` on `pipeline_work.sport` (derive.rs:287) —
wrong sport = resolution impossible by construction.

**CORPUS REPLAY ENQUEUED (the same-article A/B).** 137 legacy-era articles (Jul 27–Aug 2,
duplicate_of IS NULL, title-mention: all 37 Olise + 50 md5-sampled Vinicius + 50 Diomande)
inserted into pipeline_work stage='editor', sport='FOOTBALL' (rehearsed in a rolled-back tx
first, ON CONFLICT DO NOTHING; insert 137, queue total 1,184 with ~1,047 organic remaining;
~20 min of extra card time). Legacy's per-article verdicts on these exact articles are
already in news_article_entities → when the queue drains (~23:00 tonight), the measurement
is a per-article PAIRED comparison, per name: editor-linked × legacy-linked 2×2. Replay
rows self-identify: any editor_reads row on an article with fetched_at < Aug 3 is replay
(organic coverage starts Aug 3 02:00). Caveat to carry: week-old Google News URLs may
fetch worse than fresh ones — judge link rate per successful read alongside the raw rate.

**Threshold (for Scott to affirm):** the plan's written bar is "beats legacy" per bleed
name on the same yardstick. Vinicius already clears it on organic day 1. The replay
verdict + Scott's threshold call together decide the move-to-production step and the
post-flip debugging window he asked for.

---

**2026-08-03, 20:30–21:00 EDT: PHASE 3 CLOSED under the §4 tuning ruling (Scott,
2026-08-03: plumbing gates phases; junction quality goes to the Appendix D tuning ledger
as follow-up — "we don't need to fully refine as we go").**

✅ **VERIFY SATISFIED.** (1) Plumbing bands hold: enqueue seam 100% (6,905/6,905, zero
leaks), coverage 82.5% at +15.6h and climbing toward the 24h mark, duplicate short-circuit
9.3% (in band 5–12%), ledger rows both sides, zero journal errors since deploy. (2) Fixture
gate 100% on required axes (33/33 ×2, 3.7). (3) **Zero greenfield writes to legacy-read
tables, formally column-diffed on the sampled day (Aug 3): 5,857 editor-read articles,
bucket set 0, routing_tags set 0** — and 0 of them were touched by legacy, so the zeros
are unambiguous. Code path confirms `news_articles.full_text` is the only legacy-table
column written (mod.rs:670, write-if-different).

Quality numbers measured en route (Vinicius 60.0% vs legacy 54.9% same-yardstick; Diomande
66.7% per-success vs 49.4% but 38.5% raw; Olise no day-1 sample; under-fill = the whole
miss class; parse_failed 2.6% vs legacy 0.1%) are RECORDED in Appendix D, not gates. The
137-article corpus replay drains ~23:00 tonight; its paired verdict lands in D-T1 as the
tuning baseline. The accepted operational state carries into Phase 4: the Editor owns the
card, legacy article_read starved (~11 reads Aug 3), narratives decaying — Scott accepts
this until cutover-shaped decisions; no 24h duty day (hardware stress ruled out).

**Commit:** `rail: phase 3 — the Editor reads in shadow`.

---

### Handoff (phase 3, 3.8 deployed → 3.9 readings)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phase 3 through 3.8 COMPLETE and DEPLOYED: the Editor reads every arrival in shadow on
archbox (release.sh @ 4871fbe, 2026-08-02 04:10 EDT; editor registered before
article_read; smoke test 15/15 reads, 0 failed, shadow purity spot-checked). The 3.7
gate is green re-scoped (33/33 required ×2); the Phase 2 volume band is closed (7,259
fresh in band 6,956–10,556, zero errors).
RE-SCOPED BY SCOTT 2026-08-03 (read the 2026-08-03 20:15 Log entry FIRST): the corpus
replay replaces the 48h wait as the 3.9(d) gate. 137 legacy-era bleed-class articles
(all 37 Olise + 50 Vinicius + 50 Diomande, Jul 27–Aug 2, title-mention, duplicate_of
excluded) were enqueued to the Editor ~20:10 Aug 3; drain ETA ~23:00 Aug 3.
DO NOW: (1) measure the paired per-article A/B — replay rows self-identify as
editor_reads on articles with fetched_at < Aug 3 02:00 (minus the 15 smoke-test rows,
article ids in the 2026-08-02 Log); per name, 2×2 editor-linked × legacy-linked
(news_article_entities, entity ids: Vinicius Jr 600687, Olise 24799984, Yan Diomande
37922937; resolver is sport-scoped — replay items carry FOOTBALL); report raw rate AND
per-successful-read rate (stale Google News URLs fetch worse). Yardstick already taken:
legacy title-mention rates Vinicius 54.9%, Olise 57.4%, Diomande 49.4%; editor organic
day-1 Vinicius 60.0%. (2) Take the routine 3.9 a/b/c/e/f readings for the record
(cognition_ledger ⋈ fetched_at for waits; narratives will show the accepted starvation
dent). (3) 3.10: diff sum(pg_column_size(full_text)) vs 71 kB baseline (17.4 MB @ Aug 3
17:00). (4) Verify: zero greenfield writes to legacy-read tables (bucket/routing_tags
column-diff on a sampled day). (5) BRING SCOTT THE THRESHOLD VERDICT — bar is "beats
legacy per name on the same yardstick"; he then rules on move-to-production and the
post-flip debugging window. Do not deploy or change claim order without his ruling.
Known+accepted meanwhile: legacy article_read starved (~11 reads Aug 3), narratives
decaying; no 24h duty day (Scott: no extra hardware stress).
```

### Handoff (phase 3 → 4)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–3 committed: the greenfield Editor (stage `editor`, module junctions/editor/) reads
every arrival in shadow on the ep1 contract, persists bodies to news_articles.full_text and
reads to editor_reads, and touches nothing the legacy rail consumes (legacy module now lives
at junctions/article_reader/, eval task article_reader). See Phase 3 Log for measured
throughput/coverage.
Read §0, §3, §4 (rulings — including the 2026-08-03 tuning ruling: plumbing gates phases,
quality goes to Appendix D, never halt scaffolding on model-quality numbers), Appendix B
D-4 (CLOSED: FOOTBALL), then execute Phase 4 (the Investigator — box scores; the module
junctions/investigator/, Role::Investigator, and the budgeted fetcher are all founded here
and reused by Phase 5). Housekeeping first, 5 minutes, not a gate: the 137-article corpus
replay drained overnight — append its paired per-name verdict to Appendix D-T1, then move on.
Laws: one adapter per source family; DOM/JSON parsing, never regex, never LLM number-reading;
fixture identity validated before anything writes; promotion calls finalize_fixture in the
SAME tx (stats before the Scout, §4); a correction is a revision + recompute, never an
overwrite. The landing table fixture_boxscore_fetches and stage fixture_boxscore already
exist (mig 189 / rust/src/boxscore_fetch.rs) — you are replacing their SOURCES (providers
cancelled) and moving the file, not changing their shape. The stage wire name
fixture_boxscore is a live queue identity — it does NOT rename (§4 naming ruling).
```

---

## Phase 4 — Heart II: the Investigator arrives (box scores; the stats rail reborn)

Third-party stats are gone (cancelled 2026-07-27/28), and every day without this phase the
Scout reads staler z-scores — that is why box scores are the FIRST Investigator job (Scott,
2026-08-01). The news rail now *causes* the stats rail: the Editor's `result_line` nominates
a completed fixture; the Investigator scrapes, validates, and promotes it; the existing
percentile chain fires the Scout exactly as it did off the provider seeder.
`player_team_history` comes back to life for free.

This phase also founds the Investigator in code — the module, the role, and the budgeted
fetcher land here; Phase 5 (entity discovery) reuses all three.

- [x] **4.1** The character's module + role: create `rust/src/junctions/investigator/`;
      `git mv rust/src/boxscore_fetch.rs rust/src/junctions/investigator/boxscore.rs` (+ `use`
      path updates; the stage wire name `fixture_boxscore` and the landing table are live
      identities and do NOT rename, §4). `Role::Investigator` in `route.rs`
      (`COGNITION_ROUTE_INVESTIGATOR=gemma3:4b` on archbox; `Role::all()` bump; `env_suffix`
      `INVESTIGATOR`). The Investigator rides the pinned Gemma (§3 — hardware constraint); its
      only v1 model calls are describe-only page triage.
- [x] **4.2** The budgeted fetcher (shared substrate — built here, reused by Phase 5): extend
      `fetch.rs` with a per-domain budget (concurrency 1, ≥2s spacing per domain, respect
      429/Retry-After, circuit-break a domain after repeated failures), cache by canonical URL
      + content_hash into `source_documents` with a bounded `retained_excerpt`. No browser
      automation; a domain that blocks direct fetch is a domain we skip (never stealth).
- [~] **4.3** *(table DONE — mig 208 applied, seeded EMPTY; seed BLOCKED on Scott's source
      ruling — see Log)* `boxscore_sources` table (mig 208, data-not-code): sport, league_id NULL, domain,
      discovery mode (url_template | search), parser_family, trust_state
      (candidate|trusted|suspended), fetch_policy jsonb (rpm, concurrency, cache_ttl). **D-4
      sport is CLOSED: FOOTBALL** (Scott, 2026-08-01). In-phase: terms/robots review of
      candidate source families → seed ONE family, url_template mode (the surgical target-URL
      scrape); record the review in the Log. NBA is the fallback only if every FOOTBALL family
      fails review.
- [ ] **4.4** Fixture nomination (code, Editor handle): parseable `result_line` + both teams
      resolved → match `fixtures` within ±2d on (sport, home/away or reversed) → if found and
      status ≠ completed, or scores differ, or no row: upsert a fixture row
      (status='completed', scores from the parse, `external_id` NULL — Scoracle identity, not
      provider identity) flagged `meta needs_verification`. The existing
      `fixture_boxscore_enqueue_on_final` trigger enqueues `fixture_boxscore` off that upsert —
      verify it fires; do NOT add a second enqueue. Rehearse the upsert rolled-back against a
      real day first; fixture identity errors here are the highest-severity failure of the
      phase.
- [ ] **4.5** Extend `investigator/boxscore.rs`: `SourcePlan` from `boxscore_sources` (replacing
      the provider map path for FOOTBALL), retrieval through the 4.2 fetcher into
      `source_documents` + the existing `fixture_boxscore_fetches` landing row, one DOM/JSON
      parser module per source family. A model may TRIAGE an unfamiliar page layout
      (describe-only) — it never reads numbers into rows.
- [ ] **4.6** Validation gate (code): fixture identity (teams, date, competition), final
      status, participant completeness vs known rosters (warn-level, not fatal — rosters drift),
      per-stat key mapping into `stat_definitions.key_name` for the sport (unmapped keys land in
      `raw_labels`, never guessed), arithmetic checks (totals vs sums where the sport defines
      them), source revision (content_hash change on refetch → revision, not overwrite).
- [ ] **4.7** Promotion (the old seeder's job, now gated): validated landing row →
      `event_box_scores` + `event_team_stats` + fixtures scores/status **+
      `finalize_fixture(fixture_id, recompute)` — all in ONE tx**, parser_version stamped. That
      call aggregates player_stats/team_stats and recomputes percentiles, which is the ONLY
      road to the Scout: `trg_percentile_changed_*` → `pg_notify('percentile_changed')` → Go
      listener (≥10 pts) → `peak`. **Stats-before-the-Scout is enforced by this ordering (§4
      law).** Downstream fires by itself (verify, don't re-trigger): the percentile listener,
      `trg_detect_team_change`, momentum-refresh marks.
- [ ] **4.8** **The replay gate — the phase's proof.** Pick 20 provider-era completed FOOTBALL
      fixtures with stored `event_box_scores`. Run the public-source path end-to-end into a
      rolled-back tx; diff shared stat keys vs provider rows. Gate: 20/20 fixture identity,
      ≥95% shared-key agreement (document every disagreement — some will be provider errors;
      that is the finding, not a failure).
- [ ] **4.9** Fixtures for the parser family (3 canned pages incl. one malformed), tests,
      **[DEPLOY]** rust to archbox (`fixture_boxscore` already in `COGNITION_STAGES` from the
      provider era — verify).
- [ ] **4.10** Run live for 7 days on FOOTBALL, in the Log: fixtures nominated/verified per
      day, validation failure taxonomy, promotion count, `detect_team_change` firings,
      PEAK/momentum enqueues caused, and one Scout (`peak`) read post-promotion sampled for
      sanity — plus the law check: assert no `peak` work row for a promoted fixture predates
      its promotion tx (the collision achieved, in the right order).

**Verify:** replay gate passed; live week promoted >0 fixtures with 0 identity errors; Scout
read sane and strictly post-promotion.
**Commit:** `rail: phase 4 — box scores from public sources; the stats rail reborn`.

### Log (phase 4)

**2026-08-03, 18:19–19:00 EDT: session opened Phase 4; 4.1–4.2 built; 4.3 review run to
completion; BLOCKED at the 4.3 seed on a Scott decision (§0.3 stop — no improvisation).**

*Housekeeping first (the D-T1 replay verdict):* NOT measurable — this session fired at
18:19 EDT, before the drain. Queue state coherent: `editor` = 1,184 pending + 1 failed
(exactly the enqueue-time total), untouched since the 18:00 rest-window pause (worker
SIGINT 18:00:47, clean stop; resumes 19:00). The Phase 3 log's "~20:10 enqueue" stamp was
UTC mislabeled EDT (~16:10 EDT actual). Drain ETA ~23:00 EDT holds at day-1 throughput.
Measurement recipe pinned in D-T1 for the next session.

**4.1 DONE (code, not yet deployed).** `git mv rust/src/boxscore_fetch.rs
rust/src/junctions/investigator/boxscore.rs`; module founded with doctrine header;
`Role::Investigator` added to route.rs (`as_str` "investigator", `env_suffix`
"INVESTIGATOR", `Role::all()` 11→12). Stage wire name `fixture_boxscore` and landing
table untouched (§4). Workspace builds; route (12) + boxscore (7) tests green.
`COGNITION_ROUTE_INVESTIGATOR=gemma3:4b` env line deliberately deferred to the 4.9
[DEPLOY] (un-configured, the role resolves to the default model — inert until then).

**4.2 DONE (code).** `fetch.rs::BudgetedFetcher`: per-domain concurrency 1 (per-domain
async mutex — structural, not advisory), min spacing floored at 2s (`FetchPolicy::new`
clamps), 429/Retry-After honored as a domain hold (delay-seconds exact, date-form → flat
60s, capped 300s), circuit-break at 4 consecutive failures → 15-min hold, provenance into
`source_documents` (new row per fetch per mig-205 doctrine; title, bounded 100k-char
retained_excerpt, header subset), cache probe by URL within `cache_ttl` reuses the newest
2xx row without spending budget. 7 unit tests on the pure mechanics (spacing, circuit,
Retry-After cap, header parse, title/excerpt bounds) — 13/13 fetch tests green. No browser
automation anywhere on the path.

**4.3 TABLE DONE, SEED BLOCKED. Migration 208 (`boxscore_sources`) applied on archbox +
snapshot committed (210 versions). Seeded EMPTY** — because the terms/robots review
(run live from archbox, the production fetch host, with the declared
`ScoracleBot/1.0 (+https://scoracle.com)` UA) failed every candidate family:

| # | family | sport | verdict | evidence (2026-08-03, from archbox) |
|---|---|---|---|---|
| 1 | fbref.com | FOOTBALL | **FAIL: blocked** | Cloudflare interactive challenge on `/robots.txt` itself (HTTP 403, `cType: 'interactive'`) to the declared bot |
| 2 | ESPN (www + site.api) | both | **FAIL: robots + blocked** | `Disallow: */boxscore?` for `User-agent: *`; API host 403s the bot on robots.txt |
| 3 | understat.com | FOOTBALL | **FAIL: robots** | `User-agent: * Disallow: /` |
| 4 | sofascore.com | FOOTBALL | **FAIL: robots + ToS** | year-slugged match paths disallowed for `*`; ToS bans automated extraction; API signature-gated |
| 5 | fotmob.com | FOOTBALL | **FAIL: robots** | `Disallow: /api/*` for `*` (the only data path; only Googlebot/Bing/etc. allowed) |
| 6 | flashscore.com | FOOTBALL | **FAIL: technical** | robots permissive but data only via signed JS feeds (x-fsign) → banned automation (Appendix C) |
| 7 | soccerway.com | FOOTBALL | **FAIL: technical** | serves the bot HTTP 200, but it is Livesport (Soccer24) — match data rides the same signed JS feeds as #6 |
| 8 | worldfootball.net | FOOTBALL | **FAIL: blocked + license** | content pages 403 the bot; robots carries EU-790 Art.4 content-signals reserving AI use |
| 9 | transfermarkt.com | FOOTBALL | **FAIL: license** | robots ALLOWS match pages + serves the bot 200 (server-rendered HTML, report ids extractable) — but its declared RSL license (`license.xml`) is `<prohibits type="usage">ai-all</prohibits>`; 4.5's Gemma triage + an AI product on top is squarely what it reserves. Override = Scott's legal-posture call, not the executor's |
| 10 | football-data.org | FOOTBALL | terms PASS, **data insufficient (free) / provider (paid)** | free tier: 12 comps, no lineups/scorers/bookings — cannot feed player percentiles; €29/mo tier = re-entering a paid provider |
| 11 | api-football | FOOTBALL | terms PASS, **needs Scott** | built-for-API product; free tier 100 req/day INCLUDES fixtures/events/lineups/player stats — data-sufficient; requires him to register (free) |
| 12 | api.openligadb.de | FOOTBALL | open, **data insufficient** | fully open (200, keyless) but German comps only, no player-level rows |
| 13 | cdn.nba.com liveData | NBA | **FAIL: blocked** | declared bot: silent Akamai tarpit (25s stall); plain curl UA: 403 Access Denied — passing requires browser impersonation = stealth |
| 14 | stats.nba.com | NBA | **FAIL: blocked** | tarpits the declared bot (timeout); known to require ~10 spoofed browser headers = stealth |
| 15 | basketball-reference.com | NBA | **FAIL: terms** | robots ALLOWS `/boxscores/` (crawl-delay 3) and the published bot policy tolerates <20 req/min "regardless of bot type" (2025 Finals G1 page fetched clean, both stat tables present) — but SR ToU clause 5 explicitly bans building tools/databases on scraped data AND any use where the data feeds AI-generated "answers, text, scores, statistics" |

Also dismissed without probes: Wikipedia/Wikidata (no systematic per-matchday player
stats — coverage, not access), StatsBomb open data (selected historic comps only),
openfootball github (scores only, lag), balldontlie free tier (stats endpoints paid),
official league APIs — premierleague.com/pulselive, UEFA (Origin-header gating = spoofing).

**The finding: D-4's fallback clause fired and the fallback ALSO fails.** "NBA is the
fallback only if every FOOTBALL family fails review" — every FOOTBALL family failed, and
every keyless NBA family fails the same way. The public-web box-score commons is closed
to declared, robots-respecting bots in 2026; what remains open is API products with free
tiers. **Demand sizing for that path (measured):** FOOTBALL completed fixtures avg
9.8/day, max 35/day (400→60d window) → api-football free tier (100 req/day) fits at ~2
calls/fixture with ~30% headroom on the worst matchday; a paid tier exists if volume
grows.

**Options for Scott (the BLOCKED decision):**
(a) **api-football free key — recommended.** ToS-clean (an API product), data-sufficient
(lineups + events + player stats), free, sized to demand. ~2 min registration; token
lands in archbox `.env.local` as e.g. `APIFOOTBALL_KEY`. Re-enters "a provider" only in
the loosest sense (no contract, no cost; cancellable by deleting a row + env line).
(b) football-data.org €29/mo — ToS-clean, EU-based, but paid provider re-entry.
(c) Rule that a facts-only, rate-respecting fetcher may proceed against a terms-failed
family (#9 transfermarkt or #15 basketball-reference are the technically-clean picks) —
his legal-posture call; the executor default is the review verdict, i.e. no.
(d) Neither → Phase 4 parks after 4.4 (nomination is source-independent) and the stats
rail stays down — every day of which the Scout reads staler z-scores (the phase's own
opening argument against waiting).

**Session hygiene:** mig 208 applied + snapshot pulled; nothing deployed (no [DEPLOY]
steps ran; `target/debug` builds only); no writes to any live table beyond the empty
boxscore_sources DDL; the D-T1 pin edited in place. 4.4–4.10 untouched per §0.3.

---

**2026-08-03, ~19:10 EDT onward: Scott re-directed the session TWICE; the block is
resolved and the phase re-scoped. RULINGS (do not re-litigate):**

1. **"Try the actual league pages"** → the review continued onto official league sites
   and FOUND A PASSING FAMILY: **premierleague.com / footballapi.pulselive.com** —
   robots permissive (query-param disallows only; the API host serves no robots.txt),
   the API serves the declared ScoracleBot UA openly (fixtures list, per-match
   `teamLists` + 28 events + `/stats/match` verified on 2025/26 GW38), and the T&Cs
   scan found NO automated-access, scraping, or AI clause (IP boilerplate + database
   rights; CJEU BHB v William Hill noted in terms_review). UEFA's match API is equally
   open but Scoracle's fixtures span ONLY the top-5 domestic leagues (measured: Serie
   A/PL/La Liga 380/yr each, Bundesliga/Ligue 1 306/yr) — no UEFA comps, so it parks.
   Bundesliga bapi 403s; La Liga is subscription-key-gated; Serie A unprobed (later).
   Seed SQL for the pulselive_pl family was REHEARSED rolled-back (clean) but the
   COMMIT was denied by the session's permission classifier — **Scott runs the
   one-liner** (in the session reply; file: scratchpad seed_boxscore_source_commit.sql)
   whenever he wants the family live. mig 209 (`fixtures.meta` jsonb, additive-inert)
   applied + snapshotted for the 4.4 flag.
2. **4.4 BUILT AND WIRED** (editor/nominate.rs + the mod.rs fork hook): result_line →
   parse → per-name exact team resolution (refuse on ambiguous/unresolved/same-team) →
   ±2d orientation-aware fixture match → correct-or-create with `meta
   needs_verification` (seeded rows never regress; scores map to fixture orientation;
   season + anchor derived in SQL from the article row — this crate never decodes
   timestamps). The EXISTING trigger owns the enqueue (verified: fires on
   INSERT-completed and on status/score/team UPDATE — no second enqueue added).
   Best-effort at the call site: a nomination hiccup never re-runs the model call.
3. **"Target URLs are a later tuning session — no covered sport is in season. Priority:
   the Investigator probing mystery entities + a reliable metadata write path"**
   (Scott, mid-session) → **4.5–4.10 PARK** (the landing-table/parser/replay-gate work
   resumes when a season does — top-5 leagues restart ~Aug 14–15); the session moves to
   **Phase 5** (nomination sweep, investigate_entity stage, discovery adapters, the
   5.5 write gate) — which is exactly the discovery+metadata machinery he named. Phase
   4's Verify stays OPEN pending the live-week gates; this is his ordering call, not a
   §0.2 violation by the executor.
4. **THE TARGET, clarified by Scott mid-session (ruling):** the Investigator system =
   (i) track down event box scores, (ii) normalize names + match to correct DB entities,
   (iii) search for mystery entities and populate the meta DB. **Vetting project: NBA** —
   our NBA metadata is wrong/missing (measured: 0 of 1,311 NBA players carry
   date_of_birth or photo_url; 16 missing weight, the rest untrusted), so metadata
   repair is the proving ground; **NBA head coaches are the test class for
   non-player/team entities** (persons kind=coach). Box-score LIVE sourcing waits for a
   season (none of our covered sports is in season); the plumbing (4.4 nomination →
   trigger → fixture_boxscore) stands ready. Source family for discovery/enrichment:
   **Wikimedia (Wikidata action API + Wikipedia REST)** — terms-clean by design;
   Wikidata claims are structured JSON, so ENRICHMENT NEEDS NO MODEL CALL
   (describe-then-derive at its purest); gemma triages only prose pages on the
   mystery-candidate path.

### Handoff (phase 4 → 5)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–4 committed: the Editor reads in shadow; the Investigator scrapes FOOTBALL box
scores from public sources; promotion calls finalize_fixture in-tx so the Scout only ever
reads landed stats. The module junctions/investigator/, Role::Investigator, and the
budgeted fetcher all exist — reuse them.
Read §0, §4 (rulings), Appendix B decisions D-2/D-3, and the write-gate section of wiki
planning doc 2026-07-27 (cross-repo_living-database-seeker) if reachable — else §1 and
Phase 5 are self-contained. Execute Phase 5 (the Investigator — entity discovery, stage
investigate_entity).
Laws: the Editor nominates, the Investigator verifies; search discovers, sources prove;
exact+discriminator or refuse — a name match alone NEVER merges identities; ambiguous is a
first-class outcome. No entity is written because a model knows a name.
```

---

## Phase 5 — Heart III: the Investigator (entity discovery)

Demand-led acquisition: the resolver's refusals and unresolved `names[]` become durable work;
verified people become `persons` rows the resolver can link tomorrow. (~60 coach-shaped
names/day measured in B3, plus the namesake ties roster context can't split.) The module,
the role, and the budgeted fetcher already exist — Phase 4 founded them.

- [x] **5.1** Nomination sweep (code, in the Editor handle after resolve): for each unresolved
      name with `kind_hint='person'` (and for `refused_ambiguous` ties): upsert
      `entity_candidates` (idempotency_key = `nrm(name)||sport`; repeat mention bumps
      `mention_count`/`last_seen_at`, never duplicates) + `candidate_mentions` row with the
      code-sliced quote + descriptor. Clubs/national teams nominate too but with
      `kind_hint='club'|'national_team'` — they take the `rejected_out_of_scope` path in v1
      (Appendix B D-3) and stand as the census.
- [x] **5.2** Enqueue rule (Scott, 2026-08-01 — direct relation beats the floor): a person-kind
      candidate whose mention carries a NON-EMPTY descriptor (the direct-story-relation signal:
      "Real Madrid manager", "PSG sporting director") enqueues `pipeline_work
      (stage='investigate_entity', entity_type='candidate', entity_id=candidate.id)` on FIRST
      sight. So does any refused-ambiguous tie. Descriptor-less bare names keep the 2-mention
      floor (the noise stays out; ~60/day person-shaped names measured in B3 sit comfortably
      inside the 4.2 fetch budget).
- [x] **5.3** `Stage::InvestigateEntity` in `work.rs` + handler in
      `rust/src/junctions/investigator/`: slot_group = `ARCHBOX_GEMMA_SLOTS` (Scott's call: same
      card), `max_in_flight` = 1 (the Editor outranks it; register after the Editor). The card's
      idle time (3.9a) is expected to absorb this easily — raise the knob after 5.10 if
      contention stays nil.
- [x] **5.4** Adapters, kept separate (discovery ≠ retrieval ≠ interpretation):
      *Discovery*: (1) Wikipedia REST search+summary API (documented, structured, ToS-clean —
      the v1 workhorse for professional identity), (2) Google News RSS query for the name +
      sport term (reuses the lungs' client shape). *Retrieval*: the Phase 4 budgeted fetcher
      (4.2), caching into `source_documents`. *Interpretation*: gemma **describes** the page —
      `{page_says: {name_forms[], role, org, sport, league, nationality, dates[]}, quote}` —
      and CODE decides (5.5). No browser automation in v1; a domain that blocks direct fetch
      is a domain we skip (never stealth).
- [x] **5.5** The write gate (`investigator/gate.rs`, deterministic): ACCEPT requires (a) ≥1
      `source_documents` row whose retained excerpt contains the name form, (b) sport-relevance
      from described role/org, (c) identity discriminator agreement (sport/league/team/role) —
      name similarity alone never merges (T9's cousin; do not rebuild BGE here); match against
      existing `players`/`persons` first — if an existing entity matches with discriminator,
      resolve the candidate to it (write alias, no new row). New person → `persons` row (kind
      may be `player` for story-relevant players OUTSIDE the stats platform — rookies
      pre-debut, retired, foreign-league; NEVER auto-insert `players` rows; if a person-kind
      player later appears in a box score, the Investigator reconciles by alias/external id —
      D-2) + `entity_aliases` (+ mirror into `entity_name_surfaces` via the mig-207 refresh or
      direct insert) + `entity_facts`/`entity_relationships` (e.g. `coach_of` with valid_from)
      each citing a source_document. Anything less → `ambiguous` (first-class, terminal until
      new evidence) or `rejected_*` with reason. `acquisition_runs` records every attempt +
      query_plan. Personal-life facts are NOT metadata (family kind exists for future editorial
      use; nothing auto-writes it in v1).
- [x] **5.6** Reopen policy: terminal candidates reopen only on a NEW distinct-article mention
      after `decided_at` + 30 days, or a manual reset. (No endless rediscovery loops.)
      Reopening an ACCEPTED candidate is the **maintenance loop**, not an error: re-verification
      re-runs the gate against current sources, supersedes changed relationships (`coach_of`
      closes with `valid_to`, the new one opens dated), and appends new aliases. The story that
      staled the fact is the story that fixes it.
- [x] **5.7** Adversarial fixture set `rust/fixtures/investigate_entity/` from the B3 census:
      kyle shanahan (accept: coach, NFL, 49ers), xabi alonso (accept: coach — despite an
      ex-player record shape), pep guardiola (accept coach; must NOT merge into player `sergi
      guardiola`), a drafted-but-undebuted rookie (accept: persons kind `player`, no `players`
      row), spain (out-of-scope national team → census, no write), celtic (out-of-scope
      club), andy burnham / lee child / ice (rejected_not_sport), vinicius tobias vs junior
      (discriminator split or ambiguous — never a coin flip). Gate: 100%.
- [x] **5.8** Review surface: SQL views `investigator_review_accepted` (latest 50 with sources),
      `investigator_funnel` (counts by state/kind/day). Sampling protocol in the Log: 20 accepted
      hand-checked; **one false merge is a stop-the-line event** (blocks widening any gate until
      explained + regression-fixtured).
- [x] **5.9** *(DEPLOYED 2026-08-04 03:57–04:10 EDT @ 78c923a — see Log)* Tests, fixture gate, **[DEPLOY]** rust to archbox (`COGNITION_STAGES` +=
      `investigate_entity`).
- [x] **5.10** *(CLOSED at +41h by Scott's 2026-08-05 tuning ruling — all bands read GREEN;
      longitudinal follow-up rides D-T10 — see Log)* Measure over 72h in the Log: nominations/day, candidates by state, acceptance
      rate, editor-slot contention (Editor coverage from Phase 3.9 must not degrade >5%), and
      the compounding metric: resolver links landing on `persons` rows (starts ~0, should grow
      as accepted coaches recur).

**Verify:** 5.8 sample clean (0 false merges); Editor coverage held; funnel view populated.
**Commit:** `rail: phase 5 — the Investigator verifies people`.

### Log (phase 5)

**2026-08-03, ~19:10–20:30 EDT: Phase 5 machinery BUILT in the same session that opened
Phase 4 (Scott's re-scope ruling — see Phase 4 Log entries 3–4: mystery-entity probing +
reliable metadata writes outrank box-score target URLs while no sport is in season; NBA
is the vetting sport; NBA head coaches are the non-player/team test class).**

Built and committed (`c852588`), all 331 lib tests green:

* **5.1/5.2 — the sweep** (`editor/candidates.rs`, hooked after persist in the Editor
  handle): unresolved person-kind names + refused ties → `entity_candidates` upsert
  (idempotency `lower(sport):nrm(name)` — nrm() runs IN SQL, mig 198) + a
  `candidate_mentions` row whose quote is code-sliced ±160 chars (char-boundary safe; the
  İ lesson has a regression test). Enqueue rule as ruled: descriptor-on-person or
  refused-tie → first sight; bare names → 2-mention floor; clubs/NTs →
  `rejected_out_of_scope` census, never enqueued (D-3). The 5.6 reopen rides the upsert
  (terminal person/tie candidates reopen on a fresh mention after decided_at + 30d;
  census rows never). `work::enqueue` idempotency prevents double-queuing.
* **5.3 — the stage**: `Stage::InvestigateEntity` (`investigate_entity`, candidate- OR
  player-keyed), handler registered after the Editor, `max_in_flight` 1, same slot group.
* **5.4 — adapters, kept separate**: discovery = Wikidata action API (wbsearchentities /
  wbgetentities) via the 4.2 BudgetedFetcher (2s spacing, 7-DAY cache — labels move
  slowly; every response is a `source_documents` row); interpretation for structured
  claims = **pure code** (`discover.rs::parse_wikidata_entity`: P106/P54/P6087/P569 +
  unit-gated P2067/P2048 — a pounds amount can NEVER land in a kg field; unknown units
  drop, never guess). The gemma prose-triage arm (for names Wikimedia doesn't know) is
  deferred to Appendix D as a tuning follow-up — Role::Investigator idles until then, and
  v1 makes ZERO model calls. Wikipedia REST summary adapter exists for that arm.
* **5.5 — the gate** (`investigator/gate.rs`, pure + `entity.rs` writes): ACCEPT = (a)
  stored excerpt contains the trusted name form (checked against `source_documents` at
  write time), (b) sport-relevance (occupation-QID tables per sport, description keyword
  fallback), (c) team-link discriminator (item's P54/P6087 QIDs resolved onto OUR teams
  via label → `nrm` surface match; newly proven QID↔team mappings write back to
  `entity_external_ids` with provenance — the mapping bootstraps itself). Name agreement
  is a SCREEN only and runs through `public.nrm()` in SQL — the first Rust-fold draft
  FAILED its own fixture on "Vinícius" vs "vinicius" and was replaced (mig 198's warning,
  vindicated in-session). Resolve-to-existing first (alias, no new row; player merges
  additionally require the career-team discriminator to agree — disagreement downgrades
  to ambiguous, never a merge). New people → `persons` (D-2 kinds; `family`
  unreachable) + append-only `entity_aliases` + direct surface mirror + role fact +
  `coach_of` relationship when role=coach and the team resolved. Every non-accept records
  `acquisition_runs` + a first-class state. **Enrichment mode** (player-keyed; Scott's
  NBA project): STRICTER discriminator (career must include THIS player's current team),
  facts date_of_birth / weight_kg / height_cm / photo_url written supersede-not-overwrite
  with provenance; convenience copies UPDATE `players` (never insert — box-score-owned);
  photo_url derives from P3647 (cdn.nba.com headshot URL — stored for clients, never
  fetched by us; that host blocks bots, 4.3 review). Wikidata+nba external ids recorded.
* **5.7 — the adversarial gate fixtures** (`fixtures/investigate_entity/cases.json`, 13
  cases, 100% required by test `adversarial_fixture_gate_is_one_hundred_percent`):
  spoelstra/shanahan/alonso accepts, **pep-must-not-merge-into-sergi** (ambiguous, never
  accept without discriminator), rookie-as-persons-player, burnham/child not-sport,
  vinicius namesake tie (never a coin flip) AND its discriminator split, thin-evidence
  single survivor → ambiguous, no-name-agreement → insufficient. The nrm screen enters
  fixtures as recorded input (it belongs to SQL).
* **5.8 — review surface**: mig 210 (`investigator_review_accepted` latest-50 with
  sources + last run; `investigator_funnel` day/state/kind/sport) applied + snapshotted.
* **Vetting seeds** (`scripts/investigator-vetting-seed.sql`): SMOKE block applied —
  3 headliner NBA players with missing dob/photo + Erik Spoelstra/Steve Kerr as
  hand-seeded person candidates (5 pending `investigate_entity` items). FULL block
  (all ~603 active-tier NBA players with gaps) left commented for Scott after the smoke
  review. Measured gap baseline: 0/1,311 NBA players carry date_of_birth or photo_url.

**2026-08-03, 19:20–19:55 EDT: BOUNDED LIVE SMOKE — three iterations, ending GREEN on
all five items; three real defects found and fixed by measurement (the whole point of
vetting on NBA).** Method: throwaway debug build on archbox (live checkout's debug cache
copied, new source overlaid — NO deploy, live service untouched; tree deleted after, root
disk back to 88%), worker run with `COGNITION_STAGES=investigate_entity` under timeout,
seeds from `scripts/investigator-vetting-seed.sql` SMOKE block.

*Defects measurement found (each now regression-pinned):*
1. **Kind misclassification, two layers.** (i) My occupation table said Q13365117 =
   basketball coach; Wikidata says that QID is **handball player** — the real basketball
   coach is **Q5137571** (verified live; every other QID in the table re-verified). (ii)
   Order: P6087 current-tenure now outranks all occupations; coach occupation outranks
   player occupation (dual P106 = retired player who coaches NOW — the Spoelstra item has
   NO P6087, coaching lives only in P106). Plus qualifier-aware parsing: P6087 with a
   P582 end qualifier is an ENDED tenure (the Pat Riley shape) — dropped; P54 keeps
   history (the discriminator wants it).
2. **Provenance containment vs the excerpt bound.** Şengün's claims JSON exceeds the
   100k retained_excerpt, so the label sat past the truncation and his correct accept was
   refused. Fix: clause (a) passes when the doc is the item's OWN wbgetentities fetch
   (label parsed from that document = containment by construction); the excerpt arm stays
   load-bearing for prose sources.
3. Stage allowlist in main.rs didn't know `investigate_entity` (boot fail-fast worked as
   designed).

*Final state (all live-verified in DB):* Spoelstra → persons id 7 kind **coach**,
coach_of team 16, Q440324; Kerr → persons id 6 kind **coach**, coach_of team 25, Q523630;
both with append-only aliases (incl. full legal names) citing source_documents.
Şengün → dob 2002-07-25, height_cm 211 (players.height "6'11\""), photo_url from his real
NBA.com id 1630578 (P3647), wikidata+nba external ids; facts active+sourced; players
convenience columns updated, nothing inserted. **9 team wikidata↔id mappings
bootstrapped** with provenance on first contact. Honest refusals: "Airious Bailey" (our
DB carries Ace Bailey's legal name; Wikidata label differs → thin evidence) and
"A.J. Green" (nrm "a j green" ≠ "aj green" — initials normalization) both Ambiguous,
never a guess. All Wikimedia fetches budgeted (2s spacing, 7d cache; cache hits observed
across iterations); zero model calls.

*Appendix D follow-ups from the smoke:* **D-T6** enrichment refusals leave no durable
trace (log-only) — a census row or players.meta note would let the review surface count
them. **D-T7** initials in nrm (A.J. ↔ AJ) — measure the class size before touching the
normalizer. **D-T8** name-mismatch class (legal vs known name: Airious/Ace) — the
Wikipedia-prose + gemma triage arm (5.4's deferred fallback) is the designed answer.

**OPERATIONAL (Scott, 2026-08-03 ~19:50 EDT): the Mac's character work is PAUSED** — his
call mid-session (Mac memory pressure). `COGNITION_STAGES` on archbox narrowed to
`scrub,graph,editor,article_read` (backup: `/tmp/env.local.bak-voice-pause` on archbox),
service restarted clean, `ministral-3:14b` unloaded from the Mac's Ollama (`ollama ps`
empty). Voice queues (peak/momentum/transfers/narratives/vibe/sigil) accumulate until he
resumes: restore the backup env + `systemctl --user restart scoracle-cognition`.
The editor replay drain is UNAFFECTED (archbox stages kept) — D-T1 verdict still lands
tonight. Scott also offered sudo on archbox: the standing want is
`sudo mkdir /mnt/data/scratch && sudo chown sheneveld /mnt/data/scratch` so future
rehearsal builds live off the root disk.

**What remains open on this phase:** 5.9 [DEPLOY] (add `investigate_entity` to
COGNITION_STAGES + place the release binary — fold into the next clean-window deploy,
which also carries the 4.4 nomination fork and the sweep going live on organic reads);
the FULL NBA seed (~603 players, commented block in the seed script) after Scott eyeballs
`investigator_review_accepted`; 5.10's 72h readings post-deploy.

---

**2026-08-03, ~21:30 EDT: SESSION CLOSED on Scott's ruling — "it's actually working; the
plumbing is in; park the meta gathering as part of the follow-up plan; finish up this
stage."** 5.1–5.8 ticked (built, tested, live-smoked green); 5.9 half-ticked (tests +
13-case fixture gate done at 100%; the [DEPLOY] arm stays open for a clean window); 5.10
waits on the deploy. The meta-gathering RUN is parked as **Appendix D-T9** per the
ruling — machinery done, operations deferred. Under the §4 tuning law this phase's
remaining substance is deploy + readings, not construction. Sudo note for the record:
Scott ran the /mnt/data/scratch grant on the MAC by mistake (`/mnt/data: No such file`) —
it must run ON archbox (`ssh archbox`, then the two sudo commands); low priority, his
words: don't chase. D-T1's paired replay verdict still lands with the next session
(drain was ~21 articles short at 20:28 EDT with the 21:00–22:00 rest pause ahead;
recipe pinned in D-T1). Mac voice work remains PAUSED (his call) — resume recipe in the
19:50 log entry above.

---

**2026-08-04, 03:57–04:15 EDT: 5.9 [DEPLOY] EXECUTED in the clean window — all four
verify items GREEN; 5.10's 72h clock starts at this deploy.** (The session's D-T1
housekeeping had already landed ~22:25 the night before — verdict + full 2×2 tables in
Appendix D-T1; headline: per successful read the Editor beats legacy on all three names
on the same articles, combined 83.3% vs 51.9%; Olise's raw deficit is stale-URL fetch
decay.) Deploy record: (1) pre-flight — 11 local commits pushed to origin (incl. the
D-T1 verdict 3246f14 and 78c923a adding `investigate_entity` to the cognition unit
template's documented set, the 3.8 sync discipline, since install.sh re-renders units
at release); archbox's two modified schema-snapshot files hash-verified byte-identical
to origin/main before `git checkout --`; `git pull --ff-only` 4871fbe → 78c923a.
(2) `.env.local`: COGNITION_STAGES=scrub,graph,editor,article_read →
+`,investigate_entity` — the ONLY line changed (diff-verified); voice pause preserved;
backup /tmp/env.local.bak-59-predeploy. (3) release.sh: all binaries @ 78c923ae5520,
API health green + serving the commit, cognition active, journal error-free. Worker
boot: `stages=["scrub","graph","editor","investigate_entity","article_read"]`, route
investigator=gemma3:4b@localhost. (4) Organic verify at +10 min: **the 4.4 fork + the
sweep are LIVE** — 70 editor reads since deploy (46 success, ~9/min), **84 new
entity_candidates** (69 pending persons; 13 club + 2 national_team census rows on the
D-3 path), **67 investigate_entity items enqueued** by the 5.2 descriptor rule, funnel
view populating (FOOTBALL 46 + NFL 23 pending persons on the day row). Investigator
drain was 0 at +10 min — expected, not a defect: the overnight editor backlog (7,495
at deploy) holds the whole ARCHBOX_GEMMA_SLOTS group and worker top-up is
registration-order (the Editor outranks it, 5.3's design); discovery rides card idle.
5.10 measures this contention for real. Note for the 5.10 reader: the investigator
holds a card slot while doing pure HTTP/code work in v1 (zero model calls) — if idle
never comes, the knob discussion belongs in Appendix D, not here.

---

**2026-08-04, ~20:45–21:05 EDT: 5.10 INTERIM READINGS at +16.6h of 72 (window closes
~Aug 7 04:00 EDT). No knobs touched; knob discussion opened as Appendix D-T10 per the
deploy note.**

*Operational shape discovered first (it reframes every reading below): news ingest is a
DAILY cron batch* — `0 2 * * *` EDT, `cron-pipeline.sh -mode ingest` — not a continuous
stream. The Aug-4 batch (7,985 arrivals, +15.6% vs Aug-3's 6,905) landed 02:00–02:28,
all before the 03:57 deploy; zero arrivals since deploy is the schedule, not a stall.
The editor spends ~19h of wall clock (incl. the every-3rd-hour rest pauses) digesting
each batch, so the queue only empties in a ≤1h window before the next batch lands.

* **Nominations/day (day 1): 4,448 candidates — 3,473 person** (FOOTBALL 1,806 / NFL
  1,048 / NBA 619; 6,147 mentions) **+ 975 club/NT census rows** (837 club, 138
  national_team, all `rejected_out_of_scope` per D-3). 3,462 enqueued = 99.7% of person
  candidates: the ep1 contract asks for a descriptor, so the 5.2 descriptor rule fires
  on effectively every person name on first sight — the 2-mention floor is near-dead
  letter. Against B3's ~60/day coach-shaped estimate this is 58×, BUT day 1 flushes the
  standing corpus of recurring names through "first sight" — steady-state must be read
  off day 2–3 at the close before any knob talk.
* **Candidates by state:** everything pending except the 2 smoke accepts
  (Spoelstra/Kerr). **Acceptance rate: no new decisions** — see contention.
* **Editor-slot contention: TOTAL, and structural.** 0 `acquisition_runs` since deploy;
  `investigate_entity` never once sampled `running` across 101 ten-minute qsample rows;
  editor pending never dropped below 1,664 (the minimum IS the latest sample, 20:40 —
  the queue has been draining monotonically toward tonight's ≤1h idle window). The +10min
  expectation ("discovery rides card idle") meets reality: under a daily ~8k batch there
  is almost no card idle to ride. This is D-T10, not a 5.10 failure — the design
  explicitly ranked the Editor first.
* **Editor coverage (the >5% bar):** Aug-3 batch (pre-deploy): 94.9% at +18.3h → **100.0%
  within 24h**. Aug-4 batch (post-deploy): **77.6% at the same +18.3h offset**. The raw
  gap is batch size (+15.6%) plus the deploy window, NOT investigator slot theft — the
  investigator got zero slots. Absolute throughput: 6,200 reads in 18.3h vs 6,552
  (−5.4%); ~490/hr in active hours. Remaining 1,664 pending at 20:40 projects drained
  ~02:00, right at the 24h bar — **judge the >5% bar on within-24h coverage at the
  close**, when the Aug-5/6 batches give clean post-deploy days.
* **Read statuses since deploy** (5,774 reads): 3,469 success (60.1%) / 1,059 irrelevant
  / 488 duplicate (8.5%, in the 5–12% band) / 358 blocked / 181 fetch_failed / 142
  parse_failed / 76 empty_body / 1 paywall.
* **Compounding metric: NONZERO on day 1.** 5 resolver links landed on `persons` rows
  since deploy (all Spoelstra, persons id 7, via the mig-207 surface mirror), alongside
  8,335 player + 5,300 team links. The loop closes: a person the Investigator accepted
  yesterday is a name the resolver links today. Growth is capped by the 2-row persons
  table until the queue drains (D-T10) or D-T9 runs.

---

**2026-08-05, ~21:10–21:40 EDT: 5.10 CLOSED at +41h on Scott's ruling ("this is a tuning
issue… finish up this phase") — every band GREEN; PHASE 5 CLOSED.** The ruling also founded
`PLAN-character-tuning.md` (the Character tuning session notes; convention written into the
Appendix D preamble) — this session's editor-efficiency findings landed there as D-T11/D-T12,
not as rail work.

✅ **VERIFY SATISFIED.** (1) **Editor coverage HELD exactly:** the Aug-4 batch (first full
post-deploy day, 7,985 arrivals) finished **100.0% within 24h** — identical to the Aug-3
pre-deploy batch's 100.0%; zero degradation against the >5% bar (the interim 77.6%
same-offset read was batch size, as suspected — confirmed by the closed window). Aug-5
batch (8,358 arrivals) tracking the same shape at 79.3% @ +19h. (2) **5.8 sample clean —
0 false merges.** All 10 accepts in existence hand-checked (census, not sample; the 20-row
protocol re-arms under D-T9 when the FULL seed runs): the 8 overnight organic accepts each
picked the right identity out of genuine namesake fields — Dan Quinn (Commanders HC,
Q5214234) split from two college-basketball Dan Quinns, a golfer-hockey player, and an
actor; Ivan Jurić (Q556688) from two ORCID researchers and a historian; De Rossi (Q168497)
from a bishop; plus Shevchenko, Alonso (the 5.7 fixture case, live), Iraola, McClure,
Hickman. (3) **Funnel populated end to end:** pending 6,700 / accepted 10 / ambiguous 23 /
not_sport 20 / insufficient 19 persons + 1,827 club/NT census.

**The overnight proof the design needed:** the investigator caught the predicted idle
window — 70 runs, 01:52–02:00 EDT, ended by the 02:00 batch — and decided 70 candidates
honestly (11.4% acceptance; every ambiguous a refusal, never a coin flip). **The
compounding metric compounds:** 102 resolver links landed on persons rows since deploy
(vs 5 at the interim read) — Xabi Alonso, accepted 01:56, drew 59 links the same day;
Iraola 23. Steady-state nominations ~3k persons/day (day-2 pace matched day-1 — not a
corpus flush): queue 6,670 and growing ~2.7k/day vs ~70/day drain — recorded as D-T10's
day-2 verdict; the drain-rate knobs are the tuning session's first Investigator item.

**Commit:** `rail: phase 5 — the Investigator verifies people`.

### Handoff (phase 5 → 6)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–5 committed: the Editor reads in shadow; the Investigator scrapes FOOTBALL box
scores and verifies people — the living database compounds daily. Read §0, §1b–§1c
(storylines/packets), then execute Phase 6 (storyline assembly + packet compile — all
deterministic code, zero model calls).
T3 is the law here: 0.5–0.75 similarity is the SAME STORY with a DIFFERENT CLAIM — attach,
never collapse. The disagreement is the story.
```

---

## Phase 6 — Heart IV: storylines and packets (code, not calls)

The desk work: everything in this phase is deterministic code — zero model tokens. The
Editor's reads become storylines; storylines become packets; packets carry the tags that
will route the characters (still inert — subscriptions stay empty until Phase 7).

- [x] **6.1** `editor/storyline.rs`: the §1b attachment rule, invoked at the end of every Editor
      handle (after `editor_reads` persists, after the 5.1 nomination sweep): compute candidates
      → attach or open → write `storyline_articles`, upsert `storyline_entities`
      (join/`last_seen_at`), set `editor_reads.storyline_id`. Log every decision (storyline_id,
      score, candidate count) at debug level. *(Done with TWO measured corrections to the
      scoring rule — Log; both pinned by fixtures.)*
- [x] **6.2** Backfill pass (one-shot bin, rehearsed rolled-back first): attach the shadow
      period's existing `editor_reads` (`attach_method='backfill'`, oldest first so storylines
      form in arrival order). The shadow corpus now spans Phases 3–5 — weeks of reads, not
      48h; batch it and let it run. *(`bin/storylinefill`; rehearsed, applied, reset, re-applied
      under the corrected rule — Log.)*
- [x] **6.3** `editor/packet.rs` — compile on storyline-dirty with a 15-minute quiet debounce
      (drain-loop tick checks `storylines.last_seen_at`): assemble §1c from member
      `editor_reads` (claims from `key_facts` with article/source/published_at attribution —
      NO dedup across sources beyond byte-identical facts; register = strongest non-neutral;
      quotes code-sliced from `full_text`; slice fingerprints per voice: journalist = hash of
      claims+headline, vibe = hash of register+phrase+claims, transfers = hash of
      transfer-typed claims), insert packet (supersedes prior), mark storyline clean. Packets
      INSERT will fire mig 206's trigger — still inert (0 subscriptions). *(Code + wiring done
      and unit-tested; the drain-loop sweep is GATED OFF behind `COGNITION_PACKET_COMPILE` —
      the trigger's Journalist arm is NOT inert, see the Log's BLOCKER.)*
- [x] **6.4** Storyline lifecycle sweep (hourly, code): `open → dormant` after 14 quiet days;
      resolution stays manual/downstream for now (D5's close-in-one-stroke is wired but only
      invoked when a resolution is recorded — the transfer chain is the first writer, Phase 7).
- [x] **6.5** Fixtures: storyline unit tests over canned editor_reads (the Real-Madrid-day
      shape: Diomande/Vinicius/Rodri/Lee Kang-in/Álvarez clusters — assert Lee Kang-in lands in
      its own storyline, NOT Real Madrid's, per the hand count).
- [x] **6.6** **[DEPLOY]** rust to archbox. *(Done 2026-08-05 22:08 EDT @ `a6c467b`, packet
      compile OFF per Scott's ruling → D-T14. Log has the deploy record + organic verify.)*
- [ ] **6.7** Measure over 72h, in the Log: storylines/day/sport; articles-per-storyline
      distribution (the top cluster should land ~15–25:1 against the 20:1 hand count — outside
      that band, STOP and inspect attach scores); % of reads attached vs opened-new; hand-inspect
      the 3 biggest storylines for wrong merges AND for a preserved contradiction (T3 spot
      check — find one "agreed"/"not agreed" pair sharing a packet's claims).

**Verify:** 6.7 bands; packet trigger fired 0 work rows (subscriptions still empty).
**Commit:** `rail: phase 6 — storylines assemble, packets compile`.

### Log (phase 6)

**2026-08-05 ~23:10 EDT — the Desk is built and the shadow corpus is assembled; the packet arm
is held at the door.** Commits `22cc18c` (code) → `bf70bc9` (rehearsal fix) → `6.1 correction 1`
→ `6.1 correction 2`. Tests 363 green, clippy clean on the new files. Nothing deployed: the
running service on archbox is still the Phase 5 binary, so every storyline below was written by
`bin/storylinefill` (built to `target/debug/`, never `rust/bin/`).

**BLOCKER (6.6/6.7) — mig 206's Journalist arm is not inert, and the Verify line assumes it is.**
The phase's Verify says "packet trigger fired 0 work rows (subscriptions still empty)". That
holds for arm 1 only. Arm 2 — `enqueue_voices_on_packet`'s unconditional `narratives` fan-out —
needs no subscription by design (7.4 says so explicitly), so every packet INSERT enqueues
`narratives` work for each active player/team participant with `input_version` `pk:<fingerprint>`,
while the legacy `article_read` seat is still enqueueing the SAME `(stage, entity_type, entity_id,
sport)` rows with its own `n:<hash>` (`article_reader/mod.rs:1319`). `work::enqueue`'s ON CONFLICT
reopens on any `input_version` change, so the two writers would alternate one row forever — the
mig-197 churn loop 7.4 warns about, arriving a phase early. Damage under today's config is
bounded (the Mac's voices are paused, so nothing claims the rows; the Journalist debounces on its
own material `input_hash`, so a claimed row costs a corpus read, not a generation) — but it is
queue churn nobody asked for, and this session will not improvise a seam Phase 7/8 owns.
Containment, not a fix: the compile sweep is behind `COGNITION_PACKET_COMPILE` (default OFF,
logged at boot); zero packets exist; the trigger has never fired. **Scott's ruling wanted on one
of:** (a) leave it dark until Phase 7 lands `RAIL` and seeds subscriptions (recommended — the
storyline half is already measurable without it); (b) a mig 211 that makes arm 2 subscription-gated
too, and 7.4 seeds `narratives`; (c) compile under legacy and accept the churn.

**6.1, and the two corrections the measurements forced.** §1b as written (entity overlap + type
match + recency, attach above 2) does not survive contact with this corpus. Rehearsal 1 over
2,000 reads: **569 articles in one storyline** — 28% of the corpus in a single "story". Cause:
every attach added the read's whole cast to the storyline's matching key, so a passing mention
pulled the next story in, which pulled its own cast — rich-get-richer until any transfer piece
naming two of the blob's clubs joined it. **Correction 1: a storyline's identity is fixed at its
seed cast** (the participants carrying the earliest `joined_at`); later members still join
`storyline_entities` — the fan-out and the packet want them — but do not extend the join key.
Top cluster 569 → **56** on the same 2,000 reads. The full 12,571-read apply then showed the
second mechanism: **304 articles** in an NBA storyline seeded by a conference listicle that named
six stars and five clubs at once; one shared star scored 2+1+1 = 4 against an 11-entity key.
**Correction 2: `covers_seed()` — the join must cover at least half the seed cast.** Sharing one
name out of eleven is not the same story; one out of two is. That backfill was reset (all 12,571
edges were ours; 0 packets existed) and re-applied under the corrected rule. Also fixed in the
rehearsal: two emitted names resolving to ONE entity crashed the participant upsert ("ON CONFLICT
DO UPDATE cannot affect row a second time") — `DISTINCT ON`, strongest role winning.
Weights, now pinned by fixtures: person 2, team 1, +1 type match, +1 recency (≤48h), attach above
3. People are the join; a club alone is a coincidence — Go queries one ranked feed PER TEAM, so
the club is on the hypothesis list of every article of its day.

**6.2 — the backfill, applied.** 12,571 unattached successful reads (every `success` read with
≥1 resolved link; 238 linkless reads deliberately left unattached — no entity is no join key) →
**6,164 storylines, 12,571 membership edges, 25,759 participant edges, 51.0% attached vs 49.0%
opened new**, 2.04 articles/storyline. Rehearsal invariants asserted inside the rolled-back
transaction and all clean: 0 unstamped reads, 0 participant-less storylines, 0 non-backfill edges.
Per sport: FOOTBALL 3,648 storylines (avg 1.79, top 109), NFL 1,818 (2.37, top 69), NBA 698
(2.50, top 84). Storylines/day: FOOTBALL 1,058–1,320, NFL 433–693, NBA 215–241 (Aug 2–4 full
days; the corpus is 4 days old, not weeks — the Editor deployed at 3.8).
Articles-per-storyline: 1 → 5,075 · 2–3 → 521 · 4–9 → 382 · 10–24 → 136 · 25+ → 50.

**6.7's bands, read early off the backfill (the live 72h reading still owed after 6.6).**
Articles-per-storyline top cluster = **109 over ~4 days (~27/day)**, above the stated 15–25:1
band as written. Hand-inspected, per 6.7's instruction, and the top cluster is NOT a wrong merge:
storyline #7474 is the Vinicius→Arsenal saga end to end (Goal, ESPN, Marca, The Mirror,
Football365, The Athletic via aggregators…). **T3 spot-check PASSES on it** — the same storyline
holds ESPN's "Vinícius Júnior is set to stay at Real Madrid despite Arsenal interest",
Goal's "Vinicius Junior is determined to stay at Real Madrid", Football365's "Arsenal have
reached an agreement in principle on personal terms with Vinicius Junior" and six members'
"The Athletic: deal not agreed", each attributed to its source. The disagreement survived
assembly; the packet compiler's byte-identical-only dedup is unit-tested to carry it through.
#3 (NBA, 84) is one Durant/76ers thread. **#2 (109, the Diomande transfer) IS carrying ~10
Vinicius/Arsenal articles** — its seed article named Vinicius in passing, so the two Real Madrid
sagas share a 2-entity slice of a 4-entity key. That residue is one class, it is diagnosed, and
it is a tuning item, not a mechanism failure → **D-T13**. No further tuning this session (§4: the
rail stands first).

**2026-08-05 ~22:05–22:20 EDT: SCOTT'S RULING, then 6.6 [DEPLOY] EXECUTED — 6.7's 72h clock
starts here (window closes ~Aug 8 22:10 EDT).** The ruling: *"Leave all the actual model testing
until we've completed the rail. Mark this issue as part of the tuning. We're going to go through
each junction and tune — this is an issue for that session. We're building the rail first."* So
the fan-out seam above is **D-T14**, not a phase gate: the flag stays off, the storyline half
ships live, and Phase 7 lands `RAIL` before anything compiles a packet. Deploy record: (1)
pre-flight — archbox `git pull --ff-only` → `a6c467b`, working tree clean; `.env.local`
UNCHANGED (grep confirms `COGNITION_PACKET_COMPILE` absent = off; the voice pause and the 5.9
stage list are untouched). Deployed inside the 22:00–00:00 ACTIVE window, so no rest window was
overridden. (2) `scripts/hosting/release.sh` → all binaries @ `a6c467bce18a`, API healthy and
serving the commit, cognition `active`. Boot lines: `stages=["scrub","graph","editor",
"investigate_entity","article_read"]` and **`desk: storyline assembly always on; packet compile
gated by COGNITION_PACKET_COMPILE packet_compile=false`**. (3) Organic verify at +8 min: of the
successful reads since the deploy stamp, **4 of 4 attached, 0 unattached-with-links** — the live
§1b path works on organic arrivals; **2 new storylines opened**, `attach_method` now `auto 4 /
backfill 12,571`; **packets 0** (the trigger has still never fired); journal free of
`storyline attach failed` / `packet compile failed` / errors. Candidate-query cost measured
before the deploy on the busiest possible input (Real Madrid + Vinicius, 69 candidates):
**18.5 ms**, against a stage whose model call is ~20–35 s.

**6.3/6.4/6.5 — what is built but not running.** `packet.rs` compiles §1c in a pure function
(members + participants → draft) with the loader and the append around it: headline by lowest
`feed_rank`, claims attributed and deduped ONLY byte-identically (first filer keeps the credit),
register = the one the members most agree on with its newest phrase (no invented intensity
ladder — the Influencer owns the score), quotes code-sliced ±160 chars from stored bodies,
`facts` thin and structured, the unresolved census rolled up, claims capped at 200 with the
dropped articles NAMED (the A5 rule), and STAGE-keyed slice fingerprints — a test pins the keys
`narratives`/`transfers`/`vibe` because a character-named key would fail open on every packet
forever. `worker.rs` gained one Desk pass after each drain: the hourly dormancy sweep (open →
dormant at 14 quiet days) always, the packet compile only behind the flag; both run only where
the Editor is seated, so the Mac never sweeps. D5's `resolve_storyline` (close every other edge
`not_the_outcome` in one stroke) is wired and uncalled — Phase 7's transfer chain is its first
writer. Fixtures: the Real-Madrid-day replay asserts Lee Kang-in opens his own storyline rather
than joining Real Madrid's on a shared club link, the 20-article saga stays one cluster, the
listicle does not swallow the conference, a passing mention does not extend the key, and a
contradiction attaches rather than forking.

**2026-08-05 ~22:12 EDT — 6.7 CHECKPOINT AT +4 MIN, NOT A READING. The 72h window opened
tonight and closes ~Aug 8 22:10 EDT; at the time of this session only four minutes of
post-deploy life existed, so the bands 6.7 asks for (storylines/day/sport, the
articles-per-storyline distribution, % attached vs opened-new, the 3-biggest hand
inspection) CANNOT be read yet — a distribution over four minutes is noise, and the
backfill numbers above remain the pre-deploy baseline they were logged as. 6.7 stays
OPEN.** What the checkpoint does prove is that the live path is healthy: the new binary's
`ExecMainStartTimestamp` is 22:08:27, and of the successful reads with ≥1 resolved link
since that instant, **17 of 17 attached, 0 unattached** (per-minute: 22:09 4/0, 22:10 6/0,
22:11 4/0, 22:12 3/0). The 44 unattached-with-links rows all predate the restart (38 from
22:01–22:07 under the old binary, 6 more at 22:08:00–22:08:27 before the new one took the
seat) — they are the restart's shadow, not a failure. Totals: **6,170 storylines** (6,164
backfill + 6 opened organically), `attach_method` auto 16 / backfill 12,571, **packets 0**
(the trigger has still never fired). Nothing new for D-T13 at this resolution; the bleed
rate it asks for is exactly what the 72h window is for.

### Handoff (phase 6 → 7)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–6 committed: the heart is whole — the Editor reads, the Investigator scrapes box
scores and verifies people, storylines assemble, packets compile in shadow. Read §0, §1c
(slice fingerprints), §2 (you are wiring the seam the cutover will flip), §3 (Mac topology,
from Phase 0 Log 0.11), §4 (memory is characters-only), then execute Phase 7 (the brain —
voices onto packets + the memory loop + the voice diet, behind RAIL=legacy).
Laws: no prose reaches the Scout (T4); the 4096 window is HARD (prompt + memory + packet +
output, 7.2); memory is characters-only and stays out of input_hash; voice routing is data
(subscriptions), never asked of a model (E1); diet prompt versions are RAIL-scoped so
legacy stays bit-identical; nothing user-visible changes until Phase 8 flips RAIL.
```

---

## Phase 7 — Brain: the voices read packets, keep their memories, and go on a diet

Everything lands behind `RAIL` (env, default `legacy`). Legacy behavior is bit-identical until
Phase 8 flips it — including prompts: the diet versions are RAIL-scoped (7.11), so the legacy
rail keeps n16/v16/t11/is3/s16/momentum-s13/or8 byte-identical until flip day.

**The 4096 envelope (Scott, 2026-08-01 — the window budget, per voice; targets, measured in
7.15).** Today's system prompts run 2.5–7.2 KB (or8 is ~1,800 tokens alone), `VOICE_NUM_CTX`
is a compile-time 16384 (`route.rs:76`), and 7.2% of historical prompts would truncate at
4096 (HANDOFF 2026-07-28 measurement). The diet makes the window honest:

| part | budget |
|---|---|
| voice system prompt (post-diet) | ≤ ~550 tok (the voice, concise and clear — boilerplate cut) |
| memory block (relational card + last-4 prior reads) | ≤ ~700 tok, code-truncated |
| packet render (or stats context for the Scout) | ≤ ~2,000 tok |
| `num_predict` reservation | ≤ 800 (Journalist 700; the 4000/2000/1200/1100-era reservations die with the 16384 window) |
| **envelope** | **prompt p99 ≤ 3,300 over the shadow corpus, asserted via `eval_count` telemetry** |

- [x] **7.1** `RAIL` config (`rust/src/config.rs`): `legacy|packet`, read once at boot, logged
      loudly at startup. *(Done — parsed total, carried on the `Harness` so no handler re-reads
      the env mid-drain.)*
- [x] **7.2** Packet renderer (`editor/render.rs`): `(packet, entity, voice) → context block`,
      hard budget ≤2,000 tokens (tiktoken-free heuristic: chars/3.6, then assert with
      `eval_count` telemetry): headline, this entity's role + participation dates, claims
      (attributed, contested pairs marked `⇄`, newest first, truncate oldest first), register +
      phrase (Influencer render only), facts, one continuity line from the prior packet.
      Property-based test: NO packet in the shadow corpus renders >2,000 for any voice.
      *(Done — `editor/render.rs`, 10 tests; the Scout has no `Voice` variant, so T4 is enforced
      by the type. The contested marker is a mechanical rule — Log.)*
- [x] **7.3** Journalist: `load_packet_corpus` beside `load_vetted_corpus_with_exclusions`
      (`journalist/mod.rs:380`), selected by RAIL inside `load_narratives_material`
      (`journalist/mod.rs:1078`). Packet path: entity's packets from the last 72h, rendered,
      `num_ctx` 4096, `num_predict` 700, exclusions telemetry intact (every packet dropped by
      budget is named — the A5 rule). *(Done — it returns the SAME `(Vec<CorpusItem>,
      CorpusExclusions)` so grounding/impact/hash/marker are shared code; window+reservation are
      rail-scoped together and `voice_num_ctx(rail)` moves all six voices at once. Log.)*
- [ ] **7.4** ⏸ **DEFERRED TO PHASE 8 by Scott's ruling (2026-08-05): "we don't need to seed
      anything until the cutover."** That closes D-T15 without a migration: nothing is seeded, so
      mig 197's live article-grain trigger is never armed and the churn loop cannot start. The
      seed moves INTO the cutover act (§2's "one act"), where mig 175 is dropped in the same
      session and there is only ever one writer. **Phase 8 precondition, do not lose it:** 7.6
      gated the Journalist-side vibe enqueue to legacy, so under `RAIL=packet` the Influencer
      has NO waker until these rows exist. Seed and flip together. Original blocker — (seeding arms mig 197's LIVE article-grain trigger too; `'*'`
      is not a wildcard). Prepared and unapplied at `sql/prepared/7.4_seed_packet_subscriptions.sql`.
      Seed `stage_routing_subscriptions` (the E1 INSERT, packet-grain):
      `('transfer','transfers','team')`, `('charged','vibe','*')`. The Journalist needs no row
      (mig 206 fans narratives unconditionally). **Do not** also leave mig-175's article-grain
      transfers trigger pointing at the same stage post-flip — Phase 8 drops it; until then the
      RAIL gate in the transfers handler ignores packet-work rows under legacy (input_version
      prefix `pk:` is the discriminator).
- [x] **7.5** *(Done — the pair's IDENTITY stays Postgres's, the packet replaces its MATERIAL; Log.)* Insider (`transfers` handler): RAIL=packet path reads the packet render (its
      slice = transfer-typed claims) instead of the article-window query; the
      `transfer_identity_applications` adjudication chain downstream is UNTOUCHED (it is kept
      substrate, news-derived, and the Scout's road).
- [x] **7.6** *(Done — packet material + E3's first-voice fix + the wake-up handover; her CONTRACT TEXT deferred to the junction pass, Scott 2026-08-05. Log.)* Influencer (`vibe`): E3 — under packet RAIL she wakes from the packet trigger
      (`charged` tag), first-voice-capable: fix `enqueue_vibe_if_needed`'s empty-context no-op
      for packet work, update her contract text (she may file first; register_phrase is her
      material). The Journalist-side enqueue remains for legacy mode only.
- [x] **7.7** *(Done 2026-08-06 — DEPLOYED @ `2c6b038`. Still no packet subscription and still
      no `Voice::Scout`. The block is the personnel DELTA the memory card structurally cannot
      carry: team departures, the club a player came FROM, reverts, and the since-last-read
      anchor. Out of `input_hash` (the 7.8 ruling); `s16` NOT bumped — s17 belongs to the diet.
      Log.)* Scout (`peak`): NO packet subscription (T4, §4 stats-before-Scout). Two
      confirmed-fact roads only: (a) the stats platform (Investigator-fed since Phase 4);
      (b) `transfer_identity_applications` applied/adjudicated rows — add a compact "personnel
      changes since last read" block to the PEAK context from that table (facts with dates, no
      prose). Injury/suspension confirmation gates (the F4 pattern: claims → threshold →
      confirmed) are **deferred** to post-cutover (Appendix B D-5) — do not improvise them here.
- [x] **7.8** *(Done except E5, deferred to the junction pass by Scott's 2026-08-05 prompt ruling — the DISAGREEMENT field is an output-contract change, i.e. voice work. The window half IS done: the crown's pillar bodies are capped. Log.)* Analyst + Oracle: Analyst's context assembly gains the packet render under RAIL
      (peer-aware inputs unchanged). Oracle mechanics untouched — but implement E5 while we are
      here: when deterministic `pillar_convergence` < 40, the prompt hands the divergence as a
      decided fact and `DISAGREEMENT:` becomes a REQUIRED field (grammar-enforced), narrating
      it. One fixture proves it fires (a guard never observed firing is not a guard). For the
      4096 window: cap each pillar body in `build_crown_prompt` (~350 tok/card — today it
      truncates nothing) so five cards + memory + prompt fit the envelope.
- [x] **7.9** *(Journalist half done + pinned; the other voices' halves travel with 7.5/7.6/7.8.)*
      Memory continuity (verify, mostly not build — §4 memory ruling): each voice's
      packet-era context KEEPS its memory card + prior-read block — the packet render replaces
      the CORPUS, never the memory. The infrastructure exists and survives the flip:
      `narrative_context_for_entity` (schema.sql:3353, five-lens self-history),
      `narrative_context_for_pair` (Insider), `stat_context_for_entity` (Scout),
      `source_reliability_for_pair`, and the per-voice last-4 loaders (journalist mod.rs:615,
      insider :1924, oracle :931, influencer :408). Confirm each loader is called on the packet
      path, provenance labels intact, memory still excluded from `input_hash`, memory-load
      errors still degrade to unenriched prompts (never fail the item).
- [x] **7.10** *(Done — mig 211 APPLIED + snapshotted; inert for storyline-free entities, measured 40/40. Log.)* Storyline memory lens (mig 2xx): extend `narrative_context_for_entity` with a
      storylines section — open storylines via `storyline_entities` (role, joined_at, latest
      packet headline, `Prior story:` provenance label) — so the Journalist's "life of stories"
      memory survives thread retirement (Phase 9 kills thread clustering; the thread-fed lenses
      age out via their own date windows and read archive until then). Run it; renders identical
      output for entities with zero storylines, which proves it is wired without changing
      behavior.
- [ ] **7.11** **The voice diet — ONE re-earn event.** Per-voice system-prompt slim to the
      envelope target: the voice stays (concise and clear on the character), the boilerplate,
      restated field lists, and defensive hedging go. Version bumps (n17/v17/t12/is4/s17/
      momentum-s14/or9), RAIL-scoped: legacy rail keeps the old consts verbatim, packet rail
      gets the diet versions (the old consts die in Phase 9). Capture packet-context fixtures
      via `eval --capture-ledger` on shadow renders against **ministral-3:14b** IN THE SAME
      step (a model change re-earns its seat on the voice fixtures — D-1; one gate, not two),
      judged with judge-v2's voice-fidelity axis. Gates green on BOTH rails (legacy fixtures
      still pass against the untouched legacy prompts).
- [x] **7.12** *(Done — the routes were already live; what this step actually added was `VOICE_NUM_CTX` as a dial independent of the rail, because Scott chose 4096 NOW. Log.)* Mac routing config (values measured in Phase 0 Log 0.11): for the six voice
      roles, `COGNITION_ROUTE_<ROLE>_BASE_URL=http://192.168.1.77:11434` and
      `COGNITION_ROUTE_<ROLE>=ministral-3:14b` (Appendix B D-1, closed — **not** Nemo; Gemma stays
      pinned on archbox for editor/investigate_entity/graph and is never routed here),
      `COGNITION_BACKEND_CONCURRENCY=http://192.168.1.77:11434=3,http://localhost:11434=4`,
      voice `num_ctx` stays 16384 under legacy / 4096 under packet (RAIL-scoped constant —
      change `VOICE_NUM_CTX` const to a RAIL-aware fn). Mac worker runs voice stages only
      (`COGNITION_STAGES` on the Mac).
- [x] **7.13** *(Done — 52.0% of the last 24h now read the Editor's blurb; the gated enqueue is
      dead code with a live test until the flip.)* Graph continuity (the G1 seam — §4 graph ruling): repoint
      `load_graph_article_context` (graph/mod.rs:134) to prefer
      `editor_reads.read->>'evidence_blurb'` with fallback to the legacy reading (safe on both
      rails, deployable now); add the RAIL=packet-gated `enqueue_graph_for_article` call to the
      new Editor handle AFTER its link writes (activates at flip — until then graph keeps
      riding article_read via mig 193). Graph keeps `ARCHBOX_GEMMA_SLOTS` + num_ctx 8192.
- [x] **7.14** *(Done — DEPLOYED 2026-08-05 23:28 EDT @ `f256abb`; all 11 stages registered, the voices are back on and ministral is loaded on the Mac at context_length 4096. Log.)* Tests, clippy, **[DEPLOY]** rust to archbox AND the Mac worker, RAIL=legacy
      everywhere. Verify boot logs on both machines print rail + routes + backend budgets, and
      diff the legacy prompt consts against HEAD~1 — byte-identical (the diet is packet-only).
- [ ] **7.15** Dry-run under eval (not production): run each voice's packet path against 5
      shadow packets on the Mac; record p50/p99 prompt tokens (p99 ≤3,300 INCLUDING the memory
      block), output sanity, Mac concurrent-3 sustained without runner reloads (uniform 4096
      num_ctx per host — mixed num_ctx forces reloads, `route.rs:52-75`).

**Verify:** legacy production metrics unchanged over 48h post-deploy (T5 says gates can't see
this — compare production rates to Phase 0/3 baselines; legacy prompts byte-identical); packet
dry-runs within the envelope; both fixture gates green; memory blocks present in packet-path
prompts (spot-check 5 ledger rows for the provenance labels).
**Commit:** `rail: phase 7 — the brain is wired for packets; the voices keep their memories`.

### Log (phase 7)

**2026-08-05 ~22:15–23:05 EDT — the switch, the renderer, the Journalist's new corpus, the
graph seam; 7.4 stopped on a measured hazard.** Commits `217bc46` (7.1–7.3) → `f24dcbf`
(7.13/7.9/the 7.4 block). 380 tests green (363 → 380), clippy clean on every touched file.
**Nothing deployed** — the running binary on archbox is still `a6c467b`, the Phase 6 one.

**7.1 — `RAIL`.** `legacy|packet`, default legacy, parsed TOTAL (an unparseable value resolves
to legacy rather than failing a boot, same reasoning as `env_bool`; the boot line states what
it resolved to). Read once in `Config::from_env` and carried on the `Harness`, so no handler
re-reads the environment mid-drain and two items in one pass cannot disagree about which
corpus they are reading. Boot now prints `RAIL: the voices read legacy` beside the Desk's
`packet_compile=false`.

**7.2 — the renderer.** `editor/render.rs`: `(packet, entity, voice) → block`, hard-capped at
2,000 estimated tokens (`chars/3.6`, deliberately pessimistic against a measured ~3.9, so the
estimate errs toward rendering less; 7.15 still owes the `eval_count` assertion). Claims
render newest-first and truncate from the OLD end, and every dropped article is NAMED in the
block itself, not only in telemetry (A5). Two design choices worth the record:
* **T4 is enforced by the type.** There is no `Voice::Scout` variant to pass. The law stops
  being something a reviewer has to remember.
* **The contested marker is mechanical** (T2 — code renders the judgment). Two claims are a
  contested pair when their content-word stems overlap at ≥0.5 by the overlap coefficient AND
  their negation polarity differs; both are then marked `⇄` and **both always stand**. Stems
  are the first five characters of each non-stopword token — crude, and exactly enough to make
  "agreed"/"agreement" one stem, which is the whole job. The Phase 6 Log's T3 spot-check is now
  a fixture: Football365's "agreement in principle" and The Athletic's "deal not agreed" render
  side by side, attributed, marked. Hedging is deliberately NOT contradiction ("could collapse"
  does not contest "is agreed" — it is the same story told softer), and ESPN "set to stay" vs
  Football365 "agreement reached" is tension without opposite polarity, so it goes unmarked.
  A false mark costs a `⇄` on a line that stands anyway; a miss costs the pointer. Both are
  survivable, which is the only reason a heuristic is allowed to hold this pen.

**7.3 — the Journalist reads packets.** `load_packet_corpus` sits beside
`load_vetted_corpus_with_exclusions`, selected by rail inside `load_narratives_material`. **The
shape is deliberately unchanged**: it returns the same `(Vec<CorpusItem>, CorpusExclusions)` —
one item per MEMBER ARTICLE, carrying that article's attributed facts, contested ones prefixed
`⇄` — so the debounce hash, the SIGNALS line, citation grounding, impact scoring and the
no-corpus marker path are all shared code, and the model still cites real `news_articles.id`s
it can be grounded against. What changes is the material: read FACTS instead of headlines and
body excerpts, with the storyline framing (the story, this entity's part in it, one
`PREVIOUSLY:` line) rendered above the numbered evidence. The packet rail carries no bodies at
all — that is the diet.
* Window and reservation are rail-scoped **together** (`narratives_decode_budget`): 16384/4000
  legacy, 4096/700 packet. A window that cannot hold its own reservation is the silent
  system-prompt eviction this constant already documents, so they were never going to move
  separately.
* `VOICE_NUM_CTX` became `voice_num_ctx(rail)` and **all six voices** were switched to it in one
  step. Doing only the Journalist would have put one voice at 4096 beside housemates at 16384 —
  the measured reload thrash (~a fifth of the Mac's wall clock), reintroduced by the fix for it.
* The narratives ledger now reads `context_budget` off the EXACT wire body instead of restating
  the constants; a packet-rail call reporting legacy numbers would have been the one place the
  flip was invisible.
* **Pinned: under `RAIL=legacy` the prompt is byte-identical.** An empty or whitespace framing
  is indistinguishable from no framing, so a legacy deploy carrying all of Phase 7 sends exactly
  what the Phase 6 binary sent.

**One Phase 6 contract extension, made deliberately.** `packets.claims` now persists
`story_type` (§1c listed four fields; 6.3 kept the type in memory only). The Insider's slice IS
the transfer-typed claims (7.5) and `slice_fingerprints.transfers` hashes exactly that subset —
a renderer blind to the type would render a different slice than the fingerprint promises, and
E2's "re-read only when YOUR slice moved" would be false in both directions (silent staleness,
or a re-fan that changes nothing). Free to make: zero packets have ever been compiled.

**7.13 — the G1 seam.** `load_graph_article_context` now prefers the Editor's
`editor_reads.read->>'evidence_blurb'`, falls back to the legacy reading's, then the RSS
description (last, because it is 99.7% the title repeated). Safe on both rails and deployable
before the flip — and it is not cosmetic: over the last 24h on archbox, of 7,674 non-duplicate
articles **3,990 (52.0%) now carry an Editor blurb and 0 fall through to the legacy blurb**, so
this upgrades half the corpus today and is a no-op for the rest. The Editor also gained a
`RAIL=packet`-gated `enqueue_graph_for_article`, keyed on the editor read's content_hash
(`g:`), placed AFTER the link writes — graph's candidate list is the vetted entities, so an
enqueue that beat them would fail closed for a reason that has nothing to do with the article.
Under legacy it is dead code with a live test; graph keeps riding `article_read` via mig 193.

**7.9 — verified, not built (the Journalist's half).** Every named loader exists and is called.
`load_entity_memory` (`narrative_context_for_entity`) and `load_prior_card_reads` sit in
`finish_narratives_build`, which is rail-independent: the rail only decides which corpus loader
ran upstream, so **the packet render replaces the CORPUS and never the memory**. Memory stays
structurally out of `input_hash` (`build_narratives_input_components` takes corpus + heat only)
and still degrades to an unenriched prompt on error rather than failing the item. Pinned by a
test. The plan's cited line numbers have drifted (they were pre-Phase-7) but every function is
present: insider `narrative_context_for_pair` :887 / `source_reliability_for_pair` :911, oracle
`load_prior_read` :931, influencer `load_latest_vibe_row` :408, scout `stat_context_for_entity`
:716. The other voices' halves travel with 7.5/7.6/7.8, which have not been built.

**7.4 — STOPPED per §0 rule 3, and this one is worth the stop → D-T15.** The step's stated data
does not match the trigger it feeds, and the mismatch is live. `stage_routing_subscriptions` is
read by TWO triggers: mig 206's packet trigger (inert — 0 packets, compile off) and mig 197's
**`enqueue_voices_on_routing_tags`, which is enabled on `news_articles`** and fired by
`article_reader`'s routing_tags write on every legacy read. Seeding
`('transfer','transfers','team')` would therefore start enqueueing the transfers stage as
`s:transfer:…` against mig 175's still-live `t:…` on the same `pipeline_work` key (all 132 live
transfers rows are `t:` today) — the mig-197 churn loop, on the LEGACY rail, on a production
stage: the same failure D-T14 parked for packets, arriving through the other door. Separately,
**`'*'` is not a wildcard** — both triggers join `entity_type` on strict equality, so
`('charged','vibe','*')` would fan out to nobody and the Influencer would silently never wake.
The seed is written, corrected to two rows (player + team), and **deliberately unapplied** at
`sql/prepared/7.4_seed_packet_subscriptions.sql` with its preconditions in the header. Not
improvised around; the cheap partial (seed only `charged`/`vibe`, hold the transfers row until
Phase 8 drops mig 175) is offered there and is what 7.6 actually needs.

**Not started (as of that entry): 7.5, 7.6, 7.7, 7.8, 7.10, 7.11, 7.12, 7.14, 7.15.** 7.11 and
7.15 are model work Scott's 2026-08-05 ruling parks until the rail is complete (the diet's prompt
CODE is buildable; its `eval --capture-ledger` re-earn against ministral-3:14b is the tuning
session's). 7.5/7.6 are buildable now and only their WAKE-UP depends on D-T15.

---

**2026-08-05 ~23:05–23:35 EDT — the brain is wired, the window is pinned, and the voices are back
on.** Commits `2f1a5cd` (7.5/7.6) → `a3f7cd0` (7.8) → `d1edce1` (7.10, mig 211) → `f256abb`
(7.12/7.14). **DEPLOYED 23:28 EDT @ `f256abb`, corrected 23:42 @ `ac131ca`** — 391 tests green, clippy clean on every touched
file.

**SCOTT'S TWO RULINGS THIS SESSION, and they redirected the work:**
1. **"I don't want to focus on the prompts for the models at this point. I want to get the rail
   built and start having the data trickle through… I'd rather get the whole rail in operation
   and then spend the time going through all the junctions this weekend."** So every step below
   moves MATERIAL and leaves VOICE alone. Deliberately NOT built, and listed here so the junction
   pass finds them: the Influencer's first-voice contract text (7.6), **E5's grammar-enforced
   `DISAGREEMENT` field (7.8)** — it changes the Oracle's output contract and its `format_schema`,
   which is voice work by any reading — and the diet itself (7.11). No prompt VERSION was bumped:
   the contracts did not change, only what fills them, and the packet rail's `input_hash` moves on
   its own material anyway.
2. **"We don't need to seed anything until the cutover. Which will be soon."** D-T15 closes
   without a migration (7.4 above). Nothing is armed; the seed becomes part of Phase 8's one act.
3. **"Run them, but run them at 4096."** → the whole of 7.12 below.

**7.5 — the Insider.** The pair's IDENTITY stays Postgres's: `compute_transfer_heat` still decides
the heat, the corpus ids and therefore the F3 fingerprint (§4 — the number is never the model's,
and it is not the packet's either). The packet replaces the pair's MATERIAL: for every article
already in the pair's corpus, the Editor's **transfer-typed** claims (the exact subset
`slice_fingerprints ->> 'transfers'` hashes) overlay the RSS headline and description, contested
ones marked `⇄`, with the storyline framing above them. Articles the Desk has not assembled keep
their headline — the same passthrough principle the Journalist's `article_context` already
documents. Because the article LIST is untouched, `prompted_news_ids`, the evidence card, the
`transfer_identity_applications` chain and the debounce cannot tell the rails apart, which is the
property that lets the flip be one env var.

**7.6 — the Influencer, and E3.** Her context gains her own render — `MOOD:` and the register
phrase reach HER and no one else, enforced by the renderer's voice rule, not by a caller's flag.
The first-voice fix is one line of meaning: `VibeContext::empty()` now counts packets, so a
packet-woken entity with no narratives and no heat is no longer "empty" and
`enqueue_vibe_if_needed` no longer no-ops. Until this, she was structurally incapable of speaking
before The Journalist, because her only material was his output. Packet ids enter the debounce
pre-image (append-only ⇒ a recompile is a new id, an unmoved story keeps its own) and ONLY on the
packet rail, so every legacy `input_hash` is unchanged — pinned by a golden.
**The Journalist-side vibe enqueue is now legacy-only**, and this is not tidiness: on the packet
rail the trigger owns her wake-up, and leaving the handler's enqueue armed would put two writers
on one `pipeline_work` row with different `input_version` prefixes (`vibe:` vs `pk:`) — the
mig-197 churn loop arriving through a third door. One rail, one waker. **Consequence carried to
Phase 8: the 7.4 seed and the flip must land together, or she never wakes.**

**7.8 — the Analyst, and the crown's window.** The Analyst's storylines render between the memory
card and the decided direction — context for WHAT is moving, never a licence to re-litigate a
direction computed upstream; loaded in the handler, so its TRIGGER stays the pillar cascade and
the packet stays out of the `input_hash`, exactly like the scouting paragraph beside it. **The
Oracle reads no packet at all** — §4 keeps it blind to evidence (five cards + its own trail), so
7.2's sketched crown render was deliberately not built and `render.rs` now says so on the
`Voice::Oracle` variant. What the Oracle got instead is the cap: it is the one seat that reads
five cards at once and until now truncated NONE of them, which was survivable only inside 16,384
tokens.

**7.10 — the storyline memory lens (mig 211, APPLIED + snapshotted).** One
`Our storyline so far ("…", opened Mon DD, N reports, this entity's part: role)` line per open
storyline the entity actively participates in, rendered where the thread block renders. Phase 9
retires thread clustering; this moves the "life of stories" memory onto the Desk's storylines
BEFORE the structure under it is removed. Membership counts only — no impact, no likelihood, no
heat (the mig 179/183 discipline: measurement stays graph-anchored). **Measured at apply time
against cards captured before the function changed: 40 of 40 storyline-free entities
byte-identical (7.10's own verify), 40 of 40 participants gained the line, 0 cards lost content.**
*Process note, recorded because the next function rebuild will hit it:* the rehearsal wrapper did
NOT roll back — the migration file carries its own `BEGIN/COMMIT`, so `\i` committed inside the
rehearsal transaction. The assertions still ran against the pre-captured baselines, so the
verification is the intended one; only its order changed. **Strip a migration's own transaction
control before `\i`-ing it into a rehearsal.**

**7.12 — the voice window becomes its own dial (Scott: "run them at 4096").** The window and the
rail were one knob; they are two now. `VOICE_NUM_CTX` pins what every voice on a host requests and
falls back to the rail's size when unset (total parse like `RAIL` — junk resolves to the default
rather than failing a boot), resolved once at boot, carried on the `Harness`, logged as its own
line. **Every reservation and every context cap now keys on the WINDOW, not the rail**, because
what they prevent is arithmetic: a `num_predict` larger than the window evicts the system prompt
mid-generation, whichever corpus produced the prompt. In a small window (≤4096): narratives
reserves 700 not 4,000; vibe/momentum/sigil/the wire wrap reserve 700 not 1,100–1,200; the
Journalist's corpus defaults to 8 articles not 40 (the dropped ones still NAMED through the same
A5 band); the crown's pillar bodies are capped and its card is capped as ONE card, three
storylines sharing the budget with the remainder named.
*The crown cap ships at 700 bytes (~195 tok/card), not §7's ~350 — and the gap IS the diet.* §7
sized that number against a post-7.11 system prompt of ~550 tokens; `or8` is ~1,806 today, so 4096
leaves ~890 tokens for four bodies plus the omen and the prior read. Measured system prompts, for
7.11 to aim at: **or8 1,806 · s13 1,244 · t11 1,238 · Scout 1,370 · n16 1,175 · v16 1,010 tok.**
Output at 4096 will be shorter and blunter than at 16,384. That is understood and accepted —
Scott: *"I'm fine with an imperfect output run over the next few days. We are mostly looking to
get some practice in so we have the context for the voice session."*

**7.14 — DEPLOYED, and the voices are back.** `f256abb` live at 23:28:14 EDT. Boot lines:
`RAIL: the voices read legacy` · `VOICE WINDOW: … num_ctx 4096 pinned=true envelope="small:
reservations ≤700, crown cards capped, journalist corpus 8"` · 11 stages registered (the six voice
stages restored to `COGNITION_STAGES` after the Aug-3 pause) · both Ollama hosts reachable, the
Mac at `max_concurrent=3` (7.12's number, up from 2). **The end-to-end proof: `/api/ps` on the Mac
shows `ministral-3:14b` resident at `context_length 4096`** — the pin travelled from an env var on
archbox to the runner's KV allocation on another machine. Env backed up first to
`/tmp/env.local.bak-7.14-<epoch>` on archbox.

**Also repaired, unrelated but found here:** `examples/` had not compiled since 7.1 added
`Harness.rail` — `cargo clippy --all-targets` was green only because nobody ran it with the flag.
Fixed, and it is green with it now.

**Still not started: 7.7 (Scout's personnel block), 7.11 (the diet), 7.15 (the dry-run).** 7.7 is
plumbing and is genuinely owed; 7.11/7.15 are the junction pass Scott scheduled for the weekend.

**2026-08-06 08:00–08:40 EDT — THE LOOSE-ENDS SESSION: the cutover blocker is dead, 7.7 is in,
6.7 could not be read.** Commits `df7199c` (the NUL fix, deployed 08:10) → `2c6b038` (7.7,
deployed 08:23). 400 tests green (391 → 393 → 400), clippy adds no warning. `RAIL=legacy`,
`VOICE_NUM_CTX=4096`, 11 stages, unchanged across both restarts.

**Item 1 — the Editor's NUL dead-letters, FIXED and CONFIRMED (§2 clause 4 can now go green).**
Measured first, as instructed: `editor` attempts≥5 = **3** (266182 Aug 3, 273432 Aug 4, 278578
Aug 5), `article_read` = 2 (the Jul-26 parse failures that die with the stage). A 0x00 survives
`clean_html` because NUL is not whitespace, so `normalize_space` keeps it, and Postgres cannot
store one in a `text` column at all. **The fix went in one step upstream of the bind** — the body
is sanitised where it ENTERS the Editor, not at `persist_read` — because the same body is also
hashed, prompted, and sliced for candidate evidence (`sweep_candidates`), and that last path
writes text columns too on a best-effort call that would have failed silently. Bodies without a
NUL allocate nothing and pass through byte-identical, so no ordinary prompt or `content_hash`
moved; `fetch.rs` is untouched, so the legacy seat is unchanged. Requeue rehearsed in a
rolled-back transaction (3 rows, class→0 asserted inside), then applied: **all three cleared the
queue and wrote `editor_reads` rows** — 266182/273432/278578, `parser_outcome=parsed`,
1328/1472/1402 words, all three `irrelevant` (the sources are Football Lowdown, Bleeding Cool
News and Publishers Weekly — genuine reads of pages that are not about our entities). **Editor
dead-letters: 3 → 0.** `select count(*) … attempts>=5` is the clause-4 query and it now returns 0.
*Two notes for whoever reads this next:* `chr(0)` cannot even be written into a Postgres query —
`ERROR: null character not permitted` — which is the proposition itself, so verify the fix by
length/prefix, not by searching for the byte. And the fetched page carries no NUL today: `grep -c
$'\x00'` in zsh collapses to an EMPTY pattern and matches every line, which is a measurement trap
worth remembering (`python3 -c "…count(b'\x00')"` is the honest count).

**Item 2 — 7.7, the Scout's personnel block (the last Phase 7 plumbing step).** T4 is untouched:
no packet subscription, no `Voice::Scout`, and every column read is a date, an id resolved to a
name, or the adjudicated `event_type` enum — `reason`, `evidence` and `adjudication_raw` are
never selected, so no prose can reach the seat. Injury/suspension gates stay deferred to D-5.
**The step's premise needed a correction, and the correction is the whole value of the step:** the
memory card ALREADY carries "confirmed moves", and its source `transfer_ground_truth` turns out to
be a view over `transfer_identity_applications` itself — `DISTINCT ON (sport, player_id, team_id)`,
non-reverted only, 180 days, LIMIT 3. So a naive block would have been a duplicate. Four facts
never survive that view, and 7.7 is exactly those: (1) **a team's DEPARTURES** — the view's team
branch matches `new_team_id` alone; (2) the club a player came FROM; (3) a **REVERT** (filtered out
by `reverted_at IS NULL`), which is the correction to the very move the last brief was built
around; (4) the since-last-read anchor that makes any of it new. Proven live on application 34
before it shipped: **the same row reads "signed Yan Diomande from RB Leipzig" to Real Madrid (3468)
and "lost Yan Diomande to Real Madrid" to RB Leipzig (277)** — the second half is invisible to the
memory card. Describe in SQL, derive in code (T2): the query returns columns, `render_personnel_block`
builds the sentences and is unit-tested without a database (7 new tests). Capped at 6 lines with
the overflow NAMED (A5). **Deliberately out of `input_components`/`input_hash`** — the stats rail's
trigger stays the rating snapshot, the identical ruling 7.8 made for the Analyst's storyline
render — and **`RATING_PROMPT_VERSION` stays `s16`**: the system prompt and output contract are
untouched, and s17 is 7.11's, whose bump spends one fleet-wide regen. Bumping here would spend it
on a block most entities do not have. `with_memory` became `with_enrichment` (it gates both side
blocks); eval and the input-version builder still pass `false` and still mint an identical hash,
because neither block is in it.

**Item 3 — 6.7 COULD NOT BE READ: the window was still open.** It closes ~Aug 8 22:08 EDT and
this session ran Aug 6 08:00 — **62 hours early**. The plan's own rule ("the +4-min checkpoint is
not a reading") applies with equal force at +10h, so Phase 6 STAYS OPEN. What this session left
instead is the mechanics: **`scripts/rail-6.7-bands.sh`** (read-only, safe to run any time) emits
all four of 6.7's bands plus the Phase 6 Verify clause, and **prints `INTERIM` vs `READING` in its
own header** by comparing `now()` to the window close, so the Aug-8 session cannot mistake a
health check for the reading. Every count is restricted to `attach_method='auto'`, which is the
clean discriminator against the pre-deploy backfill baseline. **Interim numbers at +10.3h — a
health check, NOT the reading:** storylines opened 926 (FOOTBALL 623 / NFL 300 / NBA 83);
organic attaches 2,251 across 1,467 storylines, mean 1.53, p50 1 / p90 3 / p99 8, **top cluster
19 — IN BAND (15–25:1)**; **98.6% of successful reads attached** (2,251 of 2,282, 31 unattached);
55.3% joined an existing storyline vs 44.7% opened a new one. Packets 0, subscriptions 0,
`pk:` work rows 0 — Phase 6's Verify clause holds. The T3 spot-check fires on its own: storyline
**7477** holds "Leipzig deny agreement" (beIN, OneFootball, Goal, Bavarian Football Works) beside
ESPN's "Real Madrid set to sign Yan Diomande in €135M deal" and "hours away from signing
officially" — **a preserved contradiction, both sides standing**, which is the T3 pass. 7474
(Vinicius→Arsenal, 15) reads as one saga with two headlines that look like D-T13 bleed
(a Brahim Díaz squad-role piece, a generic "Arsenal eyeing a winger"). None of this closes 6.7.

**Item 4 — NOT ACTED ON, it is Scott's call.** The packet branches still have never executed
(packets 0), and the clean de-risk is still D-T14 resolution (b) — a migration making mig 206's
arm 2 subscription-gated like arm 1, THEN compile in shadow with nothing subscribed, and read one
real rendered packet before the flip. This session did not write that migration and did not touch
the compile flag. It remains the biggest unmeasured risk the flip carries.

**Found while verifying item 1, recorded not fixed → D-T17:** article 266182's stored `full_text`
is 42 KB of binary whose first bytes are `1F 8B 08` — the **gzip magic number**. An undecompressed
response was lossily decoded into "text", cleared `ARTICLE_MIN_WORDS` at 1,328 "words", and burned
a model call. That is where its NUL came from. Class size measured immediately: **1 of 19,140
stored bodies**, so it is a genuine one-off and not a fix this session should improvise.

### Handoff (loose ends → Phase 8) — run this BEFORE the cutover session

*(Written 2026-08-05 23:50 EDT with the rail live. Everything here is either a cutover blocker
found by measurement, a Phase 7 step still owed, or a reading that expires if nobody takes it.)*

**WORKED 2026-08-06 08:00–08:40 EDT — items 1 and 2 are CLOSED, item 3 was 62h early, item 4 is
untouched and still Scott's. The live block to run next is the one below this one.** ITEM 1: the
NUL sanitiser shipped @ `df7199c`, the 3 rows were requeued and landed, editor dead-letters 3 → 0.
ITEM 2: 7.7 shipped @ `2c6b038` — and its premise needed correcting, because the memory card's
"confirmed moves" already reads `transfer_ground_truth`, itself a view over
`transfer_identity_applications`; the block earns its place on the four facts that view drops
(team departures, the FROM club, reverts, the since-last-read anchor). ITEM 3: NOT TAKEN — the
window closes ~Aug 8 22:08 EDT; `scripts/rail-6.7-bands.sh` is the reading, and it labels itself
INTERIM until then. ITEM 4: NOT ACTED ON — no migration written, compile flag untouched.

**Resume block for the NEXT session (6.7's close + the D-T14 decision):**

```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail). The rail is DEPLOYED
and healthy: 2c6b038 on archbox, RAIL=legacy, VOICE_NUM_CTX=4096, 11 stages, six voices on
ministral-3:14b at the Mac (4096, 3 concurrent). The Mac runs NO worker — it is the model
host; one archbox release.sh is the whole deploy. Read §0, the STATE line, §2, then the
Phase 7 Log's 2026-08-06 entry. Phase 7's PLUMBING IS DONE (7.7 landed); 7.11 + 7.15 are the
junction pass and are NOT this session.

1. CLOSE PHASE 6 — 6.7, and ONLY after ~Aug 8 22:08 EDT. Run scripts/rail-6.7-bands.sh; it
   prints INTERIM vs READING in its own header by comparing now() to the window close. If it
   says INTERIM you are early — do not close the phase, whatever the numbers look like. It
   emits all four bands (storylines/day/sport, articles-per-storyline with the 15-25:1 band
   verdict, % attached, the 3 biggest with their member headlines) plus Phase 6's Verify
   clause. The hand-inspection is YOURS, not the script's: read the headlines it prints and
   decide whether the top 3 are one saga or a wrong merge, and name the preserved
   contradiction. Interim at +10.3h (a health check, not the reading): top cluster 19 IN BAND,
   98.6% attached, and storyline 7477 already shows a clean T3 contradiction — Leipzig's
   denials standing beside ESPN's "set to sign in €135M deal".

2. THE PACKET BRANCHES HAVE STILL NEVER EXECUTED (packets = 0) — SCOTT'S CALL, ask before
   building. This is the biggest unmeasured risk the flip carries and it CANNOT be de-risked
   by flipping COGNITION_PACKET_COMPILE: D-T14 stands (mig 206 arm 2 fans narratives
   unconditionally, so a compiled packet alternates input_version with the legacy
   article_read writer on one pipeline_work row — the mig-197 churn loop). The clean path is
   D-T14 resolution (b): a migration making arm 2 subscription-gated like arm 1, THEN compile
   in shadow with nothing subscribed, THEN read one real rendered packet before the flip.

3. THEN Phase 8, and only with the 7-day §2 window green and Scott's word. Clause 4's editor
   dead-letter count is 0 as of Aug 6 08:30 — the 7-day clock starts from a clean floor for
   the first time. Watch it: `select count(*) from pipeline_work where stage='editor' and
   attempts>=5`. One arrival resets the window.

PHASE 8 PRECONDITION, DO NOT LOSE IT: seed stage_routing_subscriptions IN THE SAME ACT as the
flip. Under RAIL=packet the Influencer has NO waker until those rows exist (7.6 gated the
Journalist-side vibe enqueue to legacy). Seed, drop mig 175's trigger, flip — one act.

RECORD, DO NOT FIX (the weekend junction pass, PLAN-character-tuning.md): the Analyst's
"VIBE:"/"Vibe:" contract-label miss (players 219665, 33934017); the first sigil at 4096 naming
TWO peers and using "z-scores"/"percentile" in served prose (both or8's own documented
defects, reproducing at the smaller window); D-T16 (the storyline memory lens renders
passing_mention edges); D-T17 (a gzip body reached the model and the column — 1 of 19,140).

LAWS THAT DID NOT CHANGE: no prose reaches the Scout (T4); memory is characters-only and stays
out of input_hash; voice routing is data, never asked of a model; nothing user-visible changes
until Phase 8 flips RAIL.
```

*(The original 2026-08-05 block, worked above, kept verbatim:)*

```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail) — the LOOSE ENDS session,
before Phase 8. The rail is DEPLOYED and running: ac131ca on archbox, RAIL=legacy,
VOICE_NUM_CTX=4096, 11 stages live, six voices on ministral-3:14b at the Mac (context_length
4096, 3 concurrent). Storylines assemble organically (6,382 and growing); packets are still 0
by design. Read §0, the STATE line, §2 (you are protecting its clauses), then the Phase 7 Log.

Do these in order — 1 is a CUTOVER BLOCKER, 2 is owed plumbing, 3 expires:

1. THE EDITOR IS DEAD-LETTERING ON NUL BYTES (blocks §2 clause 4, which requires 0).
   3 rows at attempts>=5, one per day, all the same cause:
     persist article full_text <id>: invalid byte sequence for encoding "UTF8": 0x0
   (articles 266182 Aug 3, 273432 Aug 4, 278578 Aug 5 — plus 2 legacy article_read
   dead-letters from Jul 26 that die with the stage at cutover, ignore those.)
   A 0x00 in scraped body text reaches the INSERT unstripped. Fix it in the editor's
   full_text write path (strip/replace NUL before persist — it is not valid in a Postgres
   text column at all, so this is sanitisation, not policy), pin it with a test on a body
   carrying 0x00, then requeue the 3 dead rows and confirm they land. Measure the class
   first: `select count(*) from pipeline_work where stage='editor' and attempts>=5`.
   Without this the 7-day cutover window can never go green — one arrival per day resets it.

2. 7.7 — THE SCOUT'S PERSONNEL BLOCK (the last Phase 7 plumbing step, never started).
   NO packet subscription for the Scout (T4 — there is no Voice::Scout variant, keep it that
   way). Two confirmed-fact roads only: the stats platform, and a compact "personnel changes
   since last read" block built from transfer_identity_applications applied/adjudicated rows
   (facts with dates, no prose) added to the PEAK context. Injury/suspension gates stay
   DEFERRED to Appendix B D-5 — do not improvise them.

3. 6.7 — CLOSE PHASE 6. The 72h window closes ~Aug 8 22:10 EDT and the reading expires if it
   is not taken: storylines/day/sport, articles-per-storyline, % attached, then hand-inspect
   the 3 biggest for wrong merges and one preserved contradiction. The Phase 6 Log's backfill
   numbers are the PRE-DEPLOY baseline; the +4-min checkpoint is not a reading.

4. THE PACKET BRANCHES HAVE NEVER EXECUTED. packets = 0, COGNITION_PACKET_COMPILE=false, so
   7.5/7.6/7.8's packet paths are tested but never run against real data — that is the
   biggest unmeasured risk the flip carries. It CANNOT be de-risked by simply flipping the
   compile flag: D-T14 still stands (mig 206 arm 2 fans narratives unconditionally, so a
   compiled packet alternates input_version with the legacy article_read writer on one
   pipeline_work row — the mig-197 churn loop). The clean de-risk is D-T14 resolution (b):
   a migration making arm 2 subscription-gated like arm 1, THEN compile in shadow with
   nothing subscribed, and read a real rendered packet before the flip. Scott's call.

RECORD, DO NOT FIX (they are the weekend junction pass, PLAN-character-tuning.md):
 * The Analyst answered 2 items with "VIBE:"/"Vibe:" instead of "READ:" (players 219665,
   33934017; attempts=1, not dead-lettered) — a contract-label miss, momentum-s13.
 * The first sigil at 4096 named TWO peers and used "z-scores"/"percentile" in served prose
   — both are or8's own documented defects, now reproducing live at the smaller window.
 * D-T16: the new storyline memory lens renders passing_mention edges (same root as D-T13).

LAWS THAT DID NOT CHANGE: no prose reaches the Scout (T4); memory is characters-only and
stays out of input_hash; voice routing is data, never asked of a model; nothing user-visible
changes until Phase 8 flips RAIL. Prompts and contracts are the junction pass, not this
session (Scott, 2026-08-05) — item 1 is sanitisation, not voice work.
```

### Handoff (phase 7 → 8)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phase 7 is DEPLOYED @ ac131ca (RAIL=legacy, VOICE_NUM_CTX=4096, all 11 stages live, the six
voices back on ministral at 4096). The whole packet brain is wired and dormant. Read §0, the
STATE line, and ALL of §2 (the cutover condition — you are measuring it), then execute Phase 8.
THREE THINGS PHASE 8 MUST NOT LOSE:
 1. The 7.4 seed of stage_routing_subscriptions is now PART OF THE FLIP, not a prior step
    (Scott: "we don't need to seed anything until the cutover"). Under RAIL=packet the
    Influencer has NO waker until those rows exist — 7.6 gated the Journalist-side enqueue to
    legacy. Seed, drop mig 175's trigger, and flip in ONE act.
 2. Still open in Phase 7: 7.11/7.15 ONLY (the junction pass Scott scheduled for the weekend
    of Aug 8). 7.7 is DONE and deployed @ 2c6b038 (2026-08-06) — the plumbing is complete.
 3. 6.7's 72h bands close ~Aug 8 22:10 EDT — read them and close Phase 6.
Phase 8 does NOT flip anything until the 7-day condition is green and Scott has said "flip" —
the flip is his act, prepared by you. If any clause fails, STOP, log the numbers, emit the
block with BLOCKED.
```

---

## Phase 8 — The cutover

- [x] **8.1** *(Done 2026-08-06 — `scripts/rail-cutover-check.sh`. Clauses 1/2/4/5 print their own
      PASS/FAIL with the numbers; clause 3 emits the day's link sample and reports SAMPLE, never
      PASS, because no query can score a link. Clause 4 covers both halves — dead-letters and the
      editor fixture gate. Run it with `DAY=` on a COMPLETE day: a partial day reads clause 1 as a
      failure that is really just the day being young.)* Write the five §2 condition queries as `scripts/rail-cutover-check.sh` (read-only,
      prints PASS/FAIL per clause with numbers; runnable daily by cron or by hand). Clause SQL
      sketches: (1) `editor_reads` within-24h coverage vs arrivals; (2) legacy corpus
      entity-days LEFT JOIN packet entity-days → 0 missing; (3) emit the day's 50-link audit
      sample as a table for hand/model review, record the score; (4) `eval --task editor
      --fixtures` + dead-letter count on editor/investigator stages; (5) packet-claims
      referential check + exclusions accounting.
- [ ] **8.2** Run it daily for 7 consecutive days; paste each day's output into the Log. Any
      FAIL resets the window (and is a finding — fix, then restart the count).
- [x] **8.3** *(Done 2026-08-06 — and FOLDED INTO 8.6's one act at `sql/prepared/8.6_flip_day.sql`,
      because §2 requires the seed and the trigger drops to land together. Rehearsed on archbox in
      a rolled-back transaction: 3 triggers dropped, 5 subscription rows seeded, the assertion
      block passed, prod verified unchanged after ROLLBACK. **ONE DELIBERATE DEVIATION, flagged in
      the file and to Scott:** mig 197's `enqueue_voices_on_routing_tags` is dropped in the act
      rather than left for Phase 9. This step's reasoning — post-flip it fires into no
      article-grain rows — is true post-flip and false DURING the flip, when `article_read` is
      still draining and the seed has just armed it; for those minutes it would enqueue
      `s:transfer:…` against mig 175's `t:…` on one row, the exact churn loop the act closes.)* Prepare the flip-day migration (do not apply until flip): mig 2xx
      `retire_legacy_rail_triggers` — DROP `enqueue_derive_on_vetted` (T10 dies with it) and
      `enqueue_transfers_if_transfer_related`; leave `enqueue_voices_on_routing_tags`
      (article-grain, subscription table now serves packet grain; trigger fires into no
      article-grain rows — dropped in Phase 9). The Appendix A revert block is the rollback.
- [x] **8.4** *(Done 2026-08-06 @ `5020c15`, DEPLOYED and inert — gated on `RAIL=packet` instead
      of prepared-and-held, so flip day is env + restart with no untested code path meeting
      production for the first time under load. `railIsPacket()` reads the env per call (Go's half
      is two skipped writes; the Rust side still parses RAIL once at boot because there it rides a
      whole drain). Default legacy: RAIL must be exactly `packet`, so an unset or misspelled value
      can never silently retire the old rail. The regex loop is SKIPPED rather than env-flagged —
      `LEGACY_LINKS` was not added, since RAIL already says everything it would.)* Prepare the flip-day Go change (do not deploy until flip): `persistArticles` stops
      enqueueing `scrub` (editor enqueue stays); regex secondary-link loop behind a
      `LEGACY_LINKS=0` env default-off (deletion is Phase 9; off is enough for flip day).
- [x] **8.5** *(Done 2026-08-06 @ `5020c15`, DEPLOYED and inert under `RAIL=legacy` —
      `editor::write_links`. One transaction, so an article is never half-vetted. Irrelevant read
      retracts every vetted row (mirroring the legacy `clear_vetted_entities_for_article`);
      relevant read confirms its resolved links and denies the article's remaining `vetted IS NULL`
      rows — that denial IS the "confirming/denying the 0.95 hypothesis" half, because the
      hypothesis is confirmed exactly when the resolver reached it too. **The 0.90 sentinel applies
      on INSERT only:** an existing 0.95 hypothesis row keeps its 0.95 when confirmed, because
      overwriting it would erase how the article was found — `match_confidence = 0.90` still
      greps the new rail's own links. Ordered BEFORE the graph enqueue, whose candidate set is
      exactly these rows. Verified inert: 0 rows at 0.90 with RAIL=legacy.)* Prepare the Editor's link-writing switch (RAIL=packet side): the Editor begins
      writing `news_article_entities` rows for its resolved links (vetted=TRUE,
      `match_confidence=0.90` sentinel distinct from 0.95/0.8 so Editor links stay greppable),
      confirming/denying the 0.95 hypothesis link per `entity_roles`. Ordering on flip day
      matters: **triggers drop BEFORE the Editor writes vetted** (T10). The Editor's graph
      enqueue (7.13) activates with RAIL=packet in the same deploy — within the first hour,
      verify graph work rows are arriving from the Editor path (the archivist must not starve
      when article_read stops claiming).
- [x] **8.6** *(DONE 2026-08-06 10:55 EDT — **THE OLD RAIL IS OFF.** Mig 213 applied (3 triggers
      dropped, 5 subscriptions seeded, assertions passed), `RAIL=packet` + `COGNITION_STAGES` down
      to 9 stages, released @ `dc3eb3c`. Boot line: `RAIL: the voices read packet
      voices="packets (§1c) via editor::render"`. Verified live within the first hour — see the
      Log. One thing the step did not anticipate and the flip surfaced: Go's `news_scrub`
      MAINTENANCE ticker is legacy machinery too and kept running, so it was gated on RAIL in the
      same act (@ `68619d2` — it would have enqueued `scrub` work no worker claims, and its blind
      confidence-1.0 auto-vet would have raced the Editor's 8.5 verdict).)* **FLIP (Scott's act, one sitting):** apply 8.3 migration → [DEPLOY] Go (8.4) →
      [DEPLOY] rust with RAIL=packet on archbox + Mac, `COGNITION_STAGES` drops
      `article_read` + `scrub` on archbox, voice num_ctx 4096 on Mac → run
      `rail-cutover-check.sh` once more against the live flip → snapshot-schema; commit.
- [ ] **8.7** Watch 48h with point-in-time checks (not watchers): packets/day, narratives/day
      (expect a T7 step change — packets collapse coverage-volume into story-volume; the OLD
      baselines are not comparable, record the new ones), vibe first-voice firings AND total
      vibe volume (the charged gate thins her cadence by design, but momentum's `vibe_slope`
      must not starve — vibe samples down >70% vs legacy is a finding to surface, answered by
      widening the `charged` derivation or a reconcile tick: decided, not drifted), transfers
      packet-work drains, Editor coverage, Investigator funnel, graph enqueue rate (new seam vs
      pre-flip baseline), Mac throughput. Rollback trigger: any voice starving >6h or Editor
      coverage <80% → RAIL=legacy (env flip + Appendix A trigger revert), diagnose cold.

**Verify:** 48h stable on the new rail. **Commit:** `rail: phase 8 — cutover; the old rail is
off`.

### Log (phase 8)
*(executor fills — including all 7 daily condition outputs and the flip-day timeline)*

**2026-08-06 08:45–09:15 EDT — THE PACKET BRANCHES EXECUTED. Scott's direction: "I want to get
the new rail into production. Old rail totally shut down. That way we have several days of actual
production to work with on the weekend."** Commits `94f1bf4` (mig 212) → `2f4d714` (snapshot) →
`bfa3474` (the Desk's own task) → 8.1 → `5020c15` (8.4 + 8.5) → 8.6. Deployed @ `e5dc978`.
`RAIL=legacy` throughout — nothing user-visible has changed yet.

**D-T14 is resolved, path (b), and the flip's biggest unmeasured risk is now measured.** Mig 212
gives mig 206's arm 2 the same subscription gate arm 1 has: `EXISTS` a row with
`stage='narratives'` at that entity_type. The tag column is NOT read — an existence gate, not a
tag join, and explicitly not a wildcard (D-T15's `'*'` trap in reverse). §1c's contract survives
intact: the Journalist still reads every packet, at player/team grain, per active participant.
With the table empty the whole trigger is inert, which is what makes a shadow compile possible.
**Consequence carried into 8.6: the Journalist now needs a seed row like everyone else** — the
flip-day seed is FIVE rows, not the two 7.4 prepared.

**The real reason packets were 0 was not the trigger — it was the Desk's cadence.** Setting
`COGNITION_PACKET_COMPILE=true` and restarting produced nothing, and the diagnosis is worth
keeping: `desk_sweep` ran inside `tick()` immediately after `drain_all`, and `drain_all` drains
every registered stage **to empty**. Measured at the time: 6,096 Editor items pending, draining at
~3/min (35 nomination sweeps in 12 minutes), with 30,222 legacy `article_read` items behind them.
"Empty" was ~34 hours away, so the Desk was never called at all. A cadence that depends on the
queue being empty is not a cadence. The Desk now runs on its own task at 60s (`desk_loop`),
spawned only where the Editor is seated. Safe off the drain because it is DB-only — no model call,
no GPU, no embedder, none of what the 07-15 incident split kept off the supervisor's task — and it
deliberately does NOT beat the drain's `Pulse`, since a Desk heartbeat would mask exactly the
stall the watchdog exists to catch. **First sweep: 200 packets in 0.8 seconds.**

**The shadow compile, measured.** Sustained ~200 packets/min against a 7,214-storyline backlog;
2,000 compiled by 09:10 with 5,218 left. **`pk:` rows in `pipeline_work`: 0, continuously** — mig
212's gate held under real load, and the mig-197 churn loop never started. A packet read by hand
(id 2, storyline 7471, FOOTBALL): headline "Transfer news | Real Madrid make Michael Olise deal
conditional, and Liverpool reject Vinicius offer", 15 attributed claims, 12 active participants
with roles, `slice_fingerprints` carrying distinct vibe/transfers/narratives hashes. It reads as
the product.

**§2 as measured on the day (the first real reading of clauses 1, 2, 3, 5):**

| clause | reading | verdict |
|---|---|---|
| 1 · coverage | 8,132 / 8,358 = **97.3%** (Aug 5, a complete day) | **PASS** |
| 2 · packet presence | 181 / 197 entity-days, 16 missing (Aug 6, partial — compile still draining) | partial |
| 3 · precision | 50-link sample emitted | needs a hand score |
| 4a · dead-letters | **0** on editor/investigate_entity/fixture_boxscore | **PASS** |
| 4b · editor fixtures | **43–47 / 53**, varying between identical runs at temp=0 | **FAIL** |
| 5 · accounting | 4,654 claims checked, **0** orphans | **PASS** |

Two notes the next session must not re-derive. **Clause 1 must be read on a COMPLETE day:** the
same query against Aug 6 at 09:00 says 30.4%, which is not a coverage failure, it is a day that is
nine hours old. **Clause 2 is structurally unmeasurable before today** — it compares legacy
entity-days to packets compiled *that same day*, and no packet existed before 2026-08-06, so every
prior day reads 0/N. Its first honest reading is Aug 7.

**Clause 4b is a model-quality number, and by the standing rule it does not gate the rail.** Every
miss is the Editor's `names[]` channel dropping a person the fixture asserts (Kyle Shanahan, Moyes,
Arteta, Bellingham) plus one `register[outrage]` call, and the gate is not even stable — two
identical `--fixtures` runs at temp=0 scored 47/53 and 43/53. §2 clause 4 asks for 100%, which this
gate has never delivered. Surfaced to Scott as a weekend-tuning item rather than a stop, consistent
with "plumbing gates phases; model quality goes to Appendix D."

**The flip is now one act, and it is prepared.** `sql/prepared/8.6_flip_day.sql` drops the three
legacy triggers and seeds the five subscription rows in ONE transaction, with an assertion block
that aborts if either half no-ops. Rehearsed on archbox against live prod in a rolled-back
transaction — 3 dropped, 5 seeded, assertions passed, prod verified unchanged after ROLLBACK.
8.4 and 8.5 are DEPLOYED and inert behind `RAIL`, deliberately: prepared-and-held code meets
production for the first time during the flip, which is the worst moment to discover it does not
compile. Verified inert — 0 rows at `match_confidence = 0.90` under `RAIL=legacy`.

**2026-08-06 10:55 EDT — THE FLIP. The legacy rail is OFF.** Scott's word: *"compile the backlog
and then flip"*, with the shape stated explicitly — *"the goal is to turn off the legacy rail. Don't
delete it yet until we have finished tuning the new one. But it should be draining zero of our
compute resources."* Deployed @ `dc3eb3c`, then `68619d2` for the maintenance-sweep gate found
minutes later.

**Order actually executed.** Backlog compiled out first (**7,571 packets**, `pk:` rows 0 throughout
— the shadow compile finished before anything flipped). Pre-flip baseline recorded below. Then:
mig 213 applied → `RAIL=packet` + `COGNITION_STAGES` 11 → 9 stages → `release.sh`. The migration's
assertion block passed inside its own transaction: legacy triggers remaining **0**, subscriptions
**5**, packet trigger present **1**.

**The new rail, verified live inside the first hour (8.5's "verify within the first hour"):**

| seam | before the flip | after |
|---|---|---|
| `pk:` work rows — the packet trigger, which had NEVER fired | 0 (ever) | **1,351** |
| `vibe` pending — the Influencer, who had no waker at all | 2 | **120** |
| `transfers` pending — the Insider's packet slice | 0 | **12** |
| `news_article_entities` adjudicated by the Editor | 0 | **64** |
| `narrative_events` — the archivist, which must not starve | 50 / 24h | **15 in 20 min** |
| Editor/Investigator dead-letters | 0 | **0** |

**All four arms of 8.5's `write_links` fired on real data** in the first minutes, which is the
proof the design was right and not merely compiling: **8 rows confirmed at 0.95** (the query
hypothesis, confirmed because the resolver reached it too — keeping its 0.95, not overwritten),
**5 rows INSERTED at 0.90** (links the Editor discovered on its own — the new rail's greppable
inventory), **3 legacy 0.8 regex links confirmed**, **1 denied**. The deny arm working matters
most: it is the half that had no test but the fixture.

**Clause 2 went PASS at the flip: 197/197 entity-days covered, 0 missing.** Every entity the legacy
rail would have built a narratives corpus for that day appears in a packet. It read 181/197 four
hours earlier — the difference is simply the compile backlog draining, exactly as predicted.

**The legacy rail is OFF and costs nothing, and is NOT deleted** (Scott's shape): 0 legacy
triggers; `scrub` work rows **0** and the sweep that would create them now gated; **30,224
`article_read` rows PARKED with 0 touched since the flip** — no worker claims that stage, so they
sit at zero cost, remain the rollback surface for §2's 7 days, and die in Phase 9. Nothing was
dropped that a rollback would need.

**Found and closed during the flip, not before it — the maintenance sweep.** Go's `news_scrub`
ticker (`maintenance.go`, 30-minute cadence) is legacy machinery on BOTH halves and the flip left
it running. Phase 2 enqueues `scrub`, a stage no worker claims any more — a queue that only grows,
which is what a wedged stage looks like the next time someone investigates a real problem. Phase 1
blind-auto-vets `match_confidence >= 1.0` links, which on the packet rail races the Editor's 8.5
verdict and stamps `vetted` on rows nothing read. Gated on `railIsPacket()`, deployed, verified:
scrub rows still 0. **8.4's text named `persistArticles` and the regex loop and did not know about
this third caller** — worth remembering that "the Go change" was two files, not one.

**Pre-flip baseline for 8.7's T7 comparison (24h, the LEGACY numbers — deliberately NOT comparable
after the flip; packets collapse coverage-volume into story-volume):** `news_summaries` 522,
`vibe_scores` 225, `momentum_summaries` 236, `sigil_synthesis` 212, `transfer_rumors` 68.
First 20 minutes on the packet rail: 4 / 1 / 1 / 2 / 0 — too short a window to read as a rate, and
recorded only so the next session knows the clock started here.

**Still open and honest about it:** clause 3's link sample is emitted and UNSCORED; clause 4b (the
editor fixture gate) is FAIL at 43–47/53 and **Scott waived it explicitly for the flip** — logged
as D-T19 with the waiver named, because §2's text asks for 100% and a waiver that lives only in a
chat log is a waiver nobody can audit. Clause 1 reads low on a partial day by construction (41.2%
at flip time, 97.3% on the last complete day) — do not read it before a day closes.

### Handoff (phase 8 → 9)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–8 committed: RAIL=packet is live; the legacy rail is OFF (triggers dropped, scrub +
article_read unscheduled, regex links disabled). 48h stability is in the Phase 8 Log.
Read §0 and Appendix A (the demolition inventory), then execute Phase 9: delete the corpses,
rebaseline what T7 says moved, update the wiki, write the closing handoff. Deletion only —
if removing something changes a passing test's behavior, STOP: that thing was not a corpse.
CAREFUL: 9.5 SPLITS cron-narrative-links.sh (co-mention refresh out; the typed-links/
episodes/seal rollups STAY — they feed every memory card).
```

---

## Phase 9 — Demolition, rebaseline, and the record

- [ ] **9.1** Execute Appendix A top to bottom (Go, Rust, SQL, cron, env). Each bullet is its
      own commit or tight group; `go test ./...` + `cargo test` green after every group. The
      legacy voice-prompt consts (n16/v16/t11/is3/s16/momentum-s13/or8) die here with the RAIL
      scoping — the diet versions are the only prompts left.
- [ ] **9.2** Freeze legacy artifacts as archive: `news_article_readings`,
      `narrative_threads`, old fixture dirs (move `rust/fixtures/article_reader/` →
      `rust/fixtures/_retired_article_reader/`) — COMMENT ON the tables as retired; no drops
      (the archive is the moat).
- [ ] **9.3** Rebaseline (T7): 7-day fresh baselines for narratives/day, card_score
      distribution, momentum enqueue rate, transfer heat volume (player links now Editor-written;
      proximity gates lenient), voice prompt-token distributions at 4096 (the new normal), and
      graph enqueue rate on the Editor seam — recorded in the Log as the new normal. The old
      numbers are history, not targets.
- [ ] **9.4** Wiki updates (scoracle-wiki): Living Database status → shipped-for-people+scores,
      Seeker → Investigator rename note; AI Stage Conventions → stage table gains
      editor/investigate_entity (+ Editor/Investigator contracts, the packet as the narratives
      corpus, VOICE ctx 4096 + the diet prompt versions, two-host topology — which supersedes
      the old serial one-model "limit one" note — and the character-named module convention);
      Characters/Character Contracts → memory-is-characters-only doctrine as shipped;
      Archbox Infrastructure → cron changes; DATA_FLOW.md + RUNBOOK.md in this repo likewise.
      Docs and code disagree → the code wins; make the docs agree.
- [ ] **9.5** Crontab: **SPLIT `cron-narrative-links.sh`** — co-mention refresh
      (`refresh_co_mention_links`) OUT; `refresh_typed_links` / `roll_narrative_episodes` /
      `seal_narrative_threads` STAY (they feed every memory card's relational lines — deleting
      the whole script starves the memory loop). Confirm 02:00 ingest, tier recompute, backups
      unchanged; add `rail-cutover-check.sh` renamed `rail-health-check.sh` weekly.
- [ ] **9.6** Write `HANDOFF-one-rail.md`: what shipped, the new baselines, the open decisions
      (Appendix B leftovers: F4 injury gates, national teams, out-of-scope clubs, the front
      page), and mark this plan **DONE** in the STATE line.

**Commit:** `rail: phase 9 — demolition complete; one rail`.

### Log (phase 9)
*(executor fills)*

---

## Appendix A — Demolition inventory (execute in Phase 9; prepared by recon 2026-07-28)

**Go** (the judging tier; keep the clerk):
- `go/internal/thirdparty/match.go` — delete all EXCEPT `isTeamEntity` (move it beside its
  callers in `news.go`). `SportContextTerms` goes; the `sportTerms` map in `news.go:55` STAYS
  (query builder needs it).
- `news.go`: secondary-link loop `:363-392` (+ its `LEGACY_LINKS` switch from 8.4),
  `articleMatchText :1213`, `posOrNil :548`, entity pool `:149-163, :181-182, :285-305,
  :437-532`, `BackfillTitlePositions :563-635`. Scrub enqueue block (already off since flip).
- `go/cmd/comention-backfill/` — whole directory.
- `funnel.go` MatchRejected counter + tests touching it; `maintenance.go:554-569` dead auto-vet;
  `maintenance.go` scrub backstop sweep + `NEWS_SCRUB_ENABLED` config.
- Tests: `news_test.go` regex cases, `news_live_test.go`.

**Rust:**
- Stage `article_read`: the whole `junctions/article_reader/` module (renamed from
  `junctions/editor/` in 3.0; fetch already extracted to `fetch.rs`) — handler, co-mention
  verdict application, vetted clearing, `derive_relevance`, bucket + routing_tags writers.
  `junctions/editor/` (the character's module) is the survivor.
- Stage `scrub` (`rust/src/scrub.rs`) — vetted is the Editor's fact now.
- `Role::ArticleReader` + `COGNITION_ROUTE_ARTICLE_READER` (env removed on BOTH machines —
  coordinated config change), `Role::all()` shrinks.
- BGE/embedder + `threads` cosine clustering in the narratives path (the novelty gate's
  embedding branch; exact-title dedup mig 196 + URL dedup STAY — they are deterministic).
  `narrative_threads.centroid` stops being written.
- `ARTICLE_READ_*` consts, `ar*` prompt-version namespace (T1 retires with the cache it keyed).
- Eval task `article_reader` (renamed in 3.0; the task name `editor` now belongs to the
  greenfield junction) — unregister it; retire its fixtures dir per 9.2.

**SQL** (migration `2xx_demolition`, after 7-day rollback window):
- DROP trigger `enqueue_voices_on_routing_tags` (article-grain; packet-grain trigger from mig
  206 is the survivor) + function.
- DROP functions `refresh_co_mention_links(...)`, `enqueue_derive_on_vetted()`,
  `enqueue_transfers_if_transfer_related()` (triggers already dropped at 8.3).
- `news_articles.bucket` — stop-write already happened (the greenfield Editor never wrote it);
  column stays, commented retired (archive).
- `news_article_entities.title_pos` — stays as historical data, commented retired.
- Recorded revert for the 8.3 trigger drops (rollback window only — delete this block in 9.1):
  re-CREATE `enqueue_derive_on_vetted` + `enqueue_transfers_if_transfer_related` from
  `sql/schema/schema.sql` @ the pre-flip snapshot commit.

**Cron/env:** `cron-narrative-links.sh` SPLIT, not removed (9.5): co-mention refresh out, the
typed-links/episodes/seal rollups stay — they feed every memory card. `NEWS_SCRUB_ENABLED`,
`LEGACY_LINKS`, legacy route envs removed from both machines' unit env files.

---

## Appendix B — Decisions Scott owns (defaults act if he is silent; D-1 and D-4 are closed)

- **D-1 · Voice model tag — CLOSED by Scott, 2026-07-29. The model assignment is now doctrine,
  not a default.** Two engines, one per organ, and nothing else is routed:
  - **`ministral-3:14b`** (13.9B, Q4_K_M, 9.08 GB) on the **Mac** — **all character work except
    the Editor and the Investigator**: Journalist, Influencer, Insider, Scout, Analyst, Oracle.
    Phase 7.12 routes these six. (Verified loaded on the Mac at Phase 0 check time.)
  - **`gemma3:4b`** on **archbox** — the Editor and the Investigator (Scott's "seeker"; see the
    naming ruling at the top of this plan — the character is THE INVESTIGATOR, `investigator/`
    in code), **pinned**. Not a preference: archbox's Ollama runs `OLLAMA_KEEP_ALIVE=-1` with
    `OLLAMA_MAX_LOADED_MODELS=1`, so Gemma is resident forever and a *second* model on that card
    would **evict** it. See §3 and Log 0.11.
  - **`mistral-nemo:12b` is not used.** It stays installed, unrouted. Scott's ruling ("Nemo is
    there but not used") stands; the location is the **Mac**, not archbox — Nemo is not installed
    on archbox at all (archbox's unused spares are `qwen3:8b` and `mistral:7b`). Recorded so a
    future executor does not go looking for it on the wrong box.

  This supersedes the earlier "Mistral 3:12b"/12B reading. Routes are still config — a tag change
  is an env edit — but any *model change* re-earns its seat on the voice fixtures (AI Stage
  Conventions), and `ministral-3:14b` is what the Phase 7.11 fixtures get captured against (the
  voice diet and the model re-earn are ONE gate, deliberately).
- **D-2 · Person kinds v1 — updated by Scott, 2026-08-01:** player, coach, executive, owner,
  agent, official (auto-writable); family exists in the enum, never auto-written. `player`
  covers story-relevant players OUTSIDE the stats platform — rookies pre-debut, retired
  legends, foreign-league players. The `players` table stays box-score-owned: the Investigator
  NEVER auto-inserts `players` rows; if a person-kind player later appears in a box score, it
  reconciles the two identities by alias/external id (5.5).
- **D-3 · Out-of-scope clubs + national teams** default: census only (`rejected_out_of_scope`),
  no auto-writes. When Scott widens the boundary, `teams.kind` (club|national) is a two-minute
  migration — deferred deliberately, since a column nothing writes is scar tissue. The boundary
  is a business decision, not a resolver decision.
- **D-4 · Box-score pilot sport — CLOSED by Scott, 2026-08-01: FOOTBALL** (78% of volume; the
  loop gets exercised hard and fast). The source FAMILY stays an in-phase decision: chosen in
  4.3 after the terms/robots review, url_template mode (surgical target URLs). NBA is the
  fallback only if every FOOTBALL family fails review.
- **D-5 · Injury/suspension confirmation gates (F4 pattern)** default: deferred post-cutover;
  the Scout reads stats + transfer confirmations until then. (Scott flagged interest —
  schedule as its own mini-plan after Phase 9.)
- **D-6 · The front page.** Audited out of the critical path: the packets are the compiled
  stories of the day, and a ranking call with no client surface is decoration (a junction that
  only decorates is noise — AI Stage Conventions; this is a product, not a junction, but the
  rule's spirit applies). When Scott picks a surface, the package is a `front_pages` table +
  one day-close pick-by-number call per sport (`Role::Editor`, D3 closed-list discipline) —
  about a half-day of work sitting behind the decision.

---

## Appendix C — What this plan deliberately does not do

- No packet backfill over the 150,566-article archive (forward-only; the archive keeps its old
  readings — the lazy-invalidation finding says they were never coming back anyway).
- No BGE revival, no semantic-similarity identity authority, anywhere (T9, write-gate law).
- No browser automation in Investigator v1 (direct fetch + APIs; a blocked domain is a skipped
  domain).
- No player/person RSS sweeps (teams carry the sport's story surface; discovery finds the
  people).
- No new public reading junction: the Editor and the Investigator are supporting characters —
  the six-voice cast and the one-card-per-character surface are untouched (Characters doctrine).
- No schedule scraping in v1 — completed games enter by news demand (`result_line`). Thinly
  covered fixtures can be missed; provider-era `fixtures` rows stuck `scheduled` are the running
  census of what demand alone doesn't catch, and the signal for a v2 schedule source.
- No renames of live legacy identifiers (F1's lesson, §4 ruling) — files take character names;
  stage strings, env keys, prompt versions, and tables keep theirs until demolition.
- No memory for the Editor or the Investigator — statelessness is the objectivity guarantee
  (a fixed contract, frozen evals, and deterministic gates, never hidden memory). Memory
  belongs to the six voices alone; the Oracle's is its own verdict trail, nothing else.
- No LLM number-reading anywhere on the stats road — box-score numbers enter rows through
  DOM/JSON parsers only; a model may describe or triage a page, never transcribe its digits.

---

## Appendix D — The tuning ledger (junction knobs; follow-up, NEVER phase gates)

*(Founded by Scott's 2026-08-03 ruling — see §4. Model-junction quality items are appended
here with their measured baseline and revisited as tuning passes, typically post-cutover or
in idle capacity. A phase may cite this ledger; it may not halt on it.)*

*(Convention added by Scott's 2026-08-05 ruling: this ledger is the INDEX — one D-T number +
one-line measured baseline per finding. The diagnosis detail — numbers, code pointers,
candidate knobs, the measurement that settles each knob — lives in
**`PLAN-character-tuning.md`**, the session notes for the post-rail Character tuning
sessions. Rail sessions append to both as findings surface; they fix nothing mid-rail.)*

- **D-T1 · names[] under-fill (the discovery miss class).** Every genuine bleed-class miss
  on day 1 was the model omitting the player from `names[]` (clubs/agents emitted, player
  skipped — "Yan Diomande's dream Real Madrid transfer…" → Jay-Z, two agents, no Yan).
  Baselines, same-yardstick title-mention: legacy Vinicius 54.9% / Olise 57.4% / Diomande
  49.4%; editor day-1 Vinicius **60.0%**, Diomande 66.7% per-success (38.5% raw). **The
  137-article corpus replay gives the paired per-article verdict — record it here when
  read.** *(Checked 2026-08-03 18:19 EDT by the Phase 4 session, which fired before the
  drain: editor queue still 1,184 pending — untouched since the 18:00 rest-window pause;
  worker resumes 19:00; drain ETA ~23:00 EDT holds. The enqueue-time "~20:10" stamp was
  UTC mislabeled EDT. Measure in the next session: replay rows = editor_reads on articles
  with fetched_at < Aug 3 02:00, minus the 15 NBA smoke rows; per-name 2×2 vs
  news_article_entities, ids Vinicius 600687 / Olise 24799984 / Diomande 37922937.)*
  **REPLAY VERDICT (measured 2026-08-03 ~22:25 EDT, drain complete — 152 pre-Aug-3 reads
  = 137 replay + 15 NBA smoke; name sets = token match on `nrm(title)`, one definition
  on BOTH sides of each pair; the enqueue's substring pattern had caught false positives
  — "Oliseh"/"idolises"/"Coliseum" — which the token match excludes):** per successful
  read the Editor beats legacy on ALL THREE names, on the same articles — Vinicius
  **85.0%** vs 40.0% (n=40; editor-only 20, legacy-only 2), Diomande **81.8%** vs 56.8%
  (n=44; 11 vs 0), Olise **83.3%** vs 62.5% (n=24; 6 vs 1); combined 90/108 = **83.3%**
  vs 51.9%. Raw (all statuses): Vinicius 64.2% and Diomande 67.9% beat both the paired
  legacy raw (45.3/56.6) and the 7-day baselines; Olise raw 55.6% trails its paired
  legacy 61.1% purely on fetch decay — 9 of 36 week-old Google News URLs produced no
  read (6 blocked / 2 fetch_failed / 1 parse_failed) where legacy had linked at ingest
  without needing the fetch (the recipe's stale-URL caveat, observed; organic articles
  fetch fresh). Under-fill remains the miss class but is smaller than day-1 raw implied:
  18 of 108 successful name-reads missed the player (16.7%). **The plan's written bar —
  "beats legacy per name on the same yardstick" — is MET wherever a read happens.**
  Candidate knobs: quoted-people-style
  re-scan for title principals; interplay with the 2-mention floor; the Investigator
  refusal/nomination backstop (Phase 5) which structurally catches what the prompt drops.
- **D-T2 · register `outrage` reads neutral under phrase-first order.** Known C2 cost,
  Scott declined the reorder 2026-08-01. Revisit only with a measured fixture set.
- **D-T3 · parse_failed 2.6% vs legacy 0.1%** (day-1: 137 of 5,328). Diagnose a sample:
  format_schema violations vs truncation (num_predict 900 interplay, D-T4).
- **D-T4 · editor call cost: num_predict 900→750.** Call wall p50 33.4s / p95 56.2s /
  avg 34.1s (~10 tok/s/slot at 4-parallel); editor demand ≈ 49 of the 64 daily slot-hours.
  The knob trades tail-truncation risk (watch D-T3) against ~15% card time.
- **D-T5 · descriptor leakage.** One live read emitted descriptor "team 277" (an internal
  id, not text-derived) — contract says descriptors copy the text. Count instances before
  caring.
- **D-T6 · enrichment refusals are log-only.** An `investigate_entity` player item that
  refuses (ambiguous/insufficient) leaves no durable trace — the review surface cannot
  count them. Candidate: a census row or a `players.meta` note on refusal.
- **D-T7 · initials in nrm().** "A.J. Green" folds to `a j green`, Wikidata's "AJ Green"
  to `aj green` — no agreement, honest refusal, missed enrichment. Measure the class size
  across the roster before touching the one normalizer (mig 198 caution applies doubly).
- **D-T8 · legal-name vs known-name mismatches.** Our "Airious Bailey" vs Wikidata's "Ace
  Bailey" refuses at the name screen. The designed answer is 5.4's deferred prose arm
  (Wikipedia search + gemma describe) — build it when this class proves big enough.
- **D-T9 · THE META-GATHERING RUN IS PARKED (Scott, 2026-08-03 ~21:30 EDT: "the plumbing
  is in; park the meta gathering as part of the follow-up plan").** The vetted machinery
  stands ready; what remains is operational, not construction: (1) the FULL NBA seed
  (~603 active-tier players with gaps — commented block in
  `scripts/investigator-vetting-seed.sql`), (2) the 5.9 [DEPLOY] that lets the live
  service drain it, (3) the 5.8 20-row hand-check + 5.10 72h readings, (4) widening to
  FOOTBALL rosters when that season starts. Run as its own follow-up session(s) on
  Scott's go.
- **D-T10 · investigator starvation under the daily ingest batch (measured 2026-08-04,
  5.10 interim).** The facts: ingest is one ~7–9k-article batch at 02:00 EDT daily; the
  Editor needs ~19h of wall (incl. rest pauses) to digest it, so shared-slot card idle
  is ≤1h/day; the investigator (max_in_flight 1, registered after the Editor) logged 0
  acquisition_runs in its first 16.6h against 3,462 enqueued items. Day-1 nominations
  ran 58× the B3 estimate (3,473 person candidates — the descriptor rule fires on
  ~100% of person names; the 2-mention floor is near-dead letter), though day 1 flushes
  the standing corpus and steady-state must be read off day 2–3. Even fully unblocked,
  the 4.2 budget (2s Wikimedia spacing, ~2 fetches/candidate) caps drain at ~900/day —
  the queue grows either way if steady-state nominations stay in the thousands.
  Candidate knobs, in rough order of leverage (MEASURE day-2/3 nomination rate at the
  5.10 close first; decide nothing before then): (a) the v1 investigator makes ZERO
  model calls — holding an ARCHBOX_GEMMA_SLOTS card slot for pure HTTP work is the
  structural mismatch; a separate slot group (or slotless claim) frees it from editor
  contention without costing the card anything; (b) tighten the 5.2 enqueue rule if
  steady-state volume stays high (descriptor-on-first-sight currently admits
  everything); (c) run the investigator through the GPU rest windows (the card rests;
  HTTP doesn't need it) — interacts with the pause-timer design; (d) raise
  max_in_flight only after (a), else it just deepens the same contention.
  **Day-2 verdict (2026-08-05): the design works, the arithmetic doesn't.** It caught
  exactly the predicted idle — 70 runs in the 01:52–02:00 EDT window before the daily
  batch re-buried the card; decisions honest (8 accepted / 23 ambiguous / 20 not_sport /
  19 insufficient = 11.4% acceptance); but steady-state nominations are ~3k persons/day
  (day-2 pace matched day-1 — NOT a corpus flush), queue +~2.7k/day vs ~70/day drain
  (6,670 pending). The compounding upside is real: the 8 overnight accepts drew 102
  resolver links onto persons the same day (Alonso 59, Iraola 23). Diagnosis + knobs in
  PLAN-character-tuning.md §3.
- **D-T11 · editor input hygiene (measured 2026-08-04/05, 4,774 ledgered calls).**
  `clean_html` keeps site chrome (nav/footer text); 34.3% of prompts hit the 9,000-char
  cap — sports.yahoo.com (top domain, 584 calls) 95% at cap with its "Article text"
  opening on the full nav menu, so real prose truncates off the tail (feeds D-T1's
  under-fill). `decode_entities` misses hex `&#x27;`. Knobs: article-element/boilerplate
  strip before truncation + numeric-entity decode. Diagnosis in
  PLAN-character-tuning.md §1.
- **D-T12 · editor capacity fully subscribed; output tokens dominate the wall (same
  sample).** Wall scales 16.9s→38.8s across prompt buckets but ~19s of the ~22s delta is
  extra OUTPUT (~14 tok/s/slot at 4-parallel; only ~3s prompt eval) — num_predict trims
  tails only; a real cut is an ep2 envelope bump (reopens all work; never casual).
  Capacity ~7,800 reads/day (490/hr active; rest windows pause 8h/day) vs arrivals grown
  to ~8,000–8,400/day; concurrency verified real (4×4, 100% GPU, 77% slot utilization);
  within-24h coverage still 100.0% — zero headroom for growth. Rest-window and
  model-swap knobs are Scott's calls. Diagnosis in PLAN-character-tuning.md §2.
- **D-T13 · adjacent sagas bleed through a shared seed slice (measured 2026-08-05, the 6.2
  backfill).** After the two 6.1 corrections the assembly is sound — the top cluster is one
  real saga with its contradictions intact — but storyline #7477 (Diomande → Real Madrid,
  109 members) carries ~10 Vinicius→Arsenal articles: its seed article named Vinicius in
  passing, so two distinct Real Madrid sagas share a 2-of-4 slice of the seed key, which
  clears both `covers_seed()` and the score. Class size unmeasured beyond this hand
  inspection. Candidate knobs, none touched: (a) seed the key from ep1 `entity_roles`
  (subject/opponent only) once role coverage is real — today roles exist only for
  hypothesis entities; (b) weight a seed entity by whether later members keep naming it (a
  principal recurs, a passing mention does not); (c) require the shared slice to include
  the storyline's dominant PERSON. Measure the bleed rate over a live 72h window (6.7)
  before choosing.
- **D-T14 · the packet fan-out seam under RAIL=legacy (SCOTT'S RULING, 2026-08-05: "leave
  all the actual model testing until we've completed the rail; mark this as part of the
  tuning — we're going through each junction and tuning, this is an issue for that session.
  We're building the rail first").** Mig 206's arm 2 (the Journalist's `narratives`
  fan-out) is unconditional by design, so a compiled packet stamps `pk:<fingerprint>` onto
  the same `(stage, entity_type, entity_id, sport)` rows the legacy `article_read` seat
  stamps `n:<hash>` (`article_reader/mod.rs:1319`); `work::enqueue` reopens on any version
  change, so the two writers would alternate forever — the mig-197 churn loop. Cost is
  bounded even if it fires (voices paused; the Journalist debounces on its own material
  hash, so a claimed row costs a corpus read, not a generation). **Held, not fixed:**
  `COGNITION_PACKET_COMPILE` defaults OFF and is logged at boot (`packet_compile=false`);
  zero packets have ever been compiled, so the trigger has never fired. The rail is built
  around it — Phase 6 ships the storyline half live, Phase 7 lands `RAIL` and seeds
  subscriptions, and the flag flips when this seam is the session's actual subject.
  Candidate resolutions for that session: (a) flip the flag once RAIL exists (the default
  path); (b) mig 211 makes arm 2 subscription-gated like arm 1, with 7.4 seeding
  `narratives`. Phase 6's Verify line ("packet trigger fired 0 work rows") is TRUE as
  shipped — because nothing compiles, not because the trigger is inert.
- **D-T15 · seeding `stage_routing_subscriptions` arms a LIVE article-grain trigger too
  (measured 2026-08-05 ~22:50 EDT, archbox — the 7.4 blocker).** The table is read by TWO
  triggers, and 7.4's text accounts for one. Mig 206's `enqueue_voices_on_packet` is inert
  (0 packets, compile off — D-T14); mig 197's **`enqueue_voices_on_routing_tags` is LIVE**
  (`tgenabled='O'` on `news_articles`, fired by `article_reader`'s routing_tags write on
  every legacy read — 37,590 articles carry tags, newest Aug 4 02:05). So the moment
  `('transfer','transfers','team')` exists, mig 197 begins enqueueing
  `(transfers, team, id, sport)` as `s:transfer:<count>:<md5>` against mig 175's still-live
  `t:<count>:<md5>` on the SAME `pipeline_work` key (all 132 live transfers rows are `t:`
  today) — two writers, one row, `ON CONFLICT` reopening on every alternation: the mig-197
  churn loop, on the LEGACY rail, on a production stage. **Also found: `'*'` is not a
  wildcard.** Both triggers join `entity_type` on strict equality (mig 206 line 52, mig 197
  line 116), so 7.4's `('charged','vibe','*')` would fan out to nobody and the Influencer
  would silently never wake. The seed is written, corrected to two rows (player + team —
  the grains `pipeline_work`'s CHECK admits), and **deliberately unapplied** at
  `sql/prepared/7.4_seed_packet_subscriptions.sql`, with its preconditions in the header.
  Resolutions, Scott's call: (a) Phase 8 drops mig 175's trigger, leaving one writer;
  (b) a migration narrows mig 197's trigger so the packet trigger is the only reader;
  (c) **the cheap partial — seed only the two `charged`/`vibe` rows now** and hold the
  transfers row until (a); `vibe` has no competing article-grain enqueue, so it carries no
  churn risk, and it is what 7.6 actually needs.
  **CLOSED 2026-08-05 by Scott: "we don't need to seed anything until the cutover. Which
  will be soon."** No rows are seeded, so mig 197 is never armed and neither churn loop can
  start — the resolution is (d), do nothing, which none of the three offered options was.
  The seed moves into Phase 8's single act, where mig 175 is dropped in the same session and
  there is only ever one writer. Carried forward as a PHASE 8 PRECONDITION: 7.6 gated the
  Journalist-side vibe enqueue to the legacy rail, so under `RAIL=packet` the Influencer has
  no waker at all until these rows exist. Seed and flip together.
- **D-T16 · the storyline memory lens renders passing mentions (observed 2026-08-05, mig 211
  rehearsal).** The first cards read back carry lines like `this entity's part:
  passing_mention` on storylines the entity is barely in — one sampled person sat in a
  Dodgers/Skubal trade saga on a single passing mention. It is the same root as D-T13 (a seed
  slice shared by adjacent sagas), now visible in MEMORY rather than in assembly, which makes
  it worse: a weak edge that only cost a packet slot before now also occupies a voice's
  memory card. Class size unmeasured. Candidate knobs: restrict the lens to subject/opponent
  roles once ep1 role coverage is real (D-T13's knob (a) fixes both at once); or rank by
  report count and keep the top 1–2. Measure alongside D-T13's bleed rate, not before.
- **D-T17 · a gzip body reached the model and the column (found 2026-08-06, measured 1/19,140).**
  Article 266182's stored `full_text` is 42 KB whose first bytes are `1F 8B 08` — a gzip stream
  that reqwest's `resp.text()` decoded lossily instead of decompressing. It cleared
  `ARTICLE_MIN_WORDS` at 1,328 "words", spent an editor call, and carried the 0x00 that
  dead-lettered the row until the NUL sanitiser landed. **Class size: 1 of 19,140 stored bodies**
  (`left(full_text,1) = chr(31)`), so this is a one-off, not a pattern — recorded because the
  next NUL report will look identical and this is where it comes from. Candidate knobs: ask for
  identity encoding / honour `Content-Encoding` in `fetch.rs`; or a cheap pre-model sanity gate
  (a body whose decode produced a high replacement-char ratio is not prose). Measure the class
  again before spending anything on it — at 1 in 19,140 the gate may cost more than the miss.
- **D-T18 · syndicated near-duplicates put the same fact in a packet twice (found 2026-08-06,
  the first shadow compile).** Packet 2 (storyline 7471) carries "Celtic have wrapped up an 11
  million pound deal for Kasper Hoog" AND "Celtic have signed Kasper Hoog"; "Bayern Munich's
  sporting director denied rumours linking Michael Olise with a move to Real Madrid" AND "Bayern
  Munich denies Michael Olise will be leaving" — 15 claims that are closer to 8 facts. The cause
  is not the compiler: its two members are articles 186800 and 186793, both Goal.com transfer
  roundups of the same hour, correctly clustered. The packet faithfully carries both sources, and
  that is the right default (T3 — two outlets saying a thing is evidence, and suppressing a
  restatement is how a contradiction gets silently dropped). But two lanes of ONE outlet is not
  corroboration, it is syndication, and it spends the 2,000-token render budget twice. Class size
  unmeasured. Candidate knobs: collapse claims sharing a source AND a high text similarity, keeping
  the longer; or prefer one member per (source, hour) at assembly. **Measure how much of the render
  budget it actually costs before spending anything** — on a packet with 3 members it is noise, and
  the exact-title dedup sweep already catches the byte-identical case.
- **D-T19 · the editor fixture gate is not stable, and §2 asks it for 100% (measured 2026-08-06).**
  Two consecutive `eval --task editor --fixtures` runs at temp=0 scored 47/53 and 43/53 — same
  binary, same fixtures, same model. Every miss is one of two shapes: `names[]` omitting a person
  the fixture asserts (Kyle Shanahan, Moyes, Arteta, Bellingham — the coach/manager class above
  all), or `register` reading `neutral` where the fixture says `outrage`. The names misses are the
  documented honesty gap on `target` fixtures, not a harness fault. The live consequence is §2
  clause 4, which asks this gate for 100% and has never had it. **Recorded as a tuning item, NOT a
  phase gate** (the standing rule: plumbing gates phases, model quality goes here) — but §2's text
  says otherwise, so the cutover session must either score it green, or Scott waives clause 4b
  explicitly. Do not let it be waived silently. Candidate knobs: the ep1 prompt's names[] ask is
  where the coach class is being lost; and temp=0 not being deterministic is itself worth one
  measurement before tuning anything on top of it.

# September 20 corpus: churn and quality-debugging handoff

Observed September 20, 2026, 09:33–09:36 America/Detroit. This document is the starting context for a fresh quality session. Live work continues; counts below are observations, not a frozen final tally.

## User intent and immediate scope

The user wants today's entire stored sweep corpus processed across NBA, NFL, and FOOTBALL, then close attention to harness and DuckDB context quality. Preserve some unstarted Rating work as evaluation material. Maintain clean ownership: PostgreSQL durable state/queue/publication, DuckDB bounded analytical studies, Studio model creation from prepared assignments, application adapters retrieval/routing/readiness/publication. Do not restart architecture migration or blindly regenerate the entire corpus.

Production access and deployment were explicitly authorized. SSH `archbox` works. This local machine is the Mac mini; its separate worker checkout is `/Users/scotty/scoracle-worker`. No new task was created by this handoff.

## Where the churn stands

Both machines run cognition revision `8100c9801151`. Archbox PID 383673, zero automatic restarts; Mac PID 24996 at observation. Both have all nine stages enabled, share the same PostgreSQL queue, and listen on `pipeline_work_ready`. They run granite4.2:3b on their respective localhost model hosts, configured concurrency Archbox 4 / Mac 6, context window 4,096, handler timeout 1,200 seconds, stale lease 1,800 seconds. Per-process model limits are not distributed limits if multiple workers later share one model host.

The outbox backlog remains **zero**. This is ongoing processing, not another stalled handoff. Since the earlier 08:17 check, today's article result records increased from 1,483 to about 1,744 at 09:33 (roughly 261 additional dispositions in 76 minutes). A disposition is not necessarily a successful substantive reading. No reliable end-to-end completion ETA is established; thousands of reads and downstream products remain, and completions create more work.

Today's stored article scope at 09:33:

| Sport | Stored today | Marked duplicate | Editor result exists | Pending | Running | Failed queue row |
|---|---:|---:|---:|---:|---:|---:|
| FOOTBALL | 2,772 | 156 | 1,026 | 1,621 | 2 | 5 |
| NBA | 889 | 122 | 329 | 472 | 1 | 0 |
| NFL | 2,065 | 269 | 389 | 1,434 | 1 | 1 |

These columns overlap: duplicate markers, results and work are not an exclusive partition. The previous admission check verified zero stored-today, nonduplicate, unread articles without an Editor obligation. The scope is September 20 00:00 through September 21 00:00 EDT. RSS items dropped before persistence are not included.

Whole-queue snapshot at about 09:36 (includes older work, not solely today's arrivals):

| Stage | Pending | Running | Failed |
|---|---:|---:|---:|
| Editor | 3,516 | 6 | 222 |
| Graph | 0 | 1 | 0 |
| Investigator | 0 | 0 | 3 |
| Journalist / narratives | 1,037 | 3 | 0 |
| Influencer / vibe | 1,298 | 2 | 32 |
| Scout / rating | 1,548 | 1 | 14 |
| Insider / transfers | 71 | 2 | 899 |
| Analyst / momentum | 289 | 2 | 2 |
| Oracle / sigil | 634 | 3 | 26 |

Many failed rows predate today. Do not present these as today's failure rates or reset all of them. Newly observed guard failures may be ordinary retryable attempts, not terminal losses.

Published rows since shared-worker recovery began at 07:42:50 EDT, observed around 09:34:

| Sport | Rating | Momentum | Oracle |
|---|---:|---:|---:|
| FOOTBALL | 56 | 91 | 158 |
| NBA | 100 | 56 | 50 |
| NFL | 73 | 48 | 58 |

These are row counts, not necessarily distinct entities, successful prose counts, or attribution to today's articles.

## Rating evaluation reserve — action already taken

**15 pending player Rating jobs are held until September 27 at 09:33:48.526364 EDT**, or explicit earlier release. Five per sport were selected from currently ready pending jobs. No ready team Rating rows existed at selection, so no teams were held. Existing completed team cards can provide the team baseline. The rest of Rating continues; it was not globally paused.

- NBA: player IDs **4, 8, 9, 18, 22**.
- NFL: **11, 12, 13, 14, 17**.
- FOOTBALL: **38, 268, 323, 390, 510**.

Only `available_at` changed. Attempts, input versions, products, and active claims were preserved. The hold deliberately keeps dependent Analyst/Oracle work for these entities waiting. Normal source arrivals can supersede revisions, so verify the manifest before evaluation/release; the hold reserves work, not a frozen live database.

The exact original queue rows and expiry are in [rating-hold.csv](quality-2026-09-20/rating-hold.csv). [release-rating-hold.sql](quality-2026-09-20/release-rating-hold.sql) releases only matching pending rows with the same revision and hold timestamp, and reports changed revisions for review. It has **not been run**. Queue update triggers provide normal wake-ups when work becomes ready.

A repeatable-read export retained stats across stored seasons, cohort rows, and the latest Rating product for all 15 held players: Archbox `/mnt/data/backup/scoracle/releases/quality-handoff-20260920/held-context.jsonl` (15 JSON lines, about 1.29 MB). All 15 have prior Rating output, allowing before/after comparison. FOOTBALL player 390 has one stats row and **no cohort rows**—a useful missing-context control, not yet a proven defect. NFL player 17 has five stats rows but one cohort row; investigate eligibility/competition coverage before calling this missing data. A second retained copy is `/Users/scotty/scoracle-worker/releases/quality-handoff-20260920/held-context.jsonl`, SHA-256 `c1a34f6136d2512547ecfb29792a9b5e05889a8be1f8c6cc960eba18518763d2`. This export is NOT yet the full prepared assignment, exact prompt, or complete two-season peer population needed to recompute a cohort.

## What the harness actually gets from DuckDB

The model-facing Rust harness does **not call DuckDB directly**. Application evidence adapters query PostgreSQL's `analytics_entity_context`, populated by the bounded analytics snapshot producer, and turn those rows into sourced memory. Only Scout and Analyst load this cohort block in `rust/src/evidence/memories/sources.rs`.

Current-season cohort projection at observation:

| Sport / season | Players | Teams | Projection observation time (EDT) |
|---|---:|---:|---|
| NBA / 2025 | 253 | 30 | September 19, 23:47 / 23:46 |
| NFL / 2026 | 921 | 32 | September 17, 08:13 |
| FOOTBALL / 2026 | 339 | 20 | September 17, 08:13 |

`analytics_cohort_publication` contains **only two receipts**, NBA/2025/player and NBA/2025/team, formula `cohort-context-v1`. Those are last night's validated canaries. NFL and FOOTBALL projections predate the new receipt contract. Their age and missing receipt are provenance/freshness gaps; they do not by themselves prove incorrect values. There is no installed automatic cohort snapshot schedule, general DuckDB cutover, or new request-time DuckDB dependency.

The legacy/default analytical provider remains PostgreSQL. Existing rating computations and numeric Momentum maintenance remain SQL-owned. Do not assume every number in a Rating assignment came from DuckDB. NBA season 2025 versus NFL/FOOTBALL season 2026 is the configured `sports.current_season`, not automatically a bug.

Important context seams to inspect:

1. `go/internal/analytics/` and `go/cmd/analytics-snapshot/main.go`: bounded export, Postgres reference, DuckDB calculation, parity, complete-scope publication with source revision fencing. Shadow is default; publication is explicit. `as_of` is an observation label, not historical reconstruction.
2. `rust/src/evidence/memories/cohort.sql`: seasons <= requested season, latest five **rows**, competition join, rounded model-facing values, NULL stripping, computed timestamp. Check whether multiple competitions consume that five-row budget, how competition changes are represented, and whether missing current context can make older context look current. These are investigation targets, not established bugs.
3. `rust/src/evidence/memories/sources.rs`: cohort becomes present evidence or established history; explicit warning that delta percentile is movement rather than ability, and carries no playing-time/fitness/tactical cause.
4. `rust/src/evidence/memories.rs`: byte budgeting, whole-group omissions, rendering and `current_snapshot_view`. A row in PostgreSQL does not prove that the model saw it.
5. `rust/src/application/scout.rs`: profile selection, exclusions, cross-season compatibility, memory filtering, trajectory loading, final assignment and fingerprint. `with_enrichment=false` during queue production still loads core memories; optional enrichment changes actual execution material. Capture both queue revision and final assignment input components rather than assuming identity.
6. `rust/src/studio/scout/` and `rust/src/studio/session.rs`: prompt, generation limits, bounded correction, deterministic arithmetic/band guards, raw response and final verdict.
7. `rust/src/application/analyst.rs`: which completed Rating/Vibe cards and memories become Analyst material. Oracle receives prepared pillar products, rather than the same direct cohort block.

## Concrete quality evidence already available

Today's Editor outcomes include successful readings but also many irrelevant, blocked, empty-body and fetch-failed dispositions. Around 09:34, FOOTBALL had 489 success / 322 irrelevant / 91 blocked / 68 empty / 54 fetch-failed; NBA 198 / 39 / 31 / 43 / 17; NFL 259 / 75 / 24 / 10 / 19. Counts are changing. Inspect source/domain distribution and several examples before judging acquisition or classifier quality. A blocked publisher is not automatically a harness defect.

Six stored-today Editor jobs exhausted retries: four prompts exceeded 4,096 context (observed 4,108–4,787 tokens), two returned incomplete output. FOOTBALL article **724370** reproduces the 4,787-token error. Do not merely raise context fleet-wide; inspect actual final prompt, token accounting, reserved output, article truncation and model-server configuration on both machines.

Observed retryable Rating failures, with concrete sport-qualified player IDs:

- NBA **73**, **17896076**; NFL **115**: unsupported reduced minutes/playing-time inference.
- NBA **79**: Ball Security says rose when compatible evidence says fell.
- NBA **17896062**: Rim Protection says fell when evidence says rose.
- NBA **38017697**: says Rim Protection changed when evidence says held.
- NBA **56677826**: percentile movement described as development/growth/ability.
- NBA **1028026974**: invented height absent from retained evidence.
- NBA **1028025261**, **1057263194**, **1057266649**, **1057384156**: supplied percentile-band mismatch.
- Analyst NBA player **1028028244**: prose exposes product name “Vibe”.
- Insider NBA team **21**: one pair infrastructure/persistence error; inspect the nested journal error and retained pair progress before any replay.

These are guard/error observations. The guards may correctly reject bad output, or an assignment/guard disagreement may exist. They are not proof DuckDB arithmetic is wrong, nor proof bad prose reached publication. Capture the rejected response and supplied evidence before changing either side. Historical failures remain separate.

## Starting plan for the fresh quality session

1. Read this document and manifests; confirm both workers/revision, queue/outbox health and all 15 holds. Keep the rest of the corpus processing. Start with a small representative set: one held player per sport, FOOTBALL 390 as missing-cohort control, one completed team per sport, and the arithmetic/unsupported-claim failure cases above.
2. Capture complete prepared assignments, selected source rows, source timestamps/revisions, omissions, serialized prompt, actual token counts/options, raw generation, correction response, guard result, and persisted product. Preserve an unmodified baseline before rerunning held jobs. Existing `rust/src/bin/eval.rs` has live/frozen evaluation paths; inspect it and reuse it where applicable. Live reruns alone are not reproducible when the corpus changes.
3. Run bounded **read-only shadow** cohort comparisons for NFL/2026 and FOOTBALL/2026, team and player separately, plus NBA as a known reference. Retain both seasons and full cohort membership in each export. Check exact membership/NULLs/counts/ordering, compatible competitions and years, and the established `1e-9` tolerance only for unrounded percentile/quantile interpolation. Compare fresh computed results to the current published projections; distinguish stale publication from engine disagreement.
4. Trace correct analytical values through rounding, memory selection, compatibility gating, rendering and token budget. Confirm the model sees the meaning of movement, bands, sample limits and unknowns. Test a missing prior season, competition change, low-base rise, unchanged facet, sparse sample and mixed-strength facets.
5. Fix the smallest established failure class, using frozen cases and cross-sport checks. Evaluate input quality separately from prose quality and guard false positives. Do not loosen evidence guards to improve pass rates. Do not change cohort formulas, persona and model concurrently.
6. Once context/quality gates pass, publish a bounded refreshed cohort if justified, release selected reserved Rating jobs using their exact manifest, observe Rating → Analyst → Oracle, and compare actual products. Verify whether cohort refresh changes fingerprints and requires an explicit Rating enqueue: the cohort publisher intentionally does not fan out cognition work.
7. Address permanent production policy separately: cohort refresh owner/cadence, discovery of stale analytical context, admission caps, host-labelled telemetry, and full-load cost. Do not infer those are already solved by today's manual canaries.

## Runtime and recovery facts to carry forward

Scheduling fix `64a5eb1`: independent bounded outbox polling and fair rotation after claims. Current fix `8100c98`: transaction-scoped advisory lease around packet sweeps, so either host may compile without concurrent duplication. Atomic SKIP LOCKED claims plus UUID/captured-revision publication fencing prevent two current owners and stale writes. Notifications are hints backed by durable rows, startup scans and polling. Repeated inference after failure/revision supersession remains possible.

Claim-time Analyst checks pending/running Rating and Vibe for the same entity; Oracle checks all five pillars and undispatched completion obligations. Existing failed-pillar partial-read policy remains; newer work arriving during inference can trigger a subsequent revision. This is per-entity readiness, not a strict global sequence. Editor first means news-dependent work follows published readings/packets; independent stats work need not wait for every article.

API/cron binaries remain `ebac794`; only cognition was updated. API stayed healthy without restart. PostgreSQL 18.6, migrations through 262. No GitHub push/merge happened. Main local backend branch `codex/studio-core`; Archbox checkout branch `codex/studio-production-20260920`; wiki `codex/studio-architecture`. Source updates on the Mac preserve its untracked launcher. Do not deploy the old September 4 Mac binary against the new schema.

Logs/access:

- Archbox repository `/home/sheneveld/scoracle/scoracle-backend`; `journalctl --user -u scoracle-cognition`.
- Mac launcher `/Users/scotty/scoracle-worker/run-worker.sh`, launchd label `com.scoracle.cognition`, log `logs/cognition.log` under that checkout; running executable `releases/shared-worker-20260920/scoracle-cognition`.
- Credentials stay in `.env.local`; do not print them. Existing Archbox helper `/mnt/data/backup/scoracle/releases/studio-20260920/db.py sql` accepts SQL stdin and resolves DB credentials privately. Its `run` mode provides DB configuration only, not full model routing; use the established cron wrapper when full runtime configuration is needed.
- Release/rollback evidence `/mnt/data/backup/scoracle/releases/shared-worker-20260920/` and Mac equivalent; validated pretransition backup and routine backup are documented in the previous rollout report.
- Today's corpus manifest and enqueue receipts `/mnt/data/backup/scoracle/releases/corpus-20260920/`. One-off admission added 3,628 Editor jobs; uncapped producers reconciled 1,320 Rating and 626 Oracle candidates. Those producer counts include coalesced work. Recurring cron caps remain 400 Rating / 150 Oracle; RSS read cap was not permanently changed.
- Today's data feed also reported three NFL gaps waiting for source data and four unmatched roster players. Keep feed incompleteness distinct from cognition and analytical bugs.
- Tests already passed: 497 library + 13 binary cases, 57 PostgreSQL cases across runs, Clippy. Local synthetic PostgreSQL 17 is stopped. These establish mechanics, not semantic quality on today's corpus.
- Wiki companion: `../scoracle-wiki/progress_docs/scoracle-backend/2026-09-20_shared-worker-recovery.md`; prior acceptance: `run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md` and September 19 production-transition report. Preserve unrelated wiki PRODUCT_NARRATIVE.md and frontend/iOS files.

## Fresh-session opening prompt

Read `run_docs/QUALITY_HANDOFF_2026-09-20.md` and its Rating reservation manifest. Continue quality debugging of today's NBA/NFL/FOOTBALL corpus, with particular attention to the analytical context prepared for Scout/Rating and Analyst. Fifteen player Rating jobs are reserved until September 27; do not release them before capturing baseline assignments. Verify live health, then trace one representative case per sport from raw stats and published cohort through memory selection, exact prompt, model response, guards and product. Run bounded read-only cohort shadow comparisons before attributing issues to DuckDB or publishing refreshed context. Keep normal workers processing, preserve existing failures and provenance, and use the supplied concrete failure cases. Make narrow fixes and validate cross-sport semantic quality before staged replay.

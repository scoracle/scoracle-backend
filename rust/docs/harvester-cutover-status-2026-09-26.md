# Harvester cutover implementation status — September 26, 2026

This is a working checkpoint, not a production cutover declaration. The goal is to retire Editor, storylines, packets, and the stories API while feeding verified publisher text directly to character plugins. Laya's five answers are stored as advisory signals. AI provisional annotations are for later human review and calibration, not an admission gate.

## Nightly measurement and staged delivery — September 27, 02:33 EDT

Nightly run 333 is still draining in production shadow. The ingest completed in
193.1 seconds with 4,929 canonical articles and 7,564 article/team edges. Its
Harvester classification throughput, error rates, Laya latency, and routing
fanout are measurable with `rust/examples/harvester_nightly_report.sql` while
the queue runs; `corpus_end_to_end_seconds` appears only after all actionable
Harvester claims finish. Reported Laya recommendations are behavior metrics,
not effectiveness against gold labels. The AI provisional annotations need
human adjudication before precision or recall can be calculated. A high share
of women's-team news is not presently an exclusion error.
Once character batches are released, the same aggregate nightly report will
cross-tab each Laya recommendation against actual character `used` receipts,
including sources used despite Laya not recommending them. Those are useful
disagreement samples and downstream acceptance signals, not gold accuracy.

The reproducible `rust/examples/harvest_provisional_agreement.py` compares the
70-body historical `harvest-context-v1` replay with the 84-candidate AI
provisional annotation queue using article, query entity, and publisher-body
hash. Forty-nine replayed bodies match the annotated bytes; 21 re-acquired
bodies changed and are excluded. On the 49 matches, 44 have entity labels and
30 have each character label. Against those provisional labels, Laya retained
27/30 useful entity openings, 25/25 Journalist positives, 6/8 Influencer,
10/10 Insider, and 11/13 Scout. Its positive-route precision proxies were
27/38, 25/29, 6/26, 10/30, and 11/27 respectively. The broad Influencer and
Insider routes reinforce the decision to let characters make the final call;
these small, non-independent labels do not establish production accuracy or
justify a filtering threshold. The annotation validator confirms 71 AI
provisional rows, 13 pending rows, and zero trainable gold rows.

The next live build has a per-character gate, `HARVESTER_DELIVERY_CHARACTERS`.
When `HARVESTER_SHADOW_MODE=0`, set it to an empty string to record all four
character assignments as `pending` with reason `delivery_held`, without
enqueueing their stages. Named values are comma-separated `journalist`,
`influencer`, `insider`, and `scout`; omission also holds all four. Keep the
empty value explicit for the first live canary so its intent is visible. The
character source loaders ignore held assignments. After verifying identity and
Graph receipts on a small live cohort, run
`rust/examples/harvester_release_character.sql` on the production host with a
specific `plugin_id` and `entity_limit=5` to release one character in bounded
team groups. Inspect assignment dispositions and actual products after each
batch. The running worker remains in shadow mode; the nightly shadow is isolated.

After the nightly queue has no pending, running, or scheduled-retry Harvester
work, restart the CI-passed worker with shadow mode off and the delivery list
explicitly empty. `rust/examples/harvester_enqueue_live_canary.sql` then
reopens at most five previously classified articles from a named ingest run.
Its gate selects nothing unless the named cohort has no actionable Harvester
work, no missing acquisition state or unexplained edge, no Laya error, and
exact headline/body/context/model-input bytes on every latest classification.
It returns only the number enqueued. Verify the resulting acquisition, exact
classifications, four held assignments per article/entity edge, resolved-link
and Graph receipts, and zero character publications before releasing a bounded
character batch. Do not disable Editor until each character's actual product
path and the remaining non-editorial consumers have passed this smoke.
The read-only `rust/examples/harvester_live_canary_check.sql` gives aggregate
completion, exact-byte, held/unheld assignment, per-character disposition and
product-receipt, identity, and Graph counts for the named run without returning
article metadata or publisher text.
The canary gate passed a disposable PostgreSQL positive/negative fixture:
one exact classified article plus one terminal blocked article enqueued exactly
one canary; an active retry, missing acquisition receipt, Laya classification
error, or changed retained body each enqueued zero. The same script also
returned zero against the still-running production shadow cohort.
The worker service executes `rust/bin/scoracle-cognition`, while Cargo builds
`rust/target/release/scoracle-cognition`; the active
`scoracle-cognition.path` unit restarts the service when the installed binary
is replaced. Keep the new build in `target/release` until the shadow drain is
finished. Set `HARVESTER_DELIVERY_CHARACTERS=''` while still in shadow, install
the binary atomically and let the path unit restart into shadow, verify its
health, then set `HARVESTER_SHADOW_MODE=0` and restart once more before
enqueueing the canary. This order avoids a live restart on the older all-on
binary.

The Go ingest is a fresh `go/bin/pipeline` process launched by
`cron-pipeline.sh`, which sources `.env.local` each run. After the live flag
flip, the next ingest sees `HARVESTER_SHADOW_MODE=0` and stops enqueuing new
Editor claims without an API service restart; verify that queue behavior on
the next scheduled sweep. Cognition does require a restart to load the flag.

If a released character fails its live smoke, remove it from the delivery
list and restart cognition, then run
`rust/examples/harvester_hold_character.sql` for that plugin to re-hold its
pending source assignments. For an urgent rollback, stop cognition before
the SQL so no in-flight character call races the hold; restart after checking
the count. This stops future source processing but does not erase terminal
assignment receipts or products already published. The SQL was parsed on the
production schema with a nonexistent plugin and changed zero rows.
In a disposable PostgreSQL fixture, releasing a held Journalist assignment
cleared only that plugin's hold and enqueued one `narratives` entity claim;
the Influencer assignment stayed held. Repeating the release changed zero
rows. The rollback script restored the Journalist hold while leaving the
queued claim visible for its held-aware source loader to skip. The fixture
database was stopped after these checks.

The optional `ARTICLE_READ_CHROME_ENABLED` browser fallback is unset on the
production host. A count-only on-host probe of three `low_content` articles
recovered one above Harvester's 20-word floor, at only 23 words; two remained
low-content. No source text or URLs were exported. The fallback remains off
for this sweep, preserving one consistent retrieval configuration for the
corpus-duration measurement. Review the full acquisition outcome mix before
choosing a later retry or browser policy.

## Production shadow update — September 27, 01:47 EDT

The implementation below describes the September 26 local checkpoint. Since then,
`archbox` has deployed the `codex/harvester-cutover` branch (through `dd3c388`)
with additive migrations 267–284. The API, cognition daemon, and a separate
`scoracle-laya` user service are healthy. Laya runs on the production host at
`127.0.0.1:8019`, using checkpoint
`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` on CPU with four threads.
Publisher text stays on the production host. The local-Mac reverse tunnel was
rejected by automatic approval review and has not been used.

The production `.env.local` now has `HARVESTER_INGEST_ENABLED=1`,
`HARVESTER_SHADOW_MODE=1`, and a `COGNITION_STAGES` roster containing both
`editor` and `harvester`. The 02:00 Detroit nightly ingest will enqueue every
canonical candidate for Harvester classification while retaining the capped
Editor enqueue as the live publisher. Shadow Harvester persists acquisition,
exact source context, and Laya outputs; it does not create character assignments,
identity links, or Graph handoffs. Editor, packets, and stories remain in place.
The earlier `.env.local` was saved as `.env.local.pre-harvester-20260927` on the
host. To stop new Harvester intake, set `HARVESTER_INGEST_ENABLED=0`; to stop its
drain too, remove `harvester` from `COGNITION_STAGES` and restart cognition.

Before migration, the September 27 05:09 UTC PostgreSQL dump was completed on
the data volume. After moving the Laya runtime to that volume, the dump was
copied to the separate system disk and byte-compared successfully. A live
full-delivery canary accounted for all four character assignments: Journalist
used the source, while the other three abstained. A negative verdict with an
unneeded quote initially caused Scout and Influencer retries; the parser now
discards quotes on negatives while retaining strict exact-quote validation on
positives. A second live canary verified shadow classification with no
assignments. Twenty-four retained-body samples verified exact headline and
publisher opening bytes. Three additional Google candidates were fetched and
classified by Harvester with exact source bytes; one official-club candidate
remains a visible `low_content` retry. Four-thread Laya averaged 3.12 seconds
of model time on a ten-article retained-body sample. These are smoke results,
not a full-nightly throughput or human-labeled accuracy result.

Run the read-only `rust/examples/harvester_nightly_report.sql` on `archbox` after
the 02:00 ingest completes. It reports the full observed article/entity corpus,
acquisition and Laya errors, routing distributions, character dispositions,
queue backlog, and elapsed classification time. Do not declare live cutover or
remove Editor, packets, or stories until the full corpus and character smoke
have been reviewed. Earlier lines in this document that say production was at
migration 266 or lacked Harvester flags are preserved as the September 26
baseline, not the current host state.

## Nightly cohort and deployed dependency audit — September 27, 02:18 EDT

The 02:00 production ingest (run 333) completed successfully in 193.1 seconds:
206/206 searches succeeded, yielding 4,929 canonical article candidates and
7,564 article/query-entity edges. Harvester is still draining in shadow mode;
Editor remains the live publisher. The deployed worker now permits four
overlapping Harvester claims, so publisher fetches can overlap while the
on-host Laya service serializes inference. This worker change passed its
focused Rust tests. PR #15 CI passed after the worker change; the later
read-only report change has a new CI run in progress.

The read-only nightly report now distinguishes pending claims, scheduled
retries, and dead letters. It withholds `corpus_end_to_end_seconds` until no
actionable Harvester work remains, every candidate has an acquisition state,
and each article/team edge has either a classification or an explicit terminal
duplicate/error outcome. It reports missing acquisition states, explicit
terminal-error edges, and unaccounted edges separately, so a missing enqueue
cannot masquerade as completed corpus processing. It also verifies every classification's
retained body hash, headline, exact context bytes, and exact Laya-input bytes
against `news_articles.full_text` without exporting publisher text. At 02:16,
all 160 completed edges passed all four checks. This proves storage fidelity
for completed work, not semantic accuracy or whole-corpus completion. The
provisional AI labels remain unadjudicated; no precision or recall claim is
justified from Laya's own outputs.
The report now also splits acquisition outcomes and retry attempts by status.
At 03:21 EDT, 60 articles were `low_content`, 45 `blocked`, 10 had a
retryable transport/provider error, and 16 were explicit duplicates; no
classification errors had occurred. These are evolving counts, not final
failure rates. Blocked pages had already averaged 4.18 recorded attempts,
which will inform the later retry policy without changing this sweep mid-run.

The live production catalog has one noninternal trigger on legacy packet
tables, `enqueue_voices_on_packet`. No public views referencing
`editor_reads`, `storylines`, or `packets` appeared in the catalog query.
Functions whose deployed definitions mention those tables are
`collapse_exact_title_duplicates`, `enqueue_voices_on_packet`,
`promote_established_parts`, `seal_storylines`,
`settled_transfer_identity_evidence`, and
`storyline_part_established_gate`. The active crontab still runs
`cron-narrative-links.sh` at 02:45, `cron-rust-statcommentary.sh` at 03:00,
`cron-vibesynth.sh` at 05:00, and `cron-watchdog.sh` at 08:30/20:30.
`HARVESTER_INGEST_ENABLED=1` and `HARVESTER_SHADOW_MODE=1` are active;
the opt-in Harvester source modes for Scout reports, player selection,
Sigil selection, watchdog, and narrative-link maintenance are not set.
Those switches and the legacy API/SQL consumers remain explicit cutover work.
At 03:04 EDT, Editor's production queue had no pending or running claims; its
only remaining row was one five-attempt failed dead letter from September 24.
Go's Harvester ingest mode enqueues Editor only while shadow mode is on, so
turning shadow off stops new Editor intake. The readiness query now includes
all Editor queue rows, including old dead letters, to make the final roster
drain visible before removing Editor.
The readiness query also now selects the latest completed ingest's exact
`last_seen_at` cohort, matching the nightly report. Its older rolling 24-hour
filter mixed 28 earlier articles and a pre-nightly live canary into the
September 27 readiness counts; the corrected production query reports the
nightly's 4,929 canonical articles and no shadow character assignments.

## Implemented locally

- Go ingestion has an opt-in `HARVESTER_INGEST_ENABLED=1` path. It records every article/query-entity edge, bypasses the Editor ten-read cap, and enqueues Harvester instead of Editor for new edges.
- Rust registers Harvester as an opt-in normal worker. Its fenced publication stores one retained publisher body per article, exact headline and context offsets per article/entity classification, complete Laya distributions and provenance, and four pending character assignments. Acquisition/model failures remain queryable and retryable; HTTP access denials (401/402/403/451) are recorded as `blocked`, while 429 and transport failures remain `retryable_error`. Response URL/domain now persist for HTTP, low-content, and post-fetch classification errors so failure rates can be audited by publisher. The shared queue retries at 30 seconds, 2 minutes, 10 minutes, then 30 minutes before dead-lettering on the fifth failure. Low-content pages remain `low_content`; neither errors nor low content become relevance rejects. The fetcher may use the existing opt-in Chrome fallback for sparse HTML, but never substitutes the RSS description.
- The Journalist can consume Harvester classifications directly. It verifies source hash and byte ranges, constructs a source corpus, bypasses storyline history and storyline progression, and records cited sources as `used`; uncited assigned sources are `abstained`. Its legacy path remains for work already queued under Editor.
- Influencer can consume one verified publisher context per claimed step. A source-only Granite reaction verdict now requires an exact quote showing human emotion before card creation; a routine schedule receives an auditable `abstained` disposition. Positive verdicts retain quote/model/prompt/input-hash provenance, and bounded quote correction never relaxes the verbatim guard. Card publication remains tied to that article ID, and the adapter defers with durable progress until all assigned sources are processed. Its packet path remains for legacy work.
- Graph prefers Harvester context when classifications exist and now verifies its headline, body hash, and opening byte range before extracting relations. Harvester records exact name-surface candidates only when visible in the delivered headline or publisher opening. A separate resolved-link writer promotes unique canonical names with body/opening hashes and a resolution method; aliases, Google query provenance, and Laya choices remain insufficient. Ambiguous shared names and alias-only matches now receive durable source-hash-bound unresolved receipts rather than silent omissions or authoritative links. Graph can inspect unique candidates without an Editor link.
- Graph now nominates unknown people to the existing Investigator queue for Harvester articles only when their names appear in the exact publisher headline or hash-verified opening. It code-slices a source quote, shares the existing sport/name idempotency key, and counts each article once; the candidate is an investigation request, not an authoritative identity link. Isolated Graph tests cover nomination, duplicate replay, fabricated-name refusal, source-drift failure, and legacy behavior.
- Graph also owns a verbatim completed-fixture result review on Harvester articles. Its `g6` source extraction proposes one line; the claim-fenced adapter requires that line in the exact publisher headline or opening, parses one score, resolves two distinct unique team surfaces, and records a reviewed outcome before creating or correcting a fixture flagged `needs_verification`. Empty, invented, malformed, or unresolved results stay queryable without creating fixtures. Reviews are keyed by source-material hash so a later reading cannot overwrite the original fixture evidence. Eight isolated Graph database tests now pass, including fixture publication, exact quote refusal, stale-claim idempotence, review history, and legacy behavior.
- Insider has a claimed source path requiring a resolved team and player/coach co-mention, with Harvester-only deterministic transfer heat and durable pair obligations. Its packet-free prompt keeps the complete exact publisher opening; a positive verdict must include a verbatim source quote and the resolved subject. It atomically publishes a source-linked transfer row, pair outcome, junction-origin Graph event for a player rumor, source disposition, and outbox event. Each player pair creates a durable identity-review obligation. The source-only review applies only an incoming, threshold-eligible rumor, checks the retained publisher bytes again, asks the adjudicator against that exact opening, and fences application, rating fan-out, and autofill invalidation with the claim. Ineligible reviews close with a reason; a later claim completes any pending autofill refresh. After source pairs close, durable per-team/player scored-board wraps run one target per claim without storyline history; empty boards and unchanged inputs receive explicit skipped receipts. A new classification terminally supersedes pending wraps for an older team work revision. Unit tests cover positive, negative, and invented-quote replies; an isolated database smoke published two grounded rumors from one article, skipped their identity reviews because the synthetic sport has no threshold, scored the team and both players, skipped an unrelated team, fenced a stale claim, superseded an old wrap, and drained all current wrap work. Legacy transfer work still uses its existing path.
- Scout has a claimed source path that reads the exact publisher opening and records an exact quote for performance, roster, or availability reporting. A performance report can trigger a rating built from the existing measured profile only when Harvester independently resolved the article to that entity. Roster and availability reports can also trigger a measured rating only when a source-linked applied transfer-identity or availability record corroborates the same player/team; the article remains a trigger, never statistical evidence. Missing measured data, unresolved identity, and uncorroborated reports remain relevant-but-unused. Its isolated database smoke publishes measured performance, corroborated availability, and corroborated roster ratings with source provenance, and proves that a deliberately wrong-club positive verdict cannot publish. Ordinary Scout report reads now reject source drift and structured roster/availability records that have since been reverted. Real-model validation still needs coverage. Legacy Scout work remains available.
- Harvester owns week sealing and exact-title dedup scheduling. Its additive dedup function prefers a Harvester-acquired publisher copy; the legacy Editor function remains unchanged.
- Nightly `statcommentary` now has an opt-in `STATCOMMENTARY_PLAYER_SELECTION=harvester` selector: current-season headliners or players with a recent authoritative Harvester source link, while teams remain unconditional. Its SQL executes on the disposable migrated database. The default storyline-placed player selector stays in force until the new path is smoke tested and enabled, preserving the existing nightly behavior during transition.
- Nightly `vibesynth` Sigil reconciliation has the matching opt-in `VIBESYNTH_PLAYER_SELECTION=harvester` selector. It keeps teams unconditional and selects rated players who are headliners or have a recent authoritative Harvester source link; the production default remains the storyline selector. Both SQL modes parsed and executed on the disposable migrated database, and affected Go tests passed.
- Ordinary Scout enrichment has an opt-in `SCOUT_REPORT_SOURCE=harvester` reader. It takes only Scout-owned `used` source quotes, verifies the retained headline/body hash/opening byte range and quote on read, resolves injury versus suspension from the structured availability record, and fails the rating preparation on source drift. That opt-in also suppresses storyline history in the ordinary rating assignment. An isolated database smoke retrieved both performance and corroborated availability reports and refused a changed source body. The Editor/packet reader remains the default until shadow and cutover.
- The legacy Go `/stories` and `/story/{id}` routes remain available until Harvester is final and smoke tested. Their retirement is part of the later Editor removal.
- A read-only [cutover readiness query](../examples/harvester_cutover_readiness.sql) reports unclassified query edges, missing/acquisition-error states, assignment dispositions, Insider pair/identity/wrap receipts, and queue status. It passed against the disposable migrated database. The production watchdog now has an opt-in `WATCHDOG_SOURCE_MODE=harvester` mode for query-team classification coverage and newest classification; its Editor/packet checks remain the default. Both SQL modes ran against the disposable database, and a rollback-only two-team fixture correctly alarmed at one classified team out of two. The 80% coverage threshold is inherited from the old team-coverage check and must be ratified against full nightly shadow data before enabling Harvester mode.

## Verified locally

- Current Rust library suite: 571 passed, 74 ignored, zero failed. `cargo fmt --check` and `cargo check --examples` also pass. The extended source-to-character database smoke passed separately and covers Influencer reaction-gated cards/abstentions plus Scout performance, availability, roster, and reverted-record refusal. The ignored set includes database and local-model smokes that require opt-in execution.
- The complete Go test suite passed, including the opt-in Sigil selector's SQL against the disposable migrated database.
- Migrations 267–284 applied in sequence to a disposable database restored from the checked-in schema baseline. Migrations 277–282 now register their versions in `schema_migrations`; the disposable database ledger was repaired to reflect objects already applied there. No production migration has run.
- A read-only, one-article smoke with the local pinned Laya English checkpoint and Granite 4.2 model passed through `harvest-context-v1` serialization and Journalist creation. The smoke initially exposed that the newer palette call received only a 180-character source phrase; Journalist now includes the full verified publisher opening in its actual model prompt. This is one source and one character, not a nightly throughput or production shadow result. The trace is temporary at `/tmp/harvester-context-real-model-774278-20260926.json` and contains source text, so it is not checked in.
- A separate read-only replay ran the same local Laya checkpoint through the packet-free Harvester contract for all 24 complete publisher bodies preserved in the 84-row local candidate file: 24 classified, zero classification errors, median 894 ms per classification and 21.6 seconds total inference wall time. Advisory routing sent 20 of 24 to all four characters, matching the known broad zero-shot behavior; no route was used to suppress an article. Sixty candidate rows have no complete body in that local file, so this is not a complete 84-row source replay or a new acquisition-failure count. The hash/count-only temporary summary is `/tmp/harvester-context-shadow-20260926-v2.json`; the checkpoint remains an uncalibrated development cache.
- A fresh read-only acquisition replay revisited those 60 missing bodies through the current publisher fetcher: 46 acquired, 7 low-content, and 7 explicit fetch errors, in 24.5 seconds with four fetches in flight. Together with the 24 previously retained bodies, the current local corpus contains 70 complete publisher texts. The 14 unavailable pages were not sent to Laya or labeled irrelevant. Temporary publisher corpus: `/tmp/harvester-shadow-acquired-20260927-v2.jsonl`; count/hash-only receipt: `/tmp/harvester-shadow-acquisition-20260927-v2.json`. Both are local scratch evidence, and the corpus contains publisher text. The acquisition replay CLI now uses the same blocked-versus-retryable HTTP distinction as the worker and records the responding publisher domain for HTTP failures; this improvement has compiled but has not rerun the historical corpus.
- The current `harvest-context-v1` classifier replay verified all 70 retained bodies against their exact source byte ranges and had zero classification errors: 61 entity-relevant, 9 irrelevant, with 49 of 70 receiving all four advisory routes. Median classification time was 440 ms, p95 479 ms, and total replay wall time 31.2 seconds on the local checkpoint. The result remains a historical cohort replay, not a full production nightly shadow or calibrated quality evidence. Summary: `/tmp/harvester-shadow-classification-20260927-v2.json`. The replay CLI now fails if no body classifies, preventing a mistyped endpoint from looking like a completed shadow.
- One of the newly acquired bodies (article 774320) also passed a read-only Laya → exact-context serialization → Granite Journalist smoke with one source-cited narrative. Its local trace `/tmp/harvester-context-new-source-774320-20260927.json` contains publisher text and stays outside Git.
- A Scout real-model smoke found that the first `scout-source-v1` prompt returned `none` even for an unambiguous completed team score and player stat line. The clarified `scout-source-v2` prompt distinguished a source trigger from measured statistical evidence; local Granite then cited exact source words for clear performance and injury fixtures and abstained on a speculative transfer. A 70-body historical replay initially returned 68 valid `none` verdicts and two positive answers with non-verbatim quotes, which correctly failed the parser. `scout-source-v3` adds a bounded verbatim-quote correction without relaxing the source guard; the replay then completed with zero errors and 70 abstentions in 34.6 seconds. All 19 AI-provisional Scout-positive labels present in the acquired corpus are therefore review priorities, not gold misses: some mark analysis, awards, or other-team reporting that may not meet the source-trigger plus measured-evidence contract. It does not establish full-nightly recall or source-to-rating production behavior.
- The Insider source-pair adapter passed local Granite smokes for an explicit agreement (source-quoted rumor) and a no-transfer co-mention (cleared). Influencer's original card prompt made a neutral mood card from a routine schedule; its new exact-quote reaction gate accepted a clear cheering report and passed on that schedule. A 70-body local gate replay returned 70 passes, no positive reactions, and zero model/quote errors in 78.8 seconds. All 10 AI-provisional Influencer-positive labels present in the acquired corpus are therefore review priorities, not gold misses; a human should verify that their cited openings report observed reaction rather than merely opinion or a predicted response. The added model-call cost must be measured in the full nightly shadow.
- Source-opening inspection of the 70-body corpus found some pages whose first three extracted sentences still include navigation furniture or literal HTML entities such as `&#x27;`. These remain verbatim to the retained body and pass byte-range checks, but they can reduce character usefulness. The extraction quality needs a source-sample review during full-nightly shadow; no semantic admission gate was added from these observations.
- A read-only aggregate query on the configured `archbox` production database found 5,271 Google article/query rows on the September 26 Detroit-local sweep day. The canonical join yielded 4,802 distinct articles and 5,024 distinct article/team edges; 247 rows repeated an article/team edge. Production is still at migration 266. A reproducible read-only nightly exporter is prepared to select one row per canonical article/team edge, and the local acquisition replay fetches each canonical URL once while retaining all distinct query edges. A two-edge local fixture verified this fan-out; the exporter SQL parsed against the disposable database. The proposed full production candidate export to local scratch was rejected by automatic approval review pending explicit authorization for its metadata/publisher-text payload and destination, so that full-nightly payload was not transferred and no full-nightly shadow has run.
- The cognition service is active on `archbox`; its env file currently lacks `HARVESTER_MODEL_ENDPOINT`, `HARVESTER_INGEST_ENABLED`, `SCOUT_REPORT_SOURCE`, `STATCOMMENTARY_PLAYER_SELECTION`, and `WATCHDOG_SOURCE_MODE`. This confirms the new route and its opt-in readers/monitor are not deployed there yet. The presence check did not print secret values. The new `VIBESYNTH_PLAYER_SELECTION` and `NARRATIVE_SOURCE_MODE` switches are default-off in code; their production environment presence has not been checked.
- The backup restore drill now has an opt-in `RESTORE_SOURCE_MODE=harvester` structural check for Harvester storage primary keys and transfer/dedup functions. It defaults to Editor packet-trigger assertions until cutover. Its shell syntax passed and the Harvester objects were found in the disposable migrated database; a complete dump-and-restore drill still needs a production-like run.
- `cron-narrative-links.sh` now has an opt-in `NARRATIVE_SOURCE_MODE=harvester` that keeps co-mention, typed-link, source-performance, person promotion/reconciliation, dynamic Investigator refresh, and season-week maintenance while skipping storyline fill/seal/part promotion. A temporary copy prevented production environment loading; both modes ran successfully against the disposable database, and the Harvester mode visibly skipped all three story lifecycle calls. The production default remains Editor mode.
- Isolated database tests passed for exact Harvester context/body-drift detection, packet-free Journalist publication without a storyline, and Graph preference for Harvester source context. A Harvester claim smoke test verifies multi-entity provenance, advisory Laya answers, all eight assignments, exact identity candidates, Graph/Journalist handoffs, Influencer outcomes (source-linked cards and a provenance-bearing abstention), Insider's two source-grounded pair publications, and Scout's measured rating trigger without Editor work. A stale Influencer and Insider claim cannot publish; their multi-source/pair backlogs checkpoint and resume without dropping assignments. Existing Journalist fencing publication passed. The dedup function chose the acquired publisher copy in a cross-source duplicate pair.

## Required before live cutover

- Influencer's source path has passed an isolated database smoke, a narrow positive/negative real-model check, and a 70-body reaction-verdict replay. The all-pass corpus result needs human review, and full-nightly shadow throughput and card-outcome validation remain before live enablement.
- Insider's source-only identity-review code has an isolated database smoke covering exact publisher context, `rumor_threshold` evidence, the current-team override, player/old-team/new-team rating fan-out, post-commit autofill refresh, source-drift refusal, and adjudicator rejection. Real-model shadow validation still needs coverage before it can replace the old team drain. Its scored-board wrap, junction-event banking, and ineligible identity-review path also have isolated database smoke coverage. Scout's source-linked roster and availability source-to-rating paths passed isolated database smoke; real-model validation still needs coverage. A Harvester tag alone cannot publish either product.
- The strict canonical-name writer exists; Insider uses it for subject pairs, while Scout requires the resolved query entity and exact source quote. Harvester now mirrors only these hash-bound unique canonical links into the shared `news_article_entities` index within its fenced claim, replacing stale links on replay. That preserves the co-mention and Investigator activity readers after Editor stops. The isolated database smoke confirms the shared index contains only the resolved team and players, never the alias-only or ambiguous mentions; a second link-resolution pass removes a seeded stale link. Graph now nominates unknown source-anchored people for Investigator review. Ambiguous and alias-only name cases still need equivalent adjudication. Google query provenance remains deliberately distinct from an authoritative article/entity link.
- Personnel's legacy packet/Editor report lookup now has a tested opt-in Harvester alternative; the default legacy branch and other packet/storyline SQL consumers still need retirement after shadow. Ambiguous-name and alias-only cases have source-bound unresolved receipts but still need an adjudication owner if their links are required by products. The watchdog has a tested opt-in Harvester source mode, but its coverage threshold and deployment setting need full-nightly validation. Both nightly player-rating and Sigil selectors have tested opt-in Harvester alternatives. Source-grounded fixture nomination has an isolated smoke, but still needs frozen-case parity and real-model shadow validation. Production database integration and nightly shadow throughput have not been run.
- The new migrations have not been applied to the production database. Harvester remains disabled by default; do not set `HARVESTER_INGEST_ENABLED=1` until the remaining character and identity paths are ready.

## Editor obligation audit

| Existing obligation | Current Harvester owner or remaining gap |
|---|---|
| Publisher fetch, retained `news_articles.full_text`, acquisition errors | Harvester owns the additive source acquisition state and retains the fetched body. Failure/retry parity still needs a production-like shadow run. |
| Claim fencing, dedup, week sealing, Graph enqueue | Harvester has claim-fenced publication, Harvester-aware dedup and week sealing, and Graph handoff; isolated database smoke covers the main publication transaction. |
| Known identity links and unknown people | Harvester writes uniquely resolved canonical-name links separately from advisory Laya decisions, then mirrors those links into the shared graph/Investigator index in the same fenced claim. Graph owns exact-source unknown-person nomination into Investigator; ambiguous and alias-only cases still need an owner. |
| Fixture-result nomination | Graph now reviews one exact final-result line from a Harvester article and can create/correct a verification-flagged fixture in a fenced transaction. Frozen reporting-case parity and real-model shadow checks remain before Editor retirement. |
| Storyline membership, packets, generated stories | New character source paths bypass these. Legacy memory, personnel, API and analytical consumers still need a dependency audit and retirement migration before the tables or routes can be removed. |
| Insider score and identity application | Pair verdicts, source-linked rumor rows, junction banking, a durable scored-board wrap, and a source-only identity-review owner work locally. Eligible application, rejection, source-drift refusal, rating fan-out, and autofill refresh passed an isolated database smoke; live-model shadow still needs proof. |
| Scout current reports | The source path uses an exact quote as a rating trigger and measured stats for numeric claims. Roster and availability publication requires a source-linked applied structured record. Ordinary report lookup has a verified opt-in Harvester reader; legacy report lookup remains the default until shadow. |

## Legacy consumer inventory for retirement

| Consumer | Current dependency | Retirement condition |
|---|---|---|
| Go `/{sport}/stories` and `/{sport}/story/{id}` handlers, database reads, generated Swagger | `storylines`, `storyline_articles`, `packets` | Remove routes and generated API documentation only after the new character path is live and smoke tested. |
| Web frontend Stories board and story detail route in `scoracle-frontend` | `leaderboard?board=stories`, `/stories` redirect, `/story/:sport/:id`, `getStories`/`getStory`, and their API URL helpers still consume the legacy endpoints | After live smoke, remove the board, route, data fetchers, navigation state, and related tests in the frontend in the same release sequence as backend endpoint retirement. Define the fate of old deep links before deployment. A search of the iOS checkout found no direct story endpoint caller. |
| Journalist, Influencer, Insider legacy adapters and shared packet renderer | packet corpus and storyline framing for Editor-era work | Drain or explicitly migrate queued Editor/packet work; then remove the legacy branch while preserving Harvester source dispositions. |
| Graph article loader in `src/plugins/graph/adapter/mod.rs` | Falls back from Harvester context to `editor_reads.read.evidence_blurb` for historical claims | Verify Graph's exact-source Harvester path on the live canary, then remove the Editor fallback before dropping `editor_reads`. |
| Ordinary Scout enrichment and nightly player/Sigil selectors | Editor report fallback and storyline-based player eligibility | Enable and shadow-test `SCOUT_REPORT_SOURCE=harvester`, `STATCOMMENTARY_PLAYER_SELECTION=harvester`, and `VIBESYNTH_PLAYER_SELECTION=harvester` first. |
| Character memory, `src/evaluation/tasks.rs`, and narrative eval/inspection tools | storyline history, packet corpus, and story-part progression; the Influencer evaluation builder still calls `load_vibe_context` | Prove source-only production prompts need none of these; adapt offline evals to source fixtures before table removal. |
| `cron-narrative-links.sh` and database narrative functions | storyline fill, seal, established-part promotion, co-mention refresh and dynamic Investigator refresh over shared `news_article_entities` | Harvester supplies verified links to that shared index. The tested opt-in cron mode skips story lifecycle while retaining graph/person maintenance; enable it only after live source-link smoke. |
| `cron-watchdog.sh` and `restore-drill.sh` | packet freshness and packet enqueue trigger | Switch the tested watchdog source mode at live cutover; update restore assertions before the packet trigger/table is removed. |
| Worker roster/packet compile configuration and Go ingest enqueue | Editor and packet stages can still be scheduled through production flags | Confirm no new Editor or packet work, drain existing claims, and remove those stages and flags only after all live source consumers pass smoke. |

This is a code-level inventory, not proof that production has no additional dashboards or ad hoc SQL consumers. The destructive table/trigger migration remains later than application cutover and smoke.

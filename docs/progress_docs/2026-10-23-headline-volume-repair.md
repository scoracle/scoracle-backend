# October 23 headline-volume repair — local implementation

Production remains unchanged. This records local work on main above `0ac0b9d843cc`; it is not a deployment or edition-recovery receipt. Preserve the other in-progress changes in the recovery plan.

## Selected policy

The user selected three controls: prevent previously reviewed reposts from consuming model work, collect only since the last fetch with a wider morning catch-up, and cap each entity's character input by retaining the strongest decision scores. The implementation starts at **40 headline reports per entity and character**, including optional headline history. It reuses the existing collector, decision receipts, frozen assignments and delivery dispositions. No batching service, semantic similarity model or extra scheduling system was added.

### Deduplication

- Migration 323 supplies one presentation-normalized key: Unicode compatibility normalization, case, whitespace, curly quotes and typographic dashes. Numbers, negation and punctuation remain significant. Changed wording stays new evidence.
- The collector joins exact normalized reposts to an earlier canonical article for the same entity/sport within 72 hours, for headlines at least 30 characters long. It archives every URL and publisher row and retains both original and canonical query provenance; only the canonical article is queued. A genuine claim change clears the old duplicate pointer.
- An unchanged headline/publisher retains its original publication time when Google merely refreshes the timestamp. This prevents a metadata-only repost from invalidating frozen evidence.
- Exact decision requests reuse saved receipts across article IDs, with the same request, criteria, canonical subject metadata and serving-model identity. Changed publishers are distinct decision requests; collector canonicalization handles matching cross-publisher reposts within its conservative boundary.
- At freeze, each character excludes already reviewed normalized headlines for that entity/sport, then collapses copies in the current edition. Optional history also excludes reviewed headlines and copies of current evidence. Below-cap dispositions do not count as model reviews.

### Fetch progress

- Persist a separate fetch watermark per entity type, ID and sport. With no watermark, bootstrap from the last successful legacy news-pipeline run, or 24 hours if no such run exists.
- Search Google with the rounded-up elapsed hours, or days for longer gaps; filter returned publication dates to the exact closed interval between the saved cursor and fetch start. An ordinary window uses `when:1h`; late sweeps request enough extra time to avoid a gap. Morning reopening automatically covers time spent in character work, including across a collector restart.
- Advance the watermark in the same transaction as article persistence and queue admission, only when every alias request succeeded and an explicit acquisition limit did not truncate results. Ordinary successful empty fetches advance progress. The user-selected long catch-up recheck below additionally delays advancement for morning catch-up. Partial fetches retain the original cursor; persistence failures roll back progress and articles together.
- The collector's default acquisition limit is now zero, preserving all returned candidates for decision scoring. An explicitly supplied `-rss-limit` still works and prevents cursor advancement when it truncates.

### Ranked ceiling

- During the Rust controller's transition into a frozen character edition, apply the selection transaction before building Scout's frozen request.
- Rank unique eligible headlines by the relevant decision score: Journalist subject, Influencer emotion, Insider movement, Scout availability. Break ties by subject score, publication time and spectrum ID. Preserve existing admission thresholds and identity validation.
- Keep at most 40 fresh headlines. Optional history uses only unused slots within that same ceiling. Actual tokenizer admission remains the final context boundary; unusually long headlines can still require an explicit hold.
- Record copies as `redundant` with `already_reviewed_headline` or `reposted_headline`. Record weaker overflow as `abstained / ranked_below_entity_cap`, retaining rank, scores, policy version and `model_called=false`. Keep all frozen IDs and archived evidence for accounting. Unresolved identities remain visible blockers.

## Verification

`scripts/check.sh` passed against a fresh UTF-8 PostgreSQL 18 database: 377 Rust library tests; 68 serial database/helper checks; binary tests; SQL freeze, publication and new selection contracts; Go vet/build, prepared statements and race tests. The baseline restore and repeated migration path passed. The checked-in schema snapshot includes migration 323. Production's UTF-8 encoding was verified read-only.

Regressions cover cross-URL/cross-publisher reposts, timestamp-only updates, changed denials, exact decision reuse across IDs, no reviewed history, a shared history/fresh ceiling, strongest-score retention for all four characters, unresolved identities, repeat selection, actual `when:1h` and `when:10h` requests, strict date boundaries, partial alias failure and durable monotonic fetch progress. Tests use synthetic sources and a fake decision provider; no new production inference was run.

Earlier local repairs remove arbitrary fresh-packet byte/source-count gates while retaining exact tokenizer admission, and scope number/date checks to the cited report. Replaying three saved Insider responses through the real parser accepts the two demonstrated false rejects and still rejects the unsupported Week 7 claim. These are parser regressions, not general semantic-quality approval.

## Release and existing-edition boundary

The candidate requires migration 323 before either new collector or new character binary; deployment verification now checks that contract. The release must include the Go collector and Rust controller together. No commits, pushes, deployment, service restart or production mutation happened during this work.

The current frozen edition is deliberately not rewritten by installing this code. Selection operates on pending deliveries at a new freeze; it does not reopen existing held deliveries or change saved Scout packets. Before live recovery, refresh the current edition and exact release identities, preserve the affected assignment/delivery/work receipts, prepare and test a bounded replay for each proven repair category, and separately resolve stale source revisions and identity holds. The three previously diagnosed stale Journalist sources must retain currency validation. Do not globally reset attempts, relabel identity holds as cap exclusions, discard outstanding obligations or force qualification/fetch reopening.

Known limits: presentation-equivalent deduplication does not identify reworded versions of the same story; Google's own result availability still bounds collection; a count ceiling does not guarantee every packet fits the model. Natural edition completion and production model quality remain unverified.

## Read-only comparison — October 23, 12:41 America/Detroit

Sample: the three failed assignments with the largest frozen source lists in each of Journalist, Scout, Insider and Influencer, from the still-active October 23 edition. Simulated the new selection over their remaining pending/held inputs, without changing those receipts or invoking any model. This is a counterfactual reopening comparison, not an executed edition repair. The sample favors large failures and does not represent the average production workload.

| Remaining frozen headline obligations | Count |
|---|---:|
| Total before | 1,535 |
| Unresolved identity; still blocked | 101 |
| Identity-resolved inputs before selection | 1,434 |
| Previously reviewed | 5 |
| Additional duplicate headlines | 319 |
| Unique headlines below the 40-item score cutoff | 798 |
| Selected identity-resolved inputs | 312 |

The selected population is 78.2% smaller than the identity-resolved input population. This is a headline-count comparison, not a measured model-speed improvement or prediction that the assignments now complete. Each capped sample retained scores at least as high as its strongest trimmed score. Some score gaps were very small, so this mechanical property does not establish that all important stories were retained.

Examples: Journalist NFL team 24 had 381 remaining inputs, including 23 identity holds; 33 resolved copies and 285 weaker unique headlines would be excluded, leaving 40 selected. Influencer NBA player 237 had 105 resolved inputs; three reviewed headlines and 69 copies would be excluded, leaving 33. Scout NFL team 28 had 53 inputs, eight unresolved; 29 resolved copies would be excluded, leaving 16.

### Proposed effectiveness trial

1. Freeze a balanced sample of blocked busy entities and previously successful controls across all four headline characters. Label the important events, denials, dates and entities before inspecting generated outputs. Inspect the strongest discarded headlines as well as the selected set.
2. Reuse the existing nonpublishing evaluation harness with the same Qwen model and settings for both input policies, one case at a time. Compare context admission including reserved output, first-attempt acceptance, retries, tokens and elapsed time. Human review must assess important-event coverage and unsupported claims; no extra judge model is needed. No new inference has been run for this comparison.
3. In a bounded collector trial, compare cursor-window results with a separate 24-hour reference fetch to detect missed or late-indexed stories. Check hourly, morning catch-up, empty fetch, failed alias, persistence failure and restart cases. The reference is for comparison only and must not enqueue duplicate production work.
4. After separate release approval, observe three natural editions, including morning catch-up and a restart. Require complete accounting of selected/excluded/blocked obligations, no accidental source changes or duplicate publications, measured fresh-context admission and lower input volume. Record model-output failures separately for prompt tuning; they are not a prerequisite for releasing the input pipeline. Existing identity/stale-source blockers are separate acceptance categories and must remain visible.

## Effectiveness trial execution — October 23

**Result: volume reduction and context admission demonstrated; model-output failures retained for prompt tuning. No rollout or three-edition production trial started.** Production database changes, queue repairs, publication, commits and service restarts remain unperformed. The local disposable PostgreSQL server was used for software verification only.

### 1. Paired model replay

Froze one busy assignment and one successful control for each headline character. The original snapshot, exact per-variant fixtures, raw provider receipts and outputs are retained privately under `/tmp/scoracle-headline-repair.xxB3K3/effectiveness/`. The initial run used the same Qwen3.5-4B Q4_K_M server, temperature zero, configured context 10,000, sequential first attempts and no publishing. Fifteen of sixteen variants reached inference; the large Journalist baseline failed exact token admission without clipping. This is a small deliberately selected sample, not a representative capacity benchmark.

| Busy case | Before input tokens | After input tokens | Initial mechanical result |
|---|---:|---:|---|
| Journalist — Chicago Bears | 30,617 | 3,740 | Before exceeds context; after fits but fails cited-report checks |
| Scout — Tampa Bay Buccaneers | 2,761 | 1,298 | Both rejected for availability output |
| Insider — New York Giants | 5,616 | 3,988 | Before rejected; after parser accepted but factual review failed |
| Influencer — LeBron James | 7,055 | 2,620 | Both returned an all-null reading; review of that abstention remains open |

Successful controls were Teemu Pukki, Reed Sheppard, Martin Baturina and Marcus Sasser. The previously successful Scout control failed current replay on rounded numeric claims and availability formatting. A prior published candidate is not a factual gold label.

### 2. Selection and output review

Assistant review, not human factual approval, found these concrete issues:

- Streaming spam scored highly enough to occupy top-40 slots before downstream filtering. The controller now screens frozen pending headlines with the shared spam predicate before ranking. The same transaction records spam dispositions without consuming valid slots. Observed decorated solicitations and repeated football keyword stuffing have focused positive/negative fixtures; ordinary broadcast guides and availability reports remain allowed. Selection policy is now `headline-selection-v2` and the function accepts the screened spectrum IDs.
- The smaller Journalist case still failed all three production attempts after spam screening. A sentence-case/date prompt experiment also failed and was removed from the candidate. Guard checks remained enabled; no rejected output was published.
- Both Scout cases failed their production correction replay within the three-attempt maximum. Marking availability paraphrases retryable did not improve these cases, so that retry change was removed. An experimental schema letting Qwen choose an exact attributed headline solved that copying problem, but rounded numbers and a separate guard issue remained; the schema experiment is private and unpromoted.
- One Scout guard falsely interpreted a supported thin-sample minimum as a stability claim because another sentence mentioned a season. The temporal check is now scoped to its own claim. The exact positive and a negative cross-season claim are regression-tested. Re-parsing the experimental saved outputs then accepted the busy Scout example and still rejected the control's unsupported `92` percentile.
- Insider's initial parser-accepted output claimed no confirmed signing was reported although a supplied headline announced an unnamed signing. A prompt-only experiment still made unsupported absence/active-pursuit claims, so it too was removed. The synthetic `headline-unnamed-signing` fixture now requires preservation of the reported signing while accepting empty findings for an unresolved counterparty. Its assertions use the existing Insider summary fields, and a unit regression demonstrates that shape acceptance alone cannot satisfy this fixture.

Identity and stale-source holds were excluded from these isolated evidence trials and remain explicitly unresolved. In particular, the busy Bears assignment still has 23 identity holds and one stale source. Passing an isolated writing replay would not complete that assignment.

### 3. Live RSS comparison and selected recheck policy

Ran nine bounded public RSS requests: `when:1h`, `when:10h` and `when:1d` for Chicago Bears, Tampa Bay Buccaneers and Norwich City, with exact local time filtering. No articles were persisted or enqueued in production. Narrow queries often returned more in-window stories than the 24-hour reference; the Bears reference hit Google's 100-item page limit, so it cannot be treated as complete ground truth.

A relevant Bears postgame headline, published well inside the 10-hour window, was absent from the first narrow query and appeared when the same query was repeated minutes later. A Bucs reference-only hit was about another team's linebacker, demonstrating reference noise rather than confirmed coverage loss. Feed results vary even with the same parameters.

The user selected: **recheck the morning catch-up window on the next hourly fetch before advancing it**. Migration 323 now stores `recheck_after` with each entity watermark. A successful catch-up spanning more than two hours persists its articles but retains its cursor and sets an hour-long recheck timer. Early manual retries cannot advance it. After the timer, the next complete fetch rechecks from the retained cursor, advances progress and clears the timer. Partial/failed fetches preserve both the cursor and pending recheck. Ordinary hourly fetches retain their normal behavior. The two-hour boundary distinguishes long catch-up from minor hourly scheduling delay.

Collector regressions cover the saved timer, an immediate retry, an incomplete recheck, the next hourly boundary, a new collector instance resuming the state, monotonic cursor advancement and no new queue work for the same headlines. The live comparison shows why a recheck is useful; it does not prove Google can never index a story later than that recheck.

### 4. Production observation gate

Not started. The user explicitly prioritized reliable fresh-context delivery and accepted the current model failures as separate prompt-tuning work. Input-pipeline rollout no longer waits on those experiments. Production release/scoped recovery still requires its concrete operational approval. Do not count software tests or isolated model replays as natural editions.

Follow-up prompt tuning covers Journalist/Scout articulation and Insider absence claims. The current work is collection, deduplication, selection, model admission and the existing-edition recovery boundary. Preserve the existing output guards and failure receipts.

Latest retained candidate verification: the complete `scripts/check.sh` passed after the trial-driven changes, with 377 library tests and 68 serial integration/helper checks, SQL contracts, fresh-baseline/repeated migrations, prepared statements, and Go vet/build/race tests. The read-only deployment schema verifier passed on the clean disposable baseline. Live-model trial failures remain failures despite this green software suite.


## User priority and scoped recovery preparation — October 23, afternoon

The user accepts model failures as prompt-tuning work and prioritizes reliably serving fresh context. After screening spam before selection, the busy Journalist replay fell from **26,334 to 3,385 input tokens** (initial unscreened baseline: 30,617). The reduced request reached the model under the existing 10,000-token context. Parser rejection after generation is recorded separately from transport/admission success.

At 13:40 America/Detroit the active edition was still October 23, phase `characters`, with fetching closed. Journalist had 16,016 frozen deliveries held by the old byte budget and 15,613 pending deliveries. These are delivery counts, not model calls. There were also identity, stale-source and model-output blockers. A bounded read-only discovery calculation over the entire proposed cohort exceeded its 20-second limit; no query was cancelled manually and no production changes occurred. The smallest pilot was checked separately within a 15-second read-only limit: all **88 remaining inputs were current**.

The read-only snapshot identifies **13 budget-only Journalist assignments, 1,657 frozen IDs, 1,590 remaining budget holds**, all with current source-head pointers and no retained work rows. The shared Rust spam screen found zero spam IDs in this cohort. Source discovery/identity must still be revalidated by each recovery transaction; head-pointer agreement alone is insufficient. Private per-assignment scope files are under `/tmp/scoracle-headline-repair.xxB3K3/effectiveness/recovery-*.json`.

| Sport | Type | Entity IDs | Frozen IDs |
|---|---|---|---:|
| NBA | player | 15, 115, 172, 237, 378, 417, 666676, 1028026974 | 1,027 |
| NFL | person | 8 | 96 |
| NFL | player | 43, 1482, 277730, 281375 | 534 |

`recover_headline_budget.sql` uses exact assignment keys and previews inside a rolled-back transaction. It takes the exclusive coverage lock, refuses active claims, model/identity holds, stale evidence, mismatched queued revisions and existing candidate products. Apply archives the full affected assignment/delivery/work rows plus the schedule row, reopens only proven budget holds, invokes the shared selector for those exact assignments and creates or resets only their queue jobs with a new revision. Previously consumed jobs are recreated under a transaction-local enqueue bypass; the schedule is restored before commit and remains enabled externally. The script never rewrites frozen membership, publishes a product or advances the edition pointer. A recovery marker makes repeat application a no-op, preserving later attempts. Scout's frozen packets and other characters' receipts are untouched.

The new disposable regression proves preview rollback, complete backups, strongest-40 selection, spam exclusion, stale/identity/active-claim refusal, unchanged other-character receipts, recreation of consumed jobs and repeat no-op. It runs in `scripts/check.sh`. The selector's optional JSON assignment scope exists for this bounded recovery; normal freeze passes no scope and keeps the same behavior.

### Selected failure policy

The user approved limited attempts, loud durable reporting and continued production after failures. Migration 324 supersedes the old indefinite-blocker policy: terminal failures close an edition as incomplete once other work is terminal, then collection reopens. Its previous published edition remains visible. The implementation and verification are recorded below; production is unchanged.

### Release/recovery manifest

- Candidate: local working changes above `0ac0b9d843cc0a5c4d730b1c5a7800db882957f3`; no candidate commit, push or deployed build yet. Do not identify the uncommitted candidate as that base release.
- Forward migrations: `323_bounded_headline_inputs` (durable `recheck_after`, content key, cap and scoped selector) and `324_nonblocking_coverage_failures` (terminal reporting and incomplete-edition reopen). Matching baseline and preflight included. Keep both migrations and the Go/Rust consumers together. No dependency changes.
- Existing Archbox checkout remains `0ac0b9d843cc`; actual running SHA256 values: API `6cd6fc118f2eb9ce6469adf0aebd2462a4cabf344d47e3ce7dfbc77e3e158625`; collector `aa5594a440dd9aca2ffea13c5275793c60f6052cd7a3a1182ffff642e6d9774a`; cognition `cd6e54f0a3b64e58120bb11e51f28e3136eb58bf0652b753f936dcc2e36a3400`. Decision server hash remains `bdfa668ca2d6852190ec70863546bf3612ad82ac8c469b31436278dc5d254545`.
- Mac worker checkout remains the same base; actual loaded executable is `releases/routing85-20261022-0ac0b9d843cc/scoracle-cognition`, SHA256 `e51c4effa24086c614cd324f45726f65322207e41f8694a287555a66af911411`. Preserve the launchers and release directories.
- After release approval: build and hash the exact approved revision on each host into a new release directory, stop collector/controller and drain both character workers, record zero active claims, and save a fresh schema/affected-row backup under `/mnt/data/backup/scoracle/releases/headlines-<approved-revision>/`. Apply migration, verify schema, place matching binaries, then run **one** recovery preview for NBA player 15 (88 current held inputs, 89 frozen IDs), exact scope and `spam_spectra={}`. Review before/after accounting and apply with a fresh private backup file. Observe that exact job's selected input, tokenizer admission, model call and final delivery/failure receipt before expanding to the remaining 12 separately scoped previews. Do not combine discovery checks into a large slow transaction or globally reset failures.
- Rollback: retain migrations 323/324, watermarks and failure history. Keep collector/controller paused pending compatibility review; old enqueue producers can reset exhausted budgets. Restore only a verified compatible release or repair forward. Before any recovered job is claimed, restore only archived assignment/delivery rows and remove only its newly created queue row under the exclusive lock. Once new work is claimed/settled, stop and reconcile that receipt; do not replay an old backup over it. Do not restore the whole database or alter the qualified publication pointer.
- Live acceptance: successful hourly cursor advancement; partial-fetch cursor retention; durable morning recheck after restart; no repost-driven inference; at most 40 fresh/history headlines per entity/character; exact input+reserved-output admission; selected/excluded/held totals reconcile; current source/claim checks and existing parser failures remain visible. Observe three natural editions after deployment, including an incomplete edition and morning catch-up/restart. Prompt quality is a separate follow-up.

Final pipeline verification: `scripts/check.sh` passed again on the fresh UTF-8 database `classifier_test_coverage_pipeline_final` after the scoped-recovery changes: 377 library tests, 68 serial integration/helper checks, binary tests, SQL contracts, new recovery preview/apply regression, baseline restore/repeated migrations, prepared statements, Go vet/build and race checks. No additional inference or production mutation was performed for recovery preparation.


## Approved bounded failure policy — local implementation

The user confirmed: give models a limited number of attempts, fail loudly in reporting, and prevent those failures from stopping production. The implementation keeps the existing **five job executions per unchanged input revision**. Existing within-execution correction limits remain unchanged (up to three generation attempts on those paths; five job executions can therefore mean up to fifteen generation calls). A plugin may already finish with a held failure after its own bounded correction cycle. Repeated offers of the same input preserve attempts/backoff in Go, Rust and retained packet fan-out. Changed input revisions reopen the budget; a prompt-only repair requires an explicit scoped retry with a new revision, rather than replaying the corpus automatically.

- Migration **324_nonblocking_coverage_failures** adds `coverage_failures`: edition, failure kind, stage, entity, sport, frozen input revision, known job-failure count, full error, retained receipt and timestamp. It preserves failures after queue consumption or a later changed input. Unknown attempt counts stay NULL and display as `unknown`; they are not inferred from missing jobs. Frozen assignments, prepared Scout inputs, archived sources and held rejection receipts remain available.
- Exhausted decision work settles its exact frozen drain obligation with an explicit `decision_exhausted` receipt. Successful work still requires claim-fenced completion. Retryable decision failures remain in the drain; the edition cannot start early just because the queue is empty.
- Exhausted character work becomes terminal failed, matched to the frozen input revision. Dependent pending assignments are marked `dependency_failed`; their unclaimed queue rows are removed only after the failure receipt is recorded. Healthy independent work and active claims finish normally. Remaining undelivered frozen source IDs are held as `character_assignment_failed`; previous identity/model diagnoses and used/redundant dispositions remain intact.
- Exhausted handoffs remain stored and reported. They no longer block healthy dependent claims or edition closure. Handoffs below five failures still retry and hold the appropriate boundary. Partial handoff failures always make the edition incomplete.
- Once all assignments and retryable handoffs are terminal, an edition with failures closes as **partial / completed_with_failures**. The qualified publication pointer and revision stay unchanged, so neither rejected output nor healthy candidates from the incomplete edition become public. Collection reopens through the existing durable monitor/catch-up path. Deferred changed inputs are reoffered; unchanged exhausted inputs are not reset. An incomplete edition never becomes a successful historical edition merely because a later edition succeeds.
- Both Rust and Go stale-lease recovery count each abandoned execution against the same budget. Repeated crashes therefore cannot create unlimited fresh attempts. Superseded claim tokens and revisions remain fenced.
- Final job and handoff failures emit ERROR logs. The controller logs newly archived terminal failures after commit. Existing `work status`, `work dead-letters` and `sql/queries/recovery_capacity.sql` include the failure archive, including failures with no remaining work row. No separate alerting service was introduced.

Focused tests pass for five exhausted decisions, five exhausted character executions, repeated offers during/after character work, changed revisions, five abandoned leases, reconnecting the controller/worker pool, failed prerequisite propagation, healthy work completing before reopen, unchanged publication pointer/revision, hidden partial candidates, exhausted handoffs and retained held-source diagnoses. Both existing Go operator commands were executed against disposable failure fixtures and showed both archived exhausted jobs and consumed rejection receipts.

### Updated rollout boundary

Migration 324 changes the lifecycle as soon as the controller next ticks. Before applying 323/324, pause the controller and collector and drain both workers; preserve the exact state/backup. The budget-only pilot preview/apply must run while the controller remains paused if we want that old edition to exercise the new inputs before terminal closure. Then start matching Go/Rust consumers and observe the pilot, remaining work, terminal failure reports, partial closure and catch-up. Do not reset all existing failures or publish the partial edition.

Rollback requires compatibility review: restoring old enqueue producers can reset unchanged failed work again, even with the additive schema retained. Keep the controller/collector paused if the release must be rolled back; preserve the failure archive and watermarks and repair forward or restore a verified compatible release. Do not silently restore the old indefinite-blocker function or erase failure receipts. No production mutation, commits, pushes or deployment has happened.

Final verification of the failure-policy extension: the complete `scripts/check.sh` passed on fresh UTF-8 `classifier_test_coverage_resilient_pipeline`: **377 Rust library tests, 70 serial integration/helper checks**, binary tests, SQL contracts through 324, the budget-recovery regression, fresh-baseline/repeated migrations, prepared statements, and Go vet/build/race checks. Baseline 322 → migrations 323/324 was also exercised on a separate clean database before schema capture; the read-only schema preflight passed against it. Focused controller/queue tests and actual status/dead-letter CLI fixture runs passed. No additional live-model calls or production writes were needed for this policy verification.

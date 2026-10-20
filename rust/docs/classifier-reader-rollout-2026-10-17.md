> Historical reader rollout notes. The [October 19 headline plumbing release](headline-plumbing-deployment-2026-10-19.md) supersedes this flow: production now scores and delivers headlines without article reading. Migrations 313–314 are live; model and voice tuning are deferred.

# Shared article reader progress and calibration handoff

**October 19 update:** the user asked to test native `dom_smoothie` and proposed combining reading, full-body relevance/character scoring and retention into one intake job. The [completed isolated experiment](smoothie-intake-experiment-2026-10-19.md) processed all 100 real URLs with roughly 34 MiB native peak RSS, but found recommendation/footer false successes and downstream factual errors. The single-job boundary remains a candidate; default extraction is not qualified for production. No production or local v8 routing change was made. Earlier headline-first directions and Jina recommendations below are historical context.

The subsequent [strict-policy exploration](smoothie-intake-experiment-2026-10-19.md#strict-policy-rejection-exploration) tests one built-in parsing policy plus existing body limits. It blocks the three known bad examples and preserves all six reviewed positives, but also misses one substantive report in the wider saved batch. Treat it as a conservative acquisition candidate; missing/insufficient bodies must not become fabricated negative relevance decisions.

The shared URL reader and d1 Classifier are live on Archbox as of October 17, 2026, 21:56 UTC. Acquisition browser-reads a publisher URL once, retains the returned article, and Classifier scores that same text for each RSS subject using real `meta.rs` context. Existing fresh-context consumers receive the retained text through their current source contract. No character implementation was changed or deployed in this work.

The [current October 18 scope](#current-scope-and-next-context--october-18) records the local relevance-only baseline and the latest requested flow: one decision stage gates relevance and chooses downstream characters, one shared read acquires content, and selected characters consume it. The acquisition recommendation is to retain the existing shared reader. Production still runs the reader-first release described above.

At rollout, the next step was to tune this deployed baseline using new labeled articles. Accuracy remains provisional; the reader is a simpler shared acquisition mechanism, and has not yet demonstrated an accuracy improvement. The subsequent [headline-only comparison](#headline-only-comparison-after-reboot) below records the latest evidence and the revised direction toward conservative headline routing and a stats-driven Scout.

## Current scope and next context — October 18

The earlier cleanup implemented **relevance-only admission with character selection deferred**. The user's latest instruction now fixes the target order: **decision model gates relevance and selects downstream characters → one shared extraction/read → selected character plugins receive content for their output**. Route selection belongs before publisher work. Missed stories are acceptable; usable daily content and a simple operating path take priority. Production remains on the previously deployed reader-first v6 release; the following v8 plumbing is local and not deployed, and does not yet implement the requested character selection.

**Current local flow:** Google RSS → `classifier` admission → accepted articles only → `classifier_acquire` content acquisition → retained content. Character selection is a later, separate stage and is not implemented for these new admissions.

Admission uses the RSS headline, RSS publisher name, sport and exact canonical query-subject metadata. It asks one native `subject` question with the tested publisher-disambiguation wording, at the unchanged cutoff 55. The publisher may resolve the subject; publisher identity alone does not establish relevance. No article body, publisher fetch, browser operation, emotional score, movement score or character obligation belongs to admission. At least one admitted query subject opens one content-acquisition job for the article. All-negative articles retain their verdict and create no content job. Native errors retain error receipts and retry normally; they are not rejections.

The Go RSS transaction calls `classifier_enqueue_admission`. The existing durable task names stay stable: `classifier` now performs admission; `classifier_acquire` now performs content acquisition only, with no native decision capability. Acquisition consumes the immutable accepted admission ID, validates the current RSS/subject revision, and retains a content revision linked by `admission_source_id`. Changed publisher or canonical subject metadata requires a new request; rank-only revisions reuse valid decisions. Acquisition method settings do not enter admission evidence or its discovery hash. A previous content skip cannot suppress a new relevance revision; the content worker independently preserves its terminal skip/retry budget. Old character/source histories remain readable, while new v8 admission gates have no character routes. Historical work and APIs are retained for compatibility; an old acquisition claim without an admission ID returns to admission before any publisher I/O.

Criteria is `classification-criteria-v8-admission`. The one-question native contract was replayed on the same 39 labeled real cases and six synthetic negative controls: **19 useful admissions, 3 existing false admissions, no useful misses; all six controls rejected**. Removing the unused topic questions retained both St. Pauli and Coventry recoveries. This is selected, provisional development evidence, not production-wide precision. The actual Rust replay matched all 45 qualified request hashes and admissions. The final full suite passed: **387 Rust unit/CLI tests, 57 additional integration checks**, SQL migration/no-op and rating checks, Go formatting/vet/build, prepared-statement validation and race tests. All-target compilation, populated migration/history verification, the explicit legacy-queue bridge regression, release-script checks and the read-only schema verifier also passed. Details are in the [current cleanup receipt](../fixtures/classifier/admission-cleanup-2026-10-18.json).

The existing unpublished migration 313 contains this local change; production is still at 312. Do not treat the old v7 canary or the publisher experiment as an activation receipt for v8. Coordinated matching binaries/migration and an edition boundary are required before a later release. New character work is deliberately deferred, so this is not yet a complete replacement for the production publishing flow.

### Acquisition recommendation and requested flow — October 18

**Keep one shared reader contract, but do not activate the current unbounded browser configuration on these results.** The six-article comparison below favored retaining Jina over custom extraction cleanup. The subsequent normal-sized batch exposed severe memory growth, recorded below. Keep the existing generic `article`/`main` selection and retained-content boundary while resolving that operating limit; do not add an HTTP/browser fallback or duplicate character reads as part of this work.

The [bounded comparison receipt](../fixtures/classifier/content-method-comparison-2026-10-18.json) freezes all six accepted articles from the prior eight-article canary, linked to their successful v8 relevance receipts. The sample includes the recovered St. Pauli and Coventry quotes, three sports and six publishers. Both methods preserved the reviewed key reporting on all six articles and fit the 24 KB intake cap. Quotes, attribution, uncertainty and late paragraphs were reviewed against the retained reference; that reference is not an independent completeness standard.

| Component measurement | Existing strict extraction | Shared Jina reader |
| --- | ---: | ---: |
| Returned bodies within intake cap | 6 / 6 | 6 / 6 |
| Median observed fetch/read plus extraction | 0.61 seconds | 16.09 seconds |
| Maximum observed latency | 6.62 seconds | 20.18 seconds |
| Component HTTP requests | 6 | 7, including one selector fallback |

Extraction duplicated paragraphs on both Sports Illustrated pages; the reader retained clearer paragraph structure and the embedded injury-source quotation. Both methods still included some publisher furniture. In the separate older six-article diagnostic, HTTP returned five extracted bodies, but only four fit the existing cap: one publisher returned 402 and another extraction reached 28,666 bytes versus 17,649 bytes in the live reader response. These different selected samples are not pooled into a production yield estimate.

The reader service was active with zero restarts since October 17 at 22:58 Eastern. Its observed service/cgroup memory was about 7.0 GiB, with a 7.7 GiB peak; Archbox had about 20.5 GiB available. This is a point-in-time cost observation, not a memory-leak diagnosis or durability qualification. [Upstream documentation](https://github.com/jina-ai/reader) describes the browser engine and selector controls; the deployed runtime remains pinned to the version in `ops/reader-runtime.json`.

The comparison made no database writes, native decision calls, character calls or configuration changes. It used direct accepted publisher URLs and isolated local artifacts, requesting cache bypass; browser/CDN/OS warmth was uncontrolled. It did not rerun queue/domain controls, database retention/restart recovery, cold-cache behavior, correction/table coverage, character outputs or sustained daily capacity. This is a practical first-release recommendation under the stated tolerance for missed stories, not completion of the earlier broad qualification protocol.

**Requested implementation target:** during coverage, the decision stage uses discovery evidence to gate relevance and choose downstream characters; accepted articles receive one shared read and retained content version. At the daily cutoff, each selected character receives its fresh JSON package and produces its output. There is no second body-based routing stage and no per-character publisher fetch. Keep publisher context scoped to subject disambiguation: the earlier experiment showed that publisher identity can bias movement scores. The local v8 path currently asks only relevance, so pre-read character selection still needs implementation and qualification. Scout remains measurement-driven.

Read failures retain explicit content dispositions independently of relevance. Preserve the existing resolver, network restrictions, bounded retry budget, source provenance and complete-source limits. Unchanged acquisition keys currently reuse retained text indefinitely; a later edit at the same URL is not guaranteed to refresh. Retained snapshots establish what was read, not that every future correction has been acquired.

### Normal-sized batch and read-once verification — October 18

The [batch receipt](../fixtures/classifier/reader-batch-2026-10-18.json) records a fixed 100-URL sample from the newest persisted discovery hour, matching the default RSS cap, with acquisition's normal four concurrent jobs and two-second domain spacing. This is a cross-target component load test, not a live single-target sweep or a full admission-first queue run. Google resolution and the retained cache used the real Rust fetcher in a separate PostgreSQL database; the browser service was shared with production.

**The batch was stopped for memory pressure.** In 5 minutes 17 seconds to the last completed result, 51 URLs completed: 31 returned bodies within 24 KB and 20 failed explicitly. Forty-nine did not complete. Completed-item median/p95 latency was 22.89/46.05 seconds, including resolution and domain waiting. The proxy recorded 73 completed reader requests, including selector fallbacks; requests still in flight at interruption are not fully counted. These partial results do not establish a full-batch capacity or usable-story yield.

Reader service memory grew from about 7.6 GB to at least 28.6 GB. Swap filled, and available host RAM later fell below 500 MiB while the interrupted browser work continued. Intake was stopped; a reader restart waited on the blocked dependent workers, so the reader processes were forcibly stopped to release memory. The reader and both Classifier workers were restored on the same binaries and reader request settings. Reader memory returned to about 0.75 GB and host available RAM to about 26 GiB. The cause of the memory growth is not yet diagnosed.

After collection resumed, a **runtime-only resource guard** was applied: `MemoryHigh=12G`, `MemoryMax=16G`. The existing restart policy is unchanged. The same two settings are now in the local `ops/scoracle-reader.service` template for a later deployment; the installed service file was not replaced. The runtime override survives service restarts but not a user-manager restart/reboot. This bounds host consumption and can fail an in-flight read at the ceiling; existing retry/skip semantics still apply. It does not fix the growth or qualify capacity.

The existing v8 relevance gate was replayed **after** the load test: 122 subject requests all succeeded and admitted 68 unique articles. Of the completed results, 22 returned bodies and 11 errors belonged to admitted articles; 35 admitted articles were unfinished. This is a posthoc partition, not measured admission-first throughput. A four-body inspection found substantial reporting in three bodies and a historical statistics page in the fourth; relevance and word count alone do not establish useful fresh reporting.

**Read-once behavior is verified.** The batch's 122 target links already share 100 discovery URLs, avoiding 22 potential per-target reads (18%). Across the newest discovery window, 1,905 target links shared 1,457 URLs. This exact-URL deduplication already existed. The local patch in `tools/fetch.rs` closes a separate concurrent-miss race: a PostgreSQL transaction lock keyed by resolved publisher URL and reader version encloses the cache recheck and retention. Eight independent fetchers through two Google aliases made eight reader calls before the patch and one afterward, returning the same document to every caller. No schema change is needed.

A new process replayed all 31 completed successful reads after the browser restart, with an unreachable reader endpoint. All 31 reused their original document IDs, with zero errors, in **9.3 milliseconds total**. Default URL-cache reuse remains six hours; same-article retained-source reuse is separate. Distinct unresolved tracking/redirect keys and syndication across different publisher URLs are not merged. A crash after reading but before retention commits can still cause a retry read.

The pre-existing production stall was also captured: backend 994 held a coverage lock idle in a transaction for over twelve hours, blocking the clock and discovery. Restarting the dependent workers released it. Coverage ticks resumed at 11:43:56 Eastern and new discoveries at 11:44:01. Its underlying cause remains uncorrected. Production is still v6/312; the deduplication patch and new routing release are not deployed.

Validation passed: 385 Rust unit/CLI tests, the two acquisition database checks, the cache-integrity check, all-target compilation, service-template verification on Archbox, formatting and diff checks. The new concurrency regression reproduced eight reads before the fix and one afterward. Inputs, exact bodies, native receipts, wire logs, partial results, cache snapshots and recovery observations remain in the private local and Archbox evidence directories named in the receipt.

Before activation, bound or diagnose reader memory in an isolated reader runtime and retry a complete admission-first batch. Then implement the requested pre-read routes, check usable daily character packages and release matching workers/migration 313 between editions with the existing recovery process. The newly observed coverage stall also needs a permanent fix.

Fresh-context prompt:

> Read the “Current scope and next context — October 18” section of `rust/docs/classifier-reader-rollout-2026-10-17.md` and `rust/fixtures/classifier/reader-batch-2026-10-18.json`. Keep Ponytail lite. The requested process is: decision model gates relevance and chooses downstream users, shared reader reads an admitted article once, selected character plugins receive retained content in their daily fresh JSON. A 100-URL/four-worker load test was stopped after 51 results because the shared browser reader grew beyond 28 GB; services were recovered and a runtime-only 12G/16G memory guard applied. Next, diagnose memory in an isolated reader runtime and rerun a complete admission-first batch. Preserve the local publisher-URL read-once patch, which passed concurrent-alias and persistent-cache checks; it is not deployed. Add no second routing stage or per-character publisher reads. Local v8/313 still needs pre-read character selection and usable daily-package checks. Production remains v6/312. A separate coverage-lock stall was cleared by restart but its cause is not fixed. Do not activate the new release on the partial load-test result.

## See the decisions and data flow

The [editable FigJam board](https://www.figma.com/board/H6DjvzZjQOsDsoNr3R5W6p) contains two diagrams:

1. **Reader decisions:** model comparison → bounded calibration changes → hosted versus local reader → article selection → protected browser access → paired replay → deployment.
2. **Production flow:** RSS URL → acquisition → local reader → publisher → retained article → subject metadata → native d1 → gated obligations → shared fresh context.

The diagram source is retained in [the decisions](classifier-reader-decisions.mmd) and [the data flow](classifier-reader-data-flow.mmd). The compact [rollout receipt](../fixtures/classifier/reader-rollout-2026-10-17.json) records the pins, measurements and deployment checks.

## What changed with the reader

1. **Kept the existing URL discovery and Google resolver.** RSS still supplies the URL and the entities whose queries found it. Successful Google-to-publisher mappings remain reusable. We did not replace discovery or introduce incidental entity fan-out.
2. **Selected the open-source Jina Reader.** The hosted endpoint returned a network reputation 401 from this host. We installed a pinned local runtime instead of adding an external account or API key. This runs a real Chrome browser; d1 itself does not navigate websites.
3. **Inspected what the reader actually returned.** Default page content included navigation and cookie furniture on several sites. We switched to the generic `article` element, with `main` as a fallback only when `article` is missing. This uses no publisher-specific selectors. The fallback covered the sampled site that lacked an article element.
4. **Preserved the headline and returned prose.** Accepted reader output goes directly into the Classifier request without the previous custom scrub pass. Late reporting and corrections survived the canary. Oversized output is rejected rather than silently clipped. Selecting the article does not guarantee that tables, widgets or text outside that element are included.
5. **Retained one shared copy.** We store the URL, final URL, body, content hash, reader version and selected element. Classifier, Influencer, Scout, Insider and Journalist can use this common evidence through the existing retained-source path. This release does not make each plugin browse independently, and does not reduce storage to URLs only. Retaining text preserves the evidence behind a decision when a page later changes or disappears.
6. **Kept the acquisition controls.** Domain spacing, circuit holds, deferred retries, the 48-hour rule for never-acquired reporting, 80-word minimum and 24 KB intake cap remain. Failed reads get an explicit disposition; they do not become negative relevance scores. Legacy extraction remains available when the reader setting is absent.
7. **Protected browser access.** The service binds to loopback. Chrome retains its sandbox. Browser and curl traffic use a local proxy that resolves and pins public addresses, rejects private/reserved destinations and mixed public/private DNS results, and blocks loopback bypass. A rendered private-network canary returned 451 with zero hits on the private destination; a public publisher still loaded successfully.
8. **Separated cache formats and preserved recovery.** Reader text and legacy HTML occupy distinct cache paths by reader version. Migration 312 accepts both old and new receipt shapes without rewriting history. A saved fallback binary retains the prior Classifier behavior plus one guard so it cannot consume reader text as HTML.

The simplification is one shared rendering and content-selection mechanism. Jina still selects text from a rendered page; we have centralized that work rather than eliminated content selection altogether.

## What changed with the decision model

d1 was the strongest model in the earlier matched comparison: 13 errors versus Kev's 39 and Laya's 47 across 156 provisional topic labels. We carried d1 into this release rather than running another open-ended model search.

Each explicit RSS subject now gets an independent request containing the article, sport, canonical `EntityMeta` and its stored identity record. Every question says metadata identifies the subject and that evidence must come from the article. The response must contain exactly four finite probabilities. Requests, replies, hashes, model fingerprint and source revision remain auditable.

| Score | Meaning | Current routing |
| --- | --- | --- |
| Subject | Substantive involvement in the reporting | At least 55 for every route |
| Emotion | Feelings or emotional reactions about the subject | Influencer also needs 15 |
| Movement | Current transfer, trade, signing, departure, negotiation or coaching movement, including rumors and denials | Insider also needs 80 |
| Availability | Injury, illness, fitness, suspension, eligibility, rest, absence or return | Scout also needs 90 |

Journalist requires subject relevance alone, so neutral relevant reporting is retained. Native receipts contain the four topic scores; stored role aliases preserve compatibility with the current Mac character hosts. All subject measurements and obligations publish atomically under the queue claim fence.

## What the measurements showed

The bounded calibration replay reused 18 articles with two subjects each: **36 cases, 144 topic labels**. Cutoffs stayed fixed; false positives and false negatives had equal cost.

| Input or prompt change | Topic errors |
| --- | ---: |
| Prior raw baseline | 26 / 144 |
| Manually selected article prose | 26 / 144 |
| Article prose plus compact examples | 27 / 144 |
| Article prose with identity-only metadata | 31 / 144 |
| Identity-only metadata plus examples | 30 / 144 |

We kept concise questions and full subject metadata. The neutral-reporting diagnostic retained all three relevant neutral cases. These inspected cases are now diagnostic data, not an untouched holdout.

The final live-reader comparison used **14 articles, two subjects each: 28 cases and 112 topic labels**. Google wrapper URLs were excluded from this reader-only sample; the production resolver remains in front of the reader.

| Matched input | Topic errors | Subject false positives | Subject false negatives | Brier score |
| --- | ---: | ---: | ---: | ---: |
| Prior raw text | 20 / 112 | 8 | 0 | 0.1622 |
| Browser reader text | 24 / 112 | 11 | 0 | 0.1655 |

Reader output had 88 correct topic decisions out of 112, or 78.6%. Its 24 errors were subject 11, emotion 5, movement 3 and availability 5. This is a useful first baseline, not a production-wide accuracy estimate. Labels are provisional agent judgments, cases share articles, and the sample was previously inspected. The reader did not beat the paired raw-text input here. Subject false positives are the clearest next calibration target; availability also missed four positive cases.

For browser reliability, article selection succeeded on **26 of 28 reads across two passes**, at concurrency two, in **112.15 seconds**, with a reported median latency of **5.35 seconds**. Both failures came from the site without an article element. An additional `main` fallback read succeeded there, giving observed coverage of all 14 sampled URLs. We did not rerun two complete passes with the final fallback configuration or establish sustained throughput at 8,500 discoveries per day. Content overlap was high on most sampled publishers; a GOAL match page omitted substantial material outside the article element, including widgets/tables.

## Deployment and verification

Only the reader and Classifier changes were overlaid onto a snapshot of Archbox's already-deployed source. The Mac workspace contains unrelated drafts, so it was not deployed wholesale. Archbox runs acquisition and Classifier only; Mac character services were unchanged.

- `scoracle-reader.service`: local Jina on `127.0.0.1:8301`, public-address proxy on `127.0.0.1:8302`.
- `scoracle-kev.service`: existing service name, now loads d1 on port 8081 with four 8,192-token slots.
- `scoracle-classifier-source.service` and `scoracle-cognition.service`: reader-enabled acquisition and subject classification.
- Migration `312_classifier_subject_reader` is applied. The temporary calibration server on 8082 is stopped.

At 22:10 UTC, all four services were active. The latest 20 production spectra, IDs 42237–42256, all had reader provenance, matching subject IDs, four native topic answers, compatible role aliases, and exact equality between retained body and decision input. Coverage was enabled and processing. A live reader/d1 canary also passed in the isolated `classifier_test_reader_20261017` database, with two independently measured subjects and zero character executions.

Focused checks passed: nine Classifier checks including two real PostgreSQL integration tests, two reader tests, four network checks, the cache-integrity database test, proxy checks, release-script checks, schema verification and the Linux binary build. The fallback cache test additionally proved that legacy fetching ignores reader rows. At cutover, the full repository suite and a full production rollback had not been run. The later full-suite result is recorded below; production rollback remains unrehearsed.

## Full suite validation after the checkpoint

At the user's request, the complete `scripts/check.sh` passed on October 17 against a new isolated PostgreSQL 18.6 UTF-8 database. It restored the checked-in schema, confirmed migration no-op behavior, checked SQL rating semantics, ran Go formatting/vet/build, Rust formatting, **390 Rust unit/CLI tests**, **57 additional integration checks**, prepared SQL statement validation, and `go test ./... -race`. `cargo check --all-targets`, ShellCheck and shell syntax, the reader proxy test, release-script checks and calibration self-test also passed.

This exposed two bootstrap issues that were corrected before pushing: CI used a database name rejected by the disposable-database guard, and the schema baseline stopped at migration 290. The baseline was recaptured from a separate clean database with all 314 migration files applied through migration 312. Fresh restoration also needed the coverage singleton, which is runtime state and therefore excluded from the snapshot; `sql/build.sh` now creates it with enrollment disabled in the restore transaction. The complete suite passed from an empty database with these fixes. Production services and enrollment were not changed.

The standard suite excludes live-model, publisher and private-corpus replays. Those were not rerun for this push; the earlier rollout receipts remain their evidence.

Deployment encountered an existing database session idle in a transaction for nearly 18 hours, blocking 46 sessions. After checking the exact PID, start time, last statement and lack of table write locks, we terminated that abandoned transaction. Coverage and DDL resumed without a manual phase override. The postflight found no idle transaction older than one hour. Its original cause remains unresolved.

## Resume tuning in the next session

1. **Start from the live baseline.** Read this document and the rollout receipt. Check the four service states and recent source/decision failures. Do not repeat model selection or deploy unrelated workspace changes.
2. **Build a new labeled sample before changing prompts.** Sample fresh production URLs across publishers, sports, neutral stories, direct subjects, opponents, incidental historical mentions, rumor/denial, availability changes and corrections. Review the reader body against the actual article. Human-review ambiguous labels, especially substantive subject relevance. Split by article, not article-subject row, so related cases stay together.
3. **Separate reading failures from scoring failures.** Record read success, semantic completeness, missing paragraphs/tables, timeouts, oversize skips and latency. Track actual URLs/hour, p50/p95 latency, retry rate and publisher yield over an ordinary acquisition cycle before assigning a durability or throughput score.
4. **Tune the two clearest errors first.** Reduce subject false positives from historical/side mentions without losing directly affected opponents. Review the four missed availability positives. Change one lever at a time; choose cutoffs on development data and evaluate a reserved confirmation set once. Keep equal FP/FN cost unless the user changes it.
5. **Compare against the stored baseline.** Reuse the same reader bodies, canonical metadata and model fingerprint for paired decisions. Report per-topic confusion, Brier score, neutral retention and actual per-role routing, not only an aggregate accuracy number. Further model tests should use this same article-level split and frozen reader input.

The existing article cap can still exceed native context once metadata is added. Unchanged discovery keys reuse accepted text indefinitely; this release does not poll unchanged URLs for corrections. These are explicit limits for later decisions, not reasons to expand this release.

## Headline-only comparison after reboot

Later on October 17 EDT, the user requested a comparison of retained reader output with RSS headlines, using the same four decisions, and explicitly preferred conservative admission. Archbox had frozen; the user rebooted it. All four user services were active afterward. This experiment made inference requests only: no publisher reads, decision publications, character executions, service changes or deployment.

The final [comparison receipt](../fixtures/classifier/headline-comparison-2026-10-17.json) contains the metrics and protocol. Two disjoint samples were replayed, with each reader/headline pair sharing the exact canonical metadata, native d1 fingerprint, four questions and fixed cutoffs:

- The previous diagnostic sample: 14 articles, 28 subject cases, 112 topic labels per condition. Its labels remain provisional agent judgments on previously inspected articles.
- The complete reader-classified cohort at snapshot: 64 articles, 91 RSS subject cases across football, NBA and NFL. Source heads and exact retained reader bodies were verified. This cohort is unlabeled; routing differences are not accuracy measurements.

All **238 final paired requests succeeded**, at concurrency one in shuffled order. A replay fidelity check exposed an important confound: PostgreSQL JSONB key order differs from Rust's recursively sorted `serde_json` object order, and d1 scores depend on that serialized state order. Three probes reproduced archived production scores when keys were sorted. The evaluator now sends sorted keys. All 91 final production reader replays matched their stored threshold decisions and character routes, with a maximum probability difference of approximately `1.1e-16`. Earlier insertion-order experiments remain archived as preliminary evidence and must not be mixed with these final results.

On the labeled diagnostic, both inputs made **21 / 112 topic errors**. Subject relevance improved from 10 false positives to 3, with zero subject misses in either condition. The character consequences differ:

| Character | Useful reader admissions | Useful headline admissions | Reader false admissions | Headline false admissions |
| --- | ---: | ---: | ---: | ---: |
| Journalist | 13 / 13 | 13 / 13 | 10 | 3 |
| Influencer | 5 / 8 | 3 / 8 | 1 | 2 |
| Insider | 3 / 5 | 1 / 5 | 0 | 0 |
| Scout | 2 / 7 | 0 / 7 | 0 | 0 |

Headlines preserve all relevant generalist nominations in this diagnostic but miss secondary topics. Examples include injuries late in general game reports and signing or emotional details beneath a broad headline. The 14-article diagnostic does not cover explicit injury headlines well; the separate production cohort includes those and nominated Scout ten times from headlines.

The unlabeled production routing counts were:

| Character | Reader nominations | Headline nominations |
| --- | ---: | ---: |
| Journalist | 80 | 65 |
| Influencer | 15 | 4 |
| Insider | 10 | 5 |
| Scout | 20 | 10 |

Headline decisions nominated at least one character for 51 of 64 articles, versus 58 with reader text. Total reported model input tokens fell from 409,778 to 83,970. Median request latency was 0.72 seconds for reader text and 0.32 seconds for headlines, under shared production load and prefix caching. These are inference observations, not an isolated throughput or end-to-end acquisition benchmark. Failed and never-acquired publisher URLs are absent from this cohort.

The user proposed removing Scout's article-hunting responsibility: Scout should focus on measurements, with availability an optional aside from the generalist. Omitting Scout routing from the existing four-score diagnostic reduces false admissions from **11 to 5**, while useful admissions fall from **21 to 17**. All four useful non-Scout admissions lost by headline routing remain nominated to Journalist. This is a policy analysis of four-score outputs, not a test of removing the availability question from the model request.

The proposed boundary is headline-based selection for Journalist, Influencer and Insider, with reading available to the consuming plugins; Scout remains stats-driven. Journalist owns availability reporting. If Scout needs an aside, prefer a small dated, source-attributed observation over an article coverage obligation or a news-triggered rating. That handoff does not currently exist: Journalist publishes narratives, while Scout's news-triggered ratings require an applied, source-linked structured record. Scout and Journalist currently have no same-edition dependency, so a mandatory handoff would also change scheduling.

This supports testing the simpler boundary, not claiming equivalent specialist coverage or production accuracy. The next calibration should use fresh source-reviewed headline cases, prioritize false admissions, and explicitly measure Influencer and Insider misses. If the availability question is removed, replay the remaining three questions before release; dropping a question was not part of this comparison. No routing or character behavior was changed.

Private bodies, exact requests/responses, preliminary and final runs, source mappings, evaluator snapshot, comparisons and SHA-256 manifest are retained locally at `/private/tmp/scoracle-headline-comparison-20261017`, with a package at `/private/tmp/scoracle-headline-comparison-20261017.tar.gz`. The final runs are under `canonical/`. Automatic approval review rejected uploading the private package back to Archbox as a separately unauthorized private-data transfer; no new remote archive was created. Focused evaluator self-tests, Python compilation and `git diff --check` passed; the full repository suite was not rerun for this experiment.

## Agreed direction and next implementation

The user accepted headline-first decision-model admission followed by one shared article read, then authorized the five implementation steps below. They are implemented locally as recorded in the next section. This locks the input boundary and read ownership, not a claim that current thresholds are fully calibrated. Production still runs reader-first acquisition and classification.

Keep the RSS headline alongside the URL, publisher, publication time and explicit query subject. RSS already supplies that headline without a publisher fetch. The native decision endpoint receives evidence in its request and does not navigate a URL. An opaque Google wrapper or a publisher slug cannot reliably substitute for the headline. A URL can be the handle passed between plugins, while the shared reader retains one source version internally; this does not require storing a separate body for each character.

Pursue the change in this order:

1. **Qualify the headline admission contract.** Keep d1 and production JSON ordering fixed. Use fresh source-reviewed RSS cases to calibrate conservative subject admission, with false admissions as the primary outcome and recall reported alongside it. Emotion and movement remain specialist-routing candidates; their headline misses are not solved by the shared read.
2. **Reverse the existing two-stage dependency.** Classify discovery evidence before acquisition. Rejects perform no publisher read. At least one admitted subject opens the existing acquisition work; character delivery becomes ready only after its retained article is available. Reuse the reader, resolver, queue claims, cache and network controls instead of adding another service.
3. **Separate decision evidence from character evidence.** A headline receipt must retain exactly what d1 judged. Character loaders must receive the shared article body, rather than the current `classifier_spectra.clean_text` field once that field contains only a headline. Preserve old body-scored receipts and their delivery history.
4. **Keep Scout driven by measurements.** Retire its article-routing obligation as part of the contract change. Journalist owns availability reporting. An optional source-attributed aside for Scout is a separate product choice; do not make Scout's statistics depend on completion of Journalist's reading.
5. **Check the boundary before a small canary.** Prove rejection makes zero reader calls, multiple consumers reuse the same retained article version, read failures remain failures rather than negative relevance, and no character can publish from headline-only evidence accidentally. Recheck subject identity, source revision and publication fencing, plus pending obligations across the transition. Measure reader requests, specialist usefulness and character-model calls before broad rollout.

The unresolved dispatch decision is how Influencer and Insider are selected after admission. Headline topic scores can lose useful secondary material; invoking every specialist on every article can increase generation work. Test that tradeoff on the same admitted articles before choosing a permanent rule. Shared acquisition and retained evidence are useful under either policy.

## Headline-first implementation and isolated canary

The five steps are implemented locally. The [canary receipt](../fixtures/classifier/headline-first-canary-2026-10-17.json) records the new three-question bank, model pin, routing results, reader calls and actual character completions. No production migration, binary deployment, enrollment or model replacement occurred.

The path is now **RSS headline decision → one shared article read when admitted → character publication**. To keep the change small, the headline gate runs at the start of the existing `classifier_acquire` claim. That claim retains native success/error receipts before publisher work and reuses valid decisions on retry. An all-negative batch completes as `headline_rejected` with no reader call. The existing `classifier` claim validates and publishes the retained batch; it never scores the acquired body a second time. Read failures keep their separate bounded retry/skip outcomes.

Criteria v7 has subject, emotion and movement questions, with the existing cutoffs 55/15/80. Availability routing is removed. Each decision receipt retains `state.article.headline`; the character loader receives the linked shared `Source.body`. Routing provenance declares whether scores describe a headline or a historical article. Character instructions reflect that distinction. RSS keeps the headline and URL; d1 cannot navigate a URL to obtain missing evidence.

Scout no longer loads article context or creates news-triggered ratings. Historical classifier-triggered Scout claims complete without a model call; normal ratings still use their frozen edition measurements. Journalist receives admitted availability reporting. There is no new Journalist-to-Scout dependency or automatic aside.

Migration 313 preserves currently valid historical body-scored deliveries across the discovery-hash change, leaves already-stale revisions stale, and holds pending Scout obligations without deleting history. New headline deliveries require an admitted route and a nonempty retained article; the guard also applies to delivery reactivation. Migration does not enqueue a corpus-wide replay. The source worker now requires `CLASSIFIER_URL`; the publication worker does not. Both live Archbox processes already have the reader and decision endpoint variables, verified by presence only. Frozen Scout prompts require deploying between character editions.

The new three-question bank was replayed on the same frozen diagnostic with exact canonical metadata and production JSON ordering. All **28 requests succeeded**, preserving the prior headline routing: Journalist 13 useful admissions, 3 false admissions and no useful misses; Influencer 3 useful, 2 false and 5 useful misses; Insider 1 useful, no false and 4 useful misses. These are previously inspected provisional labels, not a holdout or a claim of perfect precision.

The live canary selected **8 different real corpus articles / 11 subject cases** with provisional source-reviewed labels. Its isolated database contained minimal canonical entities; a further **11 native requests with exact archived production metadata reproduced every canary routing outcome**, including the two misses.

| Character | Useful headline admissions | False admissions | Useful misses |
| --- | ---: | ---: | ---: |
| Journalist | 4 / 6 | 0 | 2 |
| Influencer | 0 / 2 | 0 | 2 |
| Insider | 2 / 2 | 0 | 0 |

The misses were useful St. Pauli and Coventry coach-quote headlines naming Rapp or Lampard without naming the club. Lowering the cutoff was not tested or adopted. Conservative headline routing is working as implemented, but club identification from an unnamed coach remains a measurable recall limit. The shared read cannot recover an article rejected before reading, or nominate a specialist that did not pass its headline gate.

Four articles were admitted and four rejected. The canary made exactly **4 reader HTTP requests for 4 distinct URLs**, all successful: half the calls required to read all eight. Shared body assertions passed for every loaded character context. This small selected sample is not a daily throughput or savings estimate; publisher wire requests inside Jina were not instrumented.

The four articles produced six obligations: four Journalist and two Insider. Exactly **4 character completion HTTP requests** occurred: three Journalist and one Insider, all parser-accepted and published only in the isolated database. The Jazz story used the shortened team name and failed the existing exact canonical target guard; its Journalist and Insider obligations were held, not sent to a model. Influencer and Scout made zero calls. Parser acceptance is not an independent factuality qualification.

Validation covers all-negative zero-read behavior, multiple subjects, full-body sharing, cached decisions, read/fallback failures, native errors, metadata revisions, stale claims and prohibited headline-only deliveries. A separate populated-database migration check preserved historical decisions and pending Journalist obligations, held Scout obligations and kept stale revisions stale without enqueueing acquisition. The final `scripts/check.sh` passed from a new empty PostgreSQL 18 database: **387 Rust unit/CLI tests, 57 additional integration checks**, SQL baseline/migration/no-op and rating checks, Go format/vet/build, prepared-statement validation and `go test ./... -race`. `cargo check --all-targets`, release-script mock checks, the read-only schema verifier, calibration self-tests, shell syntax and diff checks also passed.

Private inputs, labels, raw native receipts, wire counts, character products, a database snapshot and the temporary canary runner are retained at `/private/tmp/scoracle-headline-implementation-20261017`. Publisher bodies remain outside the repository. Before broad activation, expand source-reviewed calibration around unnamed-club quotes, canonical-name holds and specialist misses. Test cheap RSS publisher context for those ambiguous headlines as a separate change; do not infer that it fixes them.

## RSS publisher comparison on October 18

The user requested a matched test of adding the RSS publisher beside the headline, specifically to recover the St. Pauli and Coventry misses without admitting more irrelevant articles. The [publisher comparison receipt](../fixtures/classifier/headline-publisher-comparison-2026-10-18.json) records the conditions and results. This was an inference experiment only: **576 successful native requests**, zero publisher reads, zero character calls and zero production database writes. Runtime code and production configuration were unchanged.

The main sample comprised the same **39 labeled article-subject cases across 22 articles**: the 28-case diagnostic plus 11 canary cases. Another **80 unlabeled cases across 56 articles** checked routing changes, and six separate synthetic controls paired a club-associated publisher with a clearly unrelated headline. RSS names came from `news_articles.source`, with diagnostic titles verified unchanged on Archbox; production names came from the verified retained discovery snapshots. The d1 fingerprint, canonical metadata, sorted JSON ordering, three questions and 55/15/80 cutoffs stayed fixed in the primary field-only comparison. Follow-up wording and evidence-location changes are recorded separately; labels remain provisional and these are inspected development cases, not an independent holdout.

| Relevance input | Useful Journalist admissions | False Journalist admissions | Useful misses |
| --- | ---: | ---: | ---: |
| Headline | 17 / 19 | 3 | 2 |
| Headline plus publisher field, unchanged questions | 18 / 19 | 3 | 1 |
| Publisher field with explicit subject-disambiguation wording | 19 / 19 | 3 | 0 |

Adding the field alone recovered St. Pauli, raising subject relevance from 5.94 to 66.33. Coventry rose from 39.67 to 51.92, below 55. Explicitly permitting the publisher to disambiguate the subject recovered both: **70.10 for St. Pauli and 56.20 for Coventry**. Wording alone, without the publisher, recovered neither. No new false Journalist admission or lost useful admission appeared in the frozen labeled cases, and all six synthetic controls remained rejected. The three existing false Journalist admissions were not corrected.

There is a specialist cost if all three questions share publisher evidence. Of the two additional changed routes in the unlabeled sample, one was a useful Journalist admission: a broad fantasy column contained current 49ers availability reporting. The other was a false Insider nomination: `NFLTradeRumors.co` pushed a Bengals article's movement score from **24.36 to 84.28** although its Bengals section discussed a prior draft selection and an overturned celebration fine, not a current move. These two bodies were reviewed after observing the changes; their reviews stay separate from the frozen metrics.

An instruction telling the emotion/movement questions to ignore the publisher still admitted that false Insider route. Moving publisher data into only the subject question removed the false Insider nomination, but also lost both target recoveries; that structural candidate was stopped after three qualifying requests. No threshold was lowered and no further wording search was pursued.

A composition of **two measured input contexts** did meet the observed routing objective: use publisher-disambiguated subject relevance, and preserve headline-only emotion/movement scores. It retained all 19 useful Journalist cases with the same three false admissions, gained the useful Coventry Influencer nomination, introduced no new false character admission in the 39 frozen labeled cases, and avoided the false Bengals Insider nomination. St. Pauli still did not reach the emotion cutoff, so its recovery was Journalist-only. All six synthetic controls remained rejected. The only additional Journalist route in the 80-case unlabeled sample was the source-reviewed 49ers case; there were no additional specialist routes under this composition.

The paired outputs also support a smaller conditional policy: retain every initially admitted headline result; retry only a rejected subject with publisher context, replacing only its subject score. It produced the same observed routes across all 125 real/synthetic cases. On the **119 real subject cases**, that would mean **119 initial requests plus 38 additional requests**, approximately 32% more decision calls, rather than doubling every request. This is a posthoc composition of actual full three-question requests, not an implemented queue path. Reducing those request banks to one and two questions has not been tested.

Twenty-four further requests repeated both input contexts three times on the four changed real cases; all route decisions remained identical. These repeats check serving stability, not independent accuracy. The research request-builder, response validators, routing metrics and conditional composition checks passed; Python compilation and diff checks passed. The repository runtime suite was not rerun because this turn changed only experiment receipts and documentation.

Publisher context is promising for admission, but a single shared context for all topics has a measured unwanted Insider route. No new production policy was selected or deployed. The two-context alternative needs an explicit decision about the extra inference calls and implementation of receipts, caching and failure handling before use. Private requests, responses, protocols, scripts, source-review notes and a SHA-256 manifest are retained at `/private/tmp/scoracle-headline-publisher-20261018`.

## Operational handoff

| Item | Location or value |
| --- | --- |
| Production repository | Archbox `/home/sheneveld/scoracle/scoracle-backend` |
| Reader runtime | `/home/sheneveld/.local/share/scoracle-reader` |
| Reader configuration | `ARTICLE_READER_URL=http://127.0.0.1:8301/` in both Classifier worker drop-ins |
| Model configuration | `/home/sheneveld/.config/scoracle/classifier-model.env` |
| Model | `LiquidAI/d1-3B-GGUF@sha256:16aff27ea2eefdc32b9897f43854a5d3170c1dc8dccb9c756905af30a4e22402` |
| Production binary SHA-256 | `6b4b1c9f670ac9eff0ec50d90bcf6cefb84b778fb29bb2d3bae98869ea56d7b4` |
| Reader upstream | Jina `1574bfd380d249c86c82db4dace0d9c8fe17e2b1`, Node 22.23.3, Chrome 147.0.7727.57, Koffi override 2.16.3 |
| Reader policy / criteria | `jina-content-v1` / `classification-criteria-v6-subject` |
| Recovery bundle | `/mnt/data/backup/scoracle/reader-rollout/2026-10-17` |
| Private experiment archive | `/mnt/data/backup/scoracle/classifier-calibration/2026-10-17-reader-rollout` |

The recovery bundle contains the original binary/configuration/source snapshot, activation and postflight receipts, plus `rollback-cognition` with the cache compatibility guard. Inspect recovery without changing services:

```sh
ssh archbox 'python3 /mnt/data/backup/scoracle/reader-rollout/2026-10-17/rollback.py'
```

Adding `--apply` restores the previous Kev/v5 behavior and disables reading; it retains historical receipts and migration 312. This script has been syntax-checked and dry-run; it has not been executed against the live release. It refuses to overwrite a newer binary.

Private articles, raw responses, label files, protocol, evaluator, reader benchmarks, source URL mappings, source overlays and validation logs are retained in the archive. Earlier calibration archives remain unchanged. In `boundary-audit.json`, the legacy field named `source_id` means **classifier spectrum ID**; `source-urls.jsonl` contains the verified mapping to real article IDs. Do not treat those spectrum IDs as article IDs.

Useful code entry points: `src/tools/reader.rs`, `src/tools/fetch.rs`, `src/harness/tools.rs`, `src/plugins/classifier/acquisition.rs`, `adapter.rs`, `decision.rs`, `fixtures/classifier/classification-criteria-v6-subject.json`, `examples/relevance_calibration.py`, `examples/relevance_meta.rs`, and `ops/`.

The fresh-context task and starting prompt are in the current scope section at the top. Earlier v7 implementation steps and publisher-routing experiments below it remain historical evidence, not the current character-selection policy.

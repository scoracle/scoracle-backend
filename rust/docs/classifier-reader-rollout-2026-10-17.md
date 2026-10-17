# Shared article reader progress and calibration handoff

The shared URL reader and d1 Classifier are live on Archbox as of October 17, 2026, 21:56 UTC. Acquisition browser-reads a publisher URL once, retains the returned article, and Classifier scores that same text for each RSS subject using real `meta.rs` context. Existing fresh-context consumers receive the retained text through their current source contract. No character implementation was changed or deployed in this work.

The next session should tune this deployed baseline using new labeled articles. We have stopped prompt exploration for this release. Accuracy remains provisional; the reader is a simpler shared acquisition mechanism, and has not yet demonstrated an accuracy improvement.

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

Focused checks passed: nine Classifier checks including two real PostgreSQL integration tests, two reader tests, four network checks, the cache-integrity database test, proxy checks, release-script checks, schema verification and the Linux binary build. The fallback cache test additionally proved that legacy fetching ignores reader rows. The full repository suite and a full production rollback were not run.

Deployment encountered an existing database session idle in a transaction for nearly 18 hours, blocking 46 sessions. After checking the exact PID, start time, last statement and lack of table write locks, we terminated that abandoned transaction. Coverage and DDL resumed without a manual phase override. The postflight found no idle transaction older than one hour. Its original cause remains unresolved.

## Resume tuning in the next session

1. **Start from the live baseline.** Read this document and the rollout receipt. Check the four service states and recent source/decision failures. Do not repeat model selection or deploy unrelated workspace changes.
2. **Build a new labeled sample before changing prompts.** Sample fresh production URLs across publishers, sports, neutral stories, direct subjects, opponents, incidental historical mentions, rumor/denial, availability changes and corrections. Review the reader body against the actual article. Human-review ambiguous labels, especially substantive subject relevance. Split by article, not article-subject row, so related cases stay together.
3. **Separate reading failures from scoring failures.** Record read success, semantic completeness, missing paragraphs/tables, timeouts, oversize skips and latency. Track actual URLs/hour, p50/p95 latency, retry rate and publisher yield over an ordinary acquisition cycle before assigning a durability or throughput score.
4. **Tune the two clearest errors first.** Reduce subject false positives from historical/side mentions without losing directly affected opponents. Review the four missed availability positives. Change one lever at a time; choose cutoffs on development data and evaluate a reserved confirmation set once. Keep equal FP/FN cost unless the user changes it.
5. **Compare against the stored baseline.** Reuse the same reader bodies, canonical metadata and model fingerprint for paired decisions. Report per-topic confusion, Brier score, neutral retention and actual per-role routing, not only an aggregate accuracy number. Further model tests should use this same article-level split and frozen reader input.

The existing article cap can still exceed native context once metadata is added. Unchanged discovery keys reuse accepted text indefinitely; this release does not poll unchanged URLs for corrections. These are explicit limits for later decisions, not reasons to expand this release.

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

Suggested next-session instruction: “Read docs/classifier-reader-rollout-2026-10-17.md and the rollout receipt. Keep the live reader/d1 baseline, build a fresh article-level calibration sample, and focus on subject false positives and missed availability signals. Measure reader completeness and sustained acquisition throughput separately. Stay out of character implementation work.”

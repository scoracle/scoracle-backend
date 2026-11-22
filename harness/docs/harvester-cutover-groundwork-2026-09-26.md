# Harvester cutover groundwork — September 26, 2026

> **October 7 retirement:** This is a historical experiment record. Its binary annotation/scoring, trace-manifest and Fastino adapter tools were removed for the [Classifier launch](scoracle_classifier_plugin_launch.md). Commands below describe the old experiment; their source is recoverable from commit `5c956402`.

Status: historical Phase 0 evidence and annotation groundwork. The active cutover
contract is packet-free `harvest-context-v1`; Laya decisions are advisory and all
acquired news stays eligible for character review. Stories and Editor are both
retirement targets. No roster enrollment or production migration has occurred.
[The updated cutover plan](PLAN-harvester-production-cutover.md) is authoritative.

## Durable evidence

`fixtures/harvester/cohort-20260926-publisher-first.index.json` preserves the
84-candidate corrected trace's IDs, query entities, source/request hashes,
uncalibrated distributions, character assignments, extraction byte ranges,
checkpoint and tokenizer file hashes, runtime versions, and aggregate counts.
The sibling `cohort-20260926-rss-first.index.json` preserves the failed evidence
contract distinctly. Both include SHA-256 of their original trace bytes.

These indexes intentionally contain no publisher headlines, openings, full bodies,
model request text, or generated character prose. They are historical evidence
indexes, not substitutes for a licensed source archive. The retained byte counts
and storage verification flags describe the original run; the reducer does not
independently reconstruct unavailable publisher bodies. The original 94 raw Google
rows and ten removed duplicate edges are not present in these 84-row traces.
Do not claim complete duplicate-provenance replay from these indexes.

Reproduce a reduction while the ignored traces exist (choose a new output path):

```sh
python3 examples/harvest_evidence_manifest.py logs/harvest-focus-20260926/cohort-v2-publisher-first.json /tmp/harvest-publisher-index.json
```

Offline checks do not depend on ignored logs:

```sh
cargo test --lib plugins::harvester
python3 -m unittest discover -s examples -p 'test_harvest*.py'
```

Rust tests exercise synthetic source text and deterministic provider responses.
They verify contract behavior, not Laya's semantic quality. Compilation now rejects
article changes after classification, mismatched prompts, and arbitrary byte-valid
selections that violate the extraction contract. JSON round trips preserve Unicode
headlines and UTF-8 byte offsets. Malformed distributions, missing coverage,
truncation, and provider failures remain errors.

## Historical packet meaning (replay only)

The historical replay format remains `harvest-v3`, with question versions
`harvest-relevance-v3` and `harvest-character-routing-v3`, and policy
`google-laya-character-cascade-v1`. This work tightens validation without changing
valid packet shape, prompts, or decisions.

- `article_id`, `source`, `url`, `published_at`, `headline`, `hypothesis`, and
  `retrieval` identify the supplied article/query and Google rank/description.
  Rank is retained as supplied; historical null ranks are not invented.
- `body_hash` is SHA-256 of the retained UTF-8 publisher body. `model_input_excerpt`
  is a bounded 100-word/1,200-byte publisher prefix. `excerpt` and `context` hold
  the exact first three mechanically detected sentences on acceptance, otherwise
  null. Byte ranges are half-open into that same body. The headline is a separate
  unchanged field, not a byte range into the body.
- `requests`, `input_hashes`, `question_set_versions`, `provenance`, and
  `raw_responses` retain model inputs, versioning, and provider evidence.
  `input_hashes` use the existing truncated SHA-256 `hash_components` function;
  they must not be confused with the full body or trace SHA-256.
- `signals` retains complete choice distributions. The historical field name
  `character_relevance_probabilities` means uncalibrated distribution values.
- `disposition` is accept or reject for successful classifications only.
  Acquisition/classification failures belong to the outer run state and cannot
  be encoded as reject. `character_tags` are stable advisory manifest IDs.
  Analyst and Oracle receive no tags.
- `plugin_id` is added by the Harvester wrapper. No packet claims a fenced database
  commit, final character acceptance, or public publication.

Historical provenance is preserved as recorded. Future evaluations still need a
policy that validates expected checkpoint/tokenizer identity before execution;
nonempty provider revision metadata alone does not enforce a pinned checkpoint.

## Annotation contract v1 and split policy

`cohort-20260926-annotation-queue.jsonl` contains 84 source-bound review tasks.
All five labels are null and no row is gold. Seven acquisition failures remain
unlabelable until successful acquisition creates new source hashes. The queue
excludes Laya predictions so annotators can review evidence independently; the
separate audit index retains predictions for later comparison.

`cohort-20260926-ai-provisional.jsonl` is a separate draft produced by Codex from
the prediction-free reviewer packet. It retains all 84 rows: 71 have provisional
source-grounded judgments, 13 remain pending, including all seven acquisition
failures. Six acquired articles had openings too thin or cluttered for a confident
provisional call (IDs 774287, 774301, 774310, 774313, 772385, 769234).
For negative entity judgments, character dimensions remain null
until independently judged. The draft is deliberately unassigned to any data
split and has zero trainable gold rows. Its `ai_provisional` status cannot enter
a train/evaluation split in the validator. A human must independently review
the remaining materialized articles, correct or reject draft judgments, group
syndicated stories, and adjudicate each of the five dimensions.
The draft was not assembled as a blinded benchmark: earlier exploratory notes
already identified broad categories of Laya errors. It must not be used to
estimate model quality or choose thresholds.

`cohort-20260926-disagreement-priority.json` lists 28 entity decisions where
this draft and the historical Laya outcome differ: five apparent misses and 23
apparent over-admissions. It contains IDs and paraphrased reasons without source
text. These are a human review queue, not measured false-positive/negative counts;
the cases with provisional calls were selected for clarity, not sampled randomly.

`examples/harvest_annotations.py` validates the queue against the exact trace
SHA-256 retained in the evidence index. It requires all 84 canonical candidates,
keeps acquisition failures unlabeled, validates completed dimensions and UTF-8
evidence spans against the exact retained headline/publisher openings, and rejects
story groups split across train/evaluation partitions. It requires a distinct
human adjudicator before a row can enter a training/evaluation split. These checks
do not infer labels from Laya signals.

The optional `--candidates` check verifies all 24 complete publisher bodies still
present in the pre-replay export: body hash, headline, and byte ranges. The
remaining 53 acquired bodies are unavailable as complete local source text. Their
retained openings are hash-checked against the trace and evidence index, but full
body replay remains open. To audit the local cohort and create a review packet:

```sh
python3 examples/harvest_annotations.py \
  --trace logs/harvest-focus-20260926/cohort-v2-publisher-first.json \
  --index fixtures/harvester/cohort-20260926-publisher-first.index.json \
  --queue fixtures/harvester/cohort-20260926-annotation-queue.jsonl \
  --candidates logs/harvest-focus-20260926/candidates.jsonl \
  --review-output logs/harvest-focus-20260926/annotation-review-v1.jsonl
```

The review packet contains source excerpts and must be written to a path ignored
by Git. It has 77 publisher openings and seven acquisition-error placeholders.
It does not include model predictions. Its opening text is sufficient to label
only what that opening supports; annotators should leave uncertain dimensions
null rather than infer facts from unseen article text. The local packet remains
ignored by Git and is not the durable licensed source archive Phase 0 needs.

Required row fields are `schema_version`, `cohort_date`, `article_id`,
`query_entity`, `source_state`, `body_sha256`, `headline_sha256`, `story_group`,
`split`, `review_status`, `annotator`, `adjudicator`, and `labels`.

The five independent dimensions are `entity`, `journalist`, `influencer`,
`insider`, and `scout`. Null means unreviewed or unknown, never false. A completed
dimension uses this shape:

```json
{"useful": true, "reason": "The opening directly reports this team's selection decision.",
 "evidence": [{"field": "body", "start": 12, "end": 84}]}
```

Offsets refer to exact UTF-8 bytes in the hash-bound body or headline. Every positive
requires at least one verified span; every completed decision requires a concise
reason. Evidence from a title alone cannot replace publisher acquisition. Character
labels assess useful evidence for that perspective, not whether the real adapter
currently has sufficient resolved identity, roster, or measured statistics to run.
Negative entity decisions must not mechanically populate four negative character
labels. Ambiguity remains null with review notes until adjudication.

Before any training export:

1. Obtain authorized source access and verify each source hash. Review grouping
   across all entities, duplicate URLs, syndicated variants, and nightly dates.
2. Assign an explicit story group, then assign the entire group to train,
   calibration, or test. `unassigned` rows are not trainable. Do not use article ID
   hashing as a substitute for story grouping.
3. Reserve a later nightly date as the untouched temporal holdout before tuning.
   Keep groups crossing its boundary out of earlier training/calibration data.
4. Record annotator and human adjudicator identities. Only adjudicated rows can
   become gold. Sample apparent model errors and ambiguities for human review.
5. Agree recall, precision, fan-out, latency, and Granite-cost budgets from the
   labeled baseline. No numeric thresholds or cohort-size policy is ratified here.

This is a documented annotation contract and provisional queue, not a completed labeled
set or a training exporter. Independent reviewers, authorized complete source
access, adjudicator assignment, fresh cohorts, and a temporal split remain
outstanding. The validator accepts the documented queue shape and emits a
trainable-gold count of zero until those requirements are met.

## Editor dependency inventory

This is a static source/schema audit, not evidence that deployed objects match the
checkout. Future owners below are proposals, not completed migrations.

| Obligation | Current evidence | Proposed owner / remaining proof |
|---|---|---|
| Fetch, full text, failures, fetch ledger, exact claim completion | `src/plugins/editor/adapter/mod.rs`: `prepare`, `commit_claimed`, `persist_read`, `persist_terminal`, `insert_data_fetch_ledger` | Harvester acquisition and claim-fenced publication; prove success/error/retry parity |
| Unknown-person nominations and candidate reopening | `src/plugins/editor/adapter/candidates.rs`: candidate/mention writes and Investigator enqueue | Explicit identity handoff; retain descriptor/mention evidence and idempotency |
| Authoritative entity links and team links | `src/plugins/editor/adapter/mod.rs`: `write_links`, `harvest_team_links`; `resolve.rs` | Resolved identity writer; Laya routes must never create links |
| Result-line fixture nomination | `src/plugins/editor/adapter/nominate.rs`; SQL migration 233 | Confirm deterministic fixture owner; the retired fixture-to-Boxscore trigger cannot be assumed to deliver this work |
| Graph handoff and Graph input | `enqueue_graph_for_article`; `src/plugins/graph/adapter/mod.rs` joins `editor_reads` | Migrate both atomic handoff and reader contract |
| Storyline membership and lifecycle | `src/plugins/editor/adapter/storyline.rs`; `maintenance.rs` | Retire with stories after removing consumers |
| Packet compilation | `src/evidence/news/packet.rs` joins `editor_reads`; Editor maintenance invokes `compile_dirty` | Retire; characters consume verified source context directly |
| Personnel evidence | `src/evidence/personnel.rs` requires successful `editor_reads` | Audit Insider/Scout evidence parity before retiring successful reads |
| Journalist continuity and shared memories | `src/plugins/journalist/adapter/mod.rs`; `src/evidence/memories/stories.sql` | Preserve or replace storyline membership and source continuity |
| **Week sealing** | `src/plugins/editor/adapter/maintenance.rs` calls `public.seal_weeks`; SQL migration 241 | Explicit lifecycle scheduler; test closing enqueue and sealing even when Editor is disabled |
| **Exact-title dedup maintenance** | Same module calls `collapse_exact_title_duplicates`; SQL schema canonical preference reads `editor_reads` | Acquisition/dedup owner plus revised canonical preference; retain intended cross-source semantics |
| Stories API | `../go/internal/api/server.go`, `../go/internal/db/db.go`: storyline list/detail, packet history, article and product pointers | Retire routes after external client audit |
| Operations coverage | `../scripts/hosting/cron-watchdog.sh` computes swept-team coverage using `editor_reads` | Replace monitor with equivalent Harvester coverage before retiring reads |

The checked-in SQL schema also contains foreign keys and functions referencing
`editor_reads`/storylines. A live trigger/view/function catalog comparison, scheduled
job inventory, external dashboard audit, and fenced database integration tests are
still required. No obligation above has been moved by this groundwork.

## Remaining Phase 0 gate

Preserve authorized replay sources or add source-safe representative fixtures for
reviewed false positives/negatives, canonical duplicate provenance, multi-entity
stories, and blocked acquisitions. Obtain human semantic judgments rather than
asserting model quality with stub responses. Verify actual storage round trips
once the storage owner is chosen; current Rust checks cover JSON serialization.
The pending calibration decisions and later production gates remain open.

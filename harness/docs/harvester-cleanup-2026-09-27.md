# Harvester cleanup — September 27, 2026

> **October 7 retirement:** This is a historical experiment record. Its binary annotation/scoring, trace-manifest and Fastino adapter tools were removed for the [Classifier launch](scoracle_classifier_plugin_launch.md). Commands below describe the old experiment; their source is recoverable from commit `5c956402`.

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

Retention rule: keep a component only when it has a concrete current consumer or
preserves evidence/recovery obligations. This pass follows the
[routing review](harvester-routing-review-2026-09-27.md). It changes local code and
tooling, not deployed services or production data. Routing calibration remains open.

## Active path

`headline gate → publisher acquisition → bounded theme scores → plugin eligibility → source receipts → harness queue`

`policy::CHARACTER_ROUTES` now connects each theme to its consumer's registered
manifest and delivery setting. Cognition, context validation, delivery-setting
validation and publication share it. One publication loop replaces the four
character enqueue branches and the separate theme-to-delivery-name match. Stable
plugin IDs and queue tasks come from the destination manifests. The plugin still
owns eligibility and source receipts; the application queue owns durable dispatch.

Removed the unused RSS description from `Article`, the worker's source query and
the three corpus exports. It remains ingestion provenance in Postgres; Harvester
does not need to load or copy it. Existing corpus files with that extra field still
decode without using it. Retained feed rank is used by the context/provenance contract.

Removed the duplicate `passed_relevance` policy wrapper and redundant model-identity
checks after gate verification. Saved and freshly returned signals still pass the
same distribution, coverage, source and checkpoint checks. Typed comparisons replace
JSON serialization used only to compare hypotheses, requests and excerpts.

No inference text, question ID, probability threshold, source budget or receipt
format changed. The active versions remain context v6, headline gate v2, headline
questions v2 and theme questions v5. Experimental boolean predicates are not shipped
as a second classifier path.

## Removed tools

Nine obsolete files, 1,077 lines, plus the retired comparison test:

| Files in `examples/` | Reason there is no current consumer |
| --- | --- |
| `harvest_store.py` | SQLite packet archive duplicates durable source receipts and consumes a retired replay schema. |
| `harvest_decider_server.py` | Alternative classifier experiment; the live launcher uses the Laya server. |
| `harvest_laya_bench.py` | Experimental batched inference for the retired entity/content request contract. |
| `harvest_training_data.py`, `harvest_laya_train.py`, `harvest_laya_evaluate.py`, `test_harvest_training.py` | Training/evaluation stack for retired entity/content labels; incompatible with current requests. |
| `harvest_provisional_agreement.py` | Historical comparison expects `advisory_characters`; current replay emits policy-selected destinations. No current calibration consumer. |
| `harvest_smoke.rs` | Always invokes Journalist regardless of routing, uses a stale model default and duplicates the actual worker integration boundary. |

Reference scans covered Rust callers, examples, operational scripts, docs and
fixtures. Surviving references to removed tools are historical records, now marked
accordingly. No compatibility shims or replacement experiment framework were added.

## Retained components and consumers

| Component | Why it stays |
| --- | --- |
| Cognition, context, policy and worker adapter | The one active preparation, classification and source-publication path. |
| `delivery::load_for_character` | Journalist, Influencer, Scout and Insider consume verified pending source receipts. |
| V1–v5 pending-receipt handling | Existing receipts need completion or explicit supersession before historical integrity rules can be removed. |
| Acquisition provenance, claim fences and idempotent receipts | Prevent changed evidence, duplicate publication and stale workers from producing effects. |
| Identity/link/pair/wrap obligations | Graph, Investigator and Insider still consume these source-bound effects. Their later alignment windows own replacement/removal. |
| Maintenance | Exact-title deduplication and week sealing are active scheduled jobs. |
| `harvest_laya_server.py` | Called by `scripts/hosting/run-harvester-laya.sh`; actual local classifier endpoint. |
| `context_harvest`, `harvest_shadow`, three SQL exports | Current-contract request inspection and replay; full context or text-free summaries as needed. |
| Headline review, calibration and scoring tools | Current headline policy still needs reviewed calibration. |
| Acquisition/Chrome diagnostics and operational SQL/checkers | Acquisition failures, bounded release, held delivery and recovery remain operational concerns. |
| `harvest_annotations.py`, `harvest_evidence_manifest.py` and their tests | Validate the retained September 26 source-bound historical evidence; do not implement production intake or current training. |
| Historical reports and source-free label/index fixtures | Preserve what was measured and its limitations without keeping obsolete runtime paths. |

## Verification

- Rust library: **580 passed, 77 ignored**. Environment-dependent tests are counted
  separately below.
- Focused Harvester unit checks: **19 passed**.
- All **four Harvester database integration tests passed** serially against a new
  disposable local PostgreSQL database restored from the checked-in schema. The
  added selective-delivery test verifies held-only, mixed selected/enabled, and
  zero-selected cases; enabling a consumer cannot manufacture semantic eligibility.
  Existing tests cover shadow mode, stale claims, idempotency, exact source delivery,
  attribution/body drift and downstream handoffs.
- Remaining archival Python tests: **8 passed**.
- `cargo check --offline --all-targets` passed; removed public API/tool callers do
  not leave a build failure.
- All three SQL exports passed synthetic canonical-ID, distinct-entity, best-rank
  and lean-payload checks. Neither RSS descriptions nor teacher labels are exported.

These are implementation and integration checks, not evidence that the current
Laya theme frame is calibrated. Resolve transfer/performance misses and evaluate a
frozen candidate on independent reviewed cases before changing model questions or
releasing delivery. The [routing review](harvester-routing-review-2026-09-27.md)
records that outstanding work.

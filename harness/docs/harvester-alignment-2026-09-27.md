# Harvester alignment — September 27, 2026

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

Phase one is implemented on top of backend `18602b23`. Repository checks and a subsequent [archbox real-source smoke](harvester-alignment-smoke-2026-09-27.md) passed; this is not a deployment. The governing responsibility is **plugins build the decision world; System 1 supplies bounded probability signals**. Harvester has no language product and makes no SmolLM3 call.

## Responsibility map

| Concern | Owner and enforced boundary |
| --- | --- |
| Context | Harvester selects the query entity, exact headline, publisher attribution, and retained opening. RSS descriptions remain retrieval provenance. |
| Tools | Application injects `DecisionModel` and the scoped web broker. Harvester grants classification and curated-article fetch only; no generative route. |
| Structure | Plugin-defined relevance and four theme predicates; typed article, headline gate, excerpts and context. No model-authored evidence or packets. |
| Memory | Postgres retains query provenance, acquisition attempts, body/hash, gates, classifications, assignments and dispositions. No editorial continuity enters Laya. |
| Voice | None. Downstream characters own language products in later alignment windows. |
| Boundaries | Harvester applies reading/routing thresholds, validates distributions and coverage, binds source identity, and publishes under the exact claim. Characters receive source text, attribution, time and receipt IDs, without classifier audit payloads. |

## Changes and pruning

Worker and replay now share `context::{classify_headline,classify_after_headline,classify}`. Removed the standalone `Harvester`, `Classification`, `build_harvester`, historical packet compiler/constants, and their duplicate tests. Migrated relevant source/protocol invariants to the surviving context tests. `prepare_relevance` constructs only the headline request; it no longer extracts publisher paragraphs during headline preparation.

Removed `harvest_cohort.rs`, whose publisher-first acquisition and packet-fed Journalist/Influencer calls conflicted with the live interface. Use `context_harvest` for prepared requests/full context replay, `harvest_shadow` for text-free classification summaries, with worker-to-character handoffs verified by the isolated database integration test. The later cleanup removed `harvest_smoke`: it bypassed routing eligibility and duplicated the integration boundary. Character behavior remains subject to Window 2. `harvest_acquire_shadow` is explicitly an acquisition-only review diagnostic, including rejected/missing cases; it does not determine admission or production paragraph eligibility.

Removed teacher `baseline` from `Article` and corpus export. The sample, nightly and focused SQL exports use `harvester_query_provenance`, canonical article IDs, distinct query entities and their best rank. They no longer select through Editor reads or the single query entity embedded in `news_articles.raw`.

`harvest-context-v6` versions the tightened context verification and source-identity receipt. It validates saved headline admission, theme distributions, model/coverage provenance, predicate/policy versions and selected destinations before publication. Invalid or missing probabilities return errors rather than panicking or becoming negative evidence. Publisher, URL and publication date are bound in provenance; the publication row lock fences attribution/date changes along with title/body. V6 character delivery also rejects attribution/date drift. The existing `entity_choice` database name denotes **plugin admission**, while the raw model choice remains separate audit data.

Go ingestion, canary/release scripts, shadow checks and readiness reports now name v6. This creates new classification receipts instead of counting v5 history as current verification. No database migration is needed. Headline gates remain `harvest-headline-v2`; predicates remain `harvest-headline-relevance-v2` / `harvest-theme-routing-v5` because their model input and semantics did not change.

## Acquisition, maintenance and handoff

The worker checks all query/headline edges before fetching. If every edge fails policy, it stores headline receipts and completes with no publisher fetch or source classification. Otherwise it acquires/reuses one body for the article. Existing reuse requires paragraph boundaries and at least thirty opening words; unusable/short/flat prose, blocked requests, transport errors and classification errors retain distinct outcomes. The broker enforces the curated-article grant and configured run budget; HTTP acquisition retains its 20-second timeout, ten-redirect limit and optional bounded Chrome fallback. Queue retry, claim fencing and durable progress remain in their existing owners. Per-domain 429 backoff and actual acquisition coverage still need operational measurement; this change does not claim to resolve them.

Exact-title dedup and week sealing remain hourly Harvester maintenance. Non-shadow publication retains unique name-surface candidates, canonical-name links, unresolved/alias receipts, Investigator/Graph obligations and character follow-ups in the fenced transaction. Co-mention links identify source presence, not proof of a transfer or relation. Investigator/Graph must retain that distinction. Fixture extraction remains downstream Graph/Fixture Boxscore work. Google query membership is not promoted into an authoritative source mention.

Journalist, Influencer, Scout and Insider continue to consume `delivery::SourceContext`: classification/article IDs, exact headline/opening, source and publication time. They own product claims and terminal dispositions. V1–v5 pending receipts remain readable under their historical title/body integrity contract; their release scripts no longer admit them as current v6 evidence. Account for or supersede that backlog during release; do not silently relabel it. Broader Editor packet/storyline consumers remain owned by the later character and Graph windows, with removal after their source-based product and non-editorial follow-ups are verified. Editor, its tables and application surfaces were not removed here.

## Verification and limits

- Rust library suite: **580 passed, 76 ignored** (environment-dependent tests).
- Focused Harvester suite: **19 passed**, three database tests run separately.
- Disposable PostgreSQL 17 restored from the checked-in schema: all **three Harvester integration tests passed**. They cover rejection without fetch, exact source delivery, attribution/body drift, shadow with zero assignments, repeat-publication idempotency, stale claims and the existing downstream character handoffs.
- `cargo check --all-targets` validates retained binaries/examples and removed API callers. Go ingestion/work package tests also pass.
- All three export queries executed against isolated synthetic rows. Assertions verified canonical duplicate collapse, two distinct query entities, best ranks, no teacher payload and independence from deliberately incorrect legacy raw query metadata. `context_harvest` prepared those exported rows without inference.

Model calls are unchanged: one headline request per query edge; one four-predicate theme request per admitted edge after usable acquisition; zero generative calls. Theme input remains at most 100 words/1,200 UTF-8 bytes within the first three source paragraphs. Provider transport adds no retry. Request/output budget and model latency were not benchmarked anew; no model-performance gain is claimed.

Reading **0.25** and routing **0.50** remain provisional plugin policy, not calibrated confidence. A 0.30 headline probability may be admitted even when Laya's winning choice is `irrelevant`; a 0.20 probability does not qualify. Same-name/source-binding tests prove contract fencing, not classifier accuracy. The prior reviewed-packet work and sport differences in [headline calibration](harvester-headline-calibration-2026-09-27.md) remain the evidence and open quality gates. Three paragraphs can omit later facts and the smaller Laya prefix can miss themes; no representative human-reviewed evidence here justifies changing either budget.

The old entity/content pilot and source-text-free cohort indexes are historical evidence, not current-contract training or gold labels. Their archival validators remain so those records can be checked; they do not implement a second intake path. Current training requires new source-bound labels and a matching builder. The implementation checkpoint ran no live model cohort. The subsequent user-authorized [archbox smoke](harvester-alignment-smoke-2026-09-27.md) exercised eight real articles, live Laya and three fresh publisher fetches in a disposable database: seven exact v6 contexts, zero shadow assignments. All seven theme passes recommended all four characters, so selective routing remains unproven. No production migration, deployment, delivery release or production data deletion ran.

Continue the [routing calibration](harvester-routing-review-2026-09-27.md) before marking Harvester complete; the [cleanup pass](harvester-cleanup-2026-09-27.md) is verified. After that, the next plugin context is **Journalist (Window 2)**. Start from the v6 source receipt and the exact-context loader, then remove its remaining packet/source-selection and model-owned claim decisions under the alignment plan. Production release remains gated by reviewed usefulness, a fresh v6 shadow cohort and character publication evidence.

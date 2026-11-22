# Harvester System 1 comparison — October 1, 2026

> **October 7 retirement:** This is a historical experiment record. Its binary annotation/scoring, trace-manifest and Fastino adapter tools were removed for the [Classifier launch](scoracle_classifier_plugin_launch.md). Commands below describe the old experiment; their source is recoverable from commit `5c956402`.

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

Harvester prepares the same headline and seven Boolean source predicates for either model. It validates complete input coverage and bounded scores, then owns the reading cutoff, window aggregation, route cutoffs, destinations and source receipts. The model cannot select a destination. This was a local read-only replay; no provider setting changed.

The existing 24 synthetic [development cases](../fixtures/harvester/routing-v7-development.jsonl) were replayed through `context_harvest` with the same `harvest-context-v7` plugin policy. Laya was pinned to `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` (`laya` 0.3.20). Fastino GLiNER2.5-Decide was pinned to `5a7adf72a23b4d311abae6ce050d7f0012bb3416` (`gliner2` 2.0.0). The historical Fastino adapter (`harvest_fastino_server.py`) maps each plugin Boolean question to a two-label classification and returns the model's true-label probability. Both local CPU adapters used two PyTorch threads. All 24 articles and their seven theme predicates completed with confirmed input coverage.

| Model with current plugin cutoffs | Correct routes / 96 | Missed routes | Extra routes | Median article latency | p95 article latency |
| --- | ---: | ---: | ---: | ---: | ---: |
| Laya | 90 | 3 | 3 | 350 ms | 380 ms |
| Fastino | 85 | 7 | 4 | 435 ms | 474 ms |

Fastino caught the NBA performance case that Laya missed, but missed more Journalist routes. On this development set, Journalist route-score ranking (AUC) was 0.984 for Laya and 0.750 for Fastino; changing only its cutoff cannot repair all of those ordering errors. Both models routed Influencer and Scout perfectly on these cases. The latency figures are local serial replay measurements, not host throughput benchmarks.

**Decision:** keep the current Laya provider and plugin cutoffs. The scores differ enough that a Fastino switch would require plugin-owned threshold calibration and a fresh, source-bound holdout. These 24 cases informed the existing policy, contain only positive headlines, and have provisional synthetic expectations rather than independently reviewed labels. Fastino's schema also reserves some marker characters in instructions, so real-name coverage needs checking. This establishes compatibility on the tested cases and a development comparison, not production accuracy or a final model ranking. No production endpoint or routing policy changed.

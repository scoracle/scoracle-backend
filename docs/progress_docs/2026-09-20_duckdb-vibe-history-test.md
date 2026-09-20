# DuckDB Vibe-history experiment

**Decision: do not add this history block to the current Analyst prompt.** DuckDB extracted useful temporal detail, but the paired model trial did not demonstrate a reliable improvement in grounded output. Simplify the conflicting assignment before testing a replacement study. Weekly retirement is traced in the [caller-backed plan](../planning_docs/PLAN-weekly-state-retirement.md).

## Frozen test

Read-only history capture at 2026-09-20 15:55:34 UTC retained 60 Vibe records across NBA player 4 (Bam Adebayo), NFL player 11 (Ja'Marr Chase), and FOOTBALL players 38 (Pepe Reina) and 390. The current production Analyst loader prepared assignments for the first three. Player 390 had no Analyst corpus and correctly received no model call. The captures delimit an observation interval; they are not one MVCC snapshot spanning every loader query.

DuckDB v1.5.5 processed the retained export privately. It selected scored rows with the latest model/prompt combination, kept the first observation of each input hash, and calculated daily median sentiment in UTC. No week stamps, week-close job, production publication or new model-generated summary was needed.

- Adebayo: 28 retained rows reduce to six comparable input revisions on six dates. Daily medians for September 15–20: **88, 45, 55, 55, 45, 55**.
- Chase: 25 rows reduce to eight comparable revisions on six dates. Daily medians: **55, 55, 55, 50, 46, 85**. That recent rebound differs from the existing longer-window "drifting down" label; different windows need not agree.
- Reina: retained rows have no scored Vibe history. The added block explicitly marks history unknown.

All 14 comparable revisions lack direct article references on the product rows. The study measures prior model interpretations, not independent reporting, public opinion or emotional intensity. Distinct hashes are input revisions, not independent observations. Missing source coverage remains unknown.

## Paired calls and results

Three cases × two variants × two matched seeds = **12 valid first-pass calls**. Both variants used the current Analyst voice/shared form, `granite4.2:3b`, temperature 0.3, `think:false`, 4,096 context and 700 output tokens. Seeds were 17 and 29; variant order reversed on the second run. The only prompt change was a 509-byte history block for NBA/NFL or a 133-byte missing-history statement for football, inserted into the existing memory position.

| Measurement | Baseline | With history |
|---|---:|---:|
| Valid JSON | 5/6 | 6/6 |
| Nonempty JSON fields within 140/1,200 character limits | 4/6 | 4/6 |
| Total reported prompt tokens | 7,168 | 7,818 |
| Total generated tokens | 1,985 | 1,720 |

The enriched requests added 650 prompt tokens, about 9.1%. The shorter aggregate output includes one baseline exhausting its token allowance; this is not evidence of a general efficiency improvement. Calls shared the production inference host, so wall-clock latency is not a controlled performance benchmark. No correction retries or full publication pipeline were run.

Manual review of every response found:

- **NBA:** both variants confuse season arithmetic/direction. One baseline is truncated; both enriched bodies exceed the length limit. Neither enriched reading uses the new emotional series to produce a useful interpretation.
- **NFL:** enriched responses acknowledge the thin sample more directly, and the second is much shorter. Neither explains the recent 46→85 reversal. One calls unmeasured form "flat"; the other cites the supplied medians as evidence of drifting downward. This is a limited surface improvement, not reliable grounding.
- **Football:** the explicit missing-history block is respected in one response but supplies little beyond the existing unknown mood. Both variants mix seasons/competitions and misdescribe rising ratings as decline. The enriched second response invents declining "ability or engagement."

This was an exploratory review of known difficult cases, not a blinded independent assessment or statistical efficacy study. Formatting checks are not the full production parser/guard suite. Initial transport probes omitted `think:false`, exhausted the answer budget in thinking, and were excluded before the paired trial; those probes remain separately retained.

## What this tells us

The current Analyst adapter maps Scout/Influencer products into compact numeric fields, adds raw performance/cohort memories, and declares an upstream combined direction "final." It does not yet deliver the simple pair of character readings and relevant memories described by the Studio contract. Additional history gives the model more facts to reconcile with those existing directions and overlapping windows.

The next test should **replace redundant weekly labels and raw multi-season arithmetic with one explicitly dated study accompanying the Scout/Influencer readings**. Keep current level, recent change, window, sample and missingness distinct. Compare that smaller assignment against this frozen baseline before changing production. Oracle's five-card contract does not need expansion.

Complete local artifacts: `logs/duckdb-vibe-study-20260920/`, including raw history, source captures, DuckDB computation, protocol, request preparation/runner scripts, valid/invalid responses, metrics and SHA-256 manifest. Archbox retains the raw captures, summary, protocol, requests and responses at `/mnt/data/backup/scoracle/releases/duckdb-vibe-study-20260920/`. Automatic approval review rejected uploading the final report/metrics/scripts to that destination; those final artifacts remain local. The experiment made no runtime, schema, prompt or queue changes and did not release any held work.

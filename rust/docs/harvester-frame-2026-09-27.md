# Harvester v7: plugin prepares and governs; Laya scores

Implemented locally, with shared subject metadata and native scalar scoring.
Contract and database checks pass. Semantic calibration remains provisional;
production services, queues and delivery settings were not changed.

## Prepared identity and evidence

`src/plugins/meta.rs` defines `EntityMeta`: canonical name, entity type, sport and
ID. Harvester uses the canonical query loader to populate it. The same type is
available to other plugins without creating a metadata service or another lookup
path. Only Harvester is migrated in this phase.

Every question identifies its target as, for example, “Cedar United, the FOOTBALL
team.” The ID remains in the source-bound receipt; it is not useful model prose.
Missing name, sport or type fails preparation. No aliases, affiliations or roster
relationships are invented. The existing `hypothesis` JSON field now contains
this shared type; there is no duplicate Harvester identity DTO.

The headline state is exactly the title. Its predicate asks for explicit reference
to the supplied identity, removing the earlier demand for indirect-consequence
reasoning without relationship facts. Theme state is exactly a publisher window.
The title, publisher name and subject metadata cannot become additional theme
evidence merely by being placed in that state.

## Narrow scores and enforced policy

The active `choice` request/answer contract is removed. Laya receives native `noul`
boolean predicates and returns scalar support scores; it supplies no destination.
There is no generative fallback or prompt-based routing authority.

| Destination | Predicates accepted by plugin policy |
| --- | --- |
| Journalist | Specific event/development |
| Influencer | Actual emotional reaction |
| Insider | Player move/rumour, contract decision, or staffing change |
| Scout | Performance/result, or injury/suspension/availability |

Headline admission remains 0.25. Theme predicates use 0.50, except performance at
0.70. These thresholds were selected on development evidence. They are policy
cutoffs, not accuracy guarantees. The route table links directly to the existing
plugin manifests; the harness only dispatches the selected destinations.

The selected first three paragraphs are covered by consecutive exact windows of
at most 100 words and 1,200 UTF-8 bytes. Only whitespace may fall between windows.
Eight windows is the explicit ceiling; exceeding it is an error, not a negative
judgment or silent prefix cut. Each admitted article therefore makes one headline
call and at most eight theme calls, each containing seven predicates. Rejected
headlines make no theme calls and require no body coverage.

The plugin takes the maximum observed score for each predicate across the windows.
This is support aggregation, not a calibrated probability of a union. Every window
must succeed before publication. Saved byte ranges, per-window scores, checkpoint,
aggregate scores, route thresholds, selected destinations, source identity/date,
body hash and headline request hash are revalidated against the same preparation.
Maximum aggregation and cross-window sentence relationships still need evaluation
on longer real openings; character coverage alone does not establish semantic
completeness. Material after the third paragraph remains outside this contract.

## Evaluation

The real archbox Laya checkpoint was
`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`. The initial twelve-case development probe
motivated splitting broad transfer/availability questions into narrower predicates.
The earlier broad frame had selected all 48 case/route pairs in that probe.

Twenty-four additional synthetic cases cover FOOTBALL, NBA and NFL, including
results, personnel decisions, emotions, schedules, advertising and other teams.
Labels preceded their first model evaluation. Parenthesized subject metadata
unexpectedly rejected all eight football headlines. A natural-language subject
clause restored admission; multisport result wording recovered one performance
miss. Because those observations informed changes, the corpus is explicitly named
**development**, not held out.

The final candidate admitted all 24 headlines and produced:

| Route decision against provisional expectations | Count |
| --- | ---: |
| True positive | 33 |
| True negative | 57 |
| Unwanted route | 3 |
| Missed route | 3 |

The misses are a football contract, a narrative denial and an NBA result. Unwanted
routes concern another NBA team, an NFL injury and another NFL team. Exact cases
and counts are in `fixtures/harvester/routing-v7-development{.jsonl,-report.json}`.
`examples/harvest_routing_score.py` scores the final plugin decisions and rejects
missing/error rows or changed source bodies/identity. The final 24-case replay took
45.13 seconds; that is a development run, not a throughput benchmark.

All headlines in this corpus explicitly name their subject, so this result says
nothing about headline-negative precision, ambiguous identities or indirect useful
headlines. Labels are provisional synthetic expectations, not human-reviewed gold.
No real-source quality approval is claimed. The previous real-source v6 smoke is
historical evidence and has not been relabeled as a v7 model smoke.

Live model probes used the existing `harvest-laya-v1` service without modifying it.
Separately, the real local SDK/tokenizer verified all 192 final predicate frames
against the complete frame used by the updated v2 adapter; none shortened either
question or source state (largest frame: 72 of 512 tokens). This verifies those
sample requests, not arbitrary future v1 requests. Deploy v2 for enforced complete
question/criteria coverage on every call.

## Verification and release boundary

- Rust library suite: 579 passed, 77 database/environment-dependent tests ignored.
- All Rust targets compile. Focused Harvester contracts cover metadata separation,
  complete multi-window source coverage, late evidence, malformed scores/provenance,
  unchanged checkpoints, negative short-circuiting and source/policy tampering.
- Four isolated PostgreSQL tests pass, including native scalar gate storage,
  selective dispatch, claim/source fencing, immutable receipt reuse and delivery.
- Twelve Python annotation/evidence/real-SDK coverage tests pass.
- Go `internal/thirdparty` tests pass with the matching v7 intake work version.
- Nightly and cutover-readiness SQL execute against the disposable local schema;
  its reporting cohort is empty, so this is a query/schema check.

Current versions: `harvest-context-v7`, `harvest-headline-v3`,
`harvest-headline-relevance-v3`, `harvest-theme-routing-v6`,
`explicit-headline-read-p025-v2`, `source-window-predicates-v2`.
The legacy SQL `distributions` column now stores seven named scalar scores;
`choice` on headline gates is a deterministic 0.50 projection retained for the
existing schema. Admission is separately persisted at 0.25. Operational reports
read the plugin's selected destinations, not that projection. No schema migration
or duplicate scoring path was introduced.

Ship the Rust worker, Go producer, operational scripts and `harvest-laya-v2` adapter
together. Older pending delivery receipts remain supported for their existing
consumers. Independent reviewed real-source calibration and a v7 worker/model
rehearsal remain release work. Player acquisition, alias/roster resolution and the
other plugin migrations remain outside this Harvester change.

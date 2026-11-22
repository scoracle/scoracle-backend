# Harvester selective-routing development review

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

September 27, 2026. Development diagnosis only; not a release or calibration claim.

## Ownership decision

Plugins define semantic eligibility and policy. The harness executes validated,
durable dispatch. Laya supplies bounded predicate scores; downstream SmolLM3
expresses the material selected by its owning plugin. Harvester's routing intent
does not establish downstream product sufficiency. See the governing
[ownership contract](PLAN-plugin-alignment-2026-09-27.md#routing-ownership).

## Evidence

The earlier [real-source smoke](harvester-alignment-smoke-2026-09-27.md) successfully
verified the source and delivery boundary, but all seven admitted contexts selected
all four themes. That result established a need to test selectivity separately.

Read-only inference probes used the running archbox Laya endpoint, revision
`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`, Laya 0.3.20, CPU. Twelve authored
synthetic development cases cover results, transfers, emotional reactions, injury,
schedules, an unrelated team, coaching appointments, mixed themes, advertising,
rumours, transfer denial alongside a result, and missing publisher text. Labels
were authored with the fixtures, not independently reviewed. Every variant reused
these cases; they are development data, not a holdout. Empty text is a diagnostic
control, not a claim that the worker admits an empty opening.

At the existing 0.50 threshold, the current theme request classified all 48
case/theme pairs positive, including every negative control. Shortening the
instructions while retaining the same choice labels did not resolve the problem.
Changing choice order also did not resolve it. Native boolean questions (`noul`)
with source-only state substantially improved selectivity. This is an observed
comparison, not proof of a single causal mechanism: both representation and
wording affect results.

The final development variant retained explicit `false: Not reported` and
`true: Reported` criteria, placed the target in each predicate, and used only
publisher text as state. At 0.50:

| Theme | True positives | False positives | False negatives | True negatives |
| --- | ---: | ---: | ---: | ---: |
| Narrative | 7 | 0 | 0 | 5 |
| Emotion | 2 | 0 | 0 | 10 |
| Transfers | 3 | 0 | 2 | 7 |
| Performance / availability | 2 | 2 | 1 | 7 |

Mean request latency for four questions was 484 ms, compared with 1,134 ms for
the original frame in this small sequential run. These are diagnostic timings,
not a load benchmark. The candidate missed the transfer rumour and denial;
performance missed a result with a transfer denial and incorrectly selected the
appointment and celebratory signing cases. Raising a global threshold would not
repair those misses. Do not tune thresholds against these twelve cases and call
the result calibrated.

Candidate predicates, with `{target}` supplied by the plugin:

- The source reports a specific new event or match result involving `{target}`.
- This text describes someone's actual feelings or emotional reaction about `{target}`.
- This text discusses a player transfer, contract or staffing decision involving `{target}`.
- This text contains match results, statistics, injury news or team selection involving `{target}`.

Archbox artifacts remain at `/tmp/harvester-routing-review-20260927/`, including
`current-probe.json`, `explicit-probe.json`, `evidence-probe.json`,
`choice_labels-probe.json`, `noul-probe.json`, `noul_source-probe.json`,
`plain-probe.json`, `default-probe.json`, and `topics-probe.json`. They retain
synthetic requests, scores, labels, latency and checkpoint provenance. They are
temporary diagnostic artifacts, not a checked-in evaluation suite. No source
article bodies or credentials were copied into this report. No service, deployed
code, database rows or delivery flags changed during these probes.

## Remaining work

Resolve the transfer topic/denial and performance boundaries, freeze the frame
and policy, and evaluate independent reviewed cases across FOOTBALL, NBA, NFL,
teams and players. Measure misses and unwanted deliveries independently. Native
`noul` returns a scalar rather than the current Rust `ChoiceAnswer`; adopting it
requires an explicit typed response, validation, contract version, and migration
of reports that currently inspect `choice == relevant`. No boolean candidate is
implemented in the active Rust path yet.

The subsequent [cleanup](harvester-cleanup-2026-09-27.md) removed the retired packet
import, Decider adapter, entity/content training and benchmark tools after caller
checks. The [final integrity review](harvester-integrity-review-2026-09-27.md)
records additional boundary fixes and the remaining semantic-effectiveness gaps.

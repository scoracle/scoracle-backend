# Scout S1–S5 closure evidence

The ownership target is **harness → plugin → cognition model**, with the plugin
owning DB selection, assembly, acceptance and publication. Harvester is a peer
plugin using Laya. There is no separate System 1 layer.

The plan revision was committed as `895b18d7`. S1–S4 and the S5 plumbing deletion
are implemented locally. **The S5 product gate is open. Nothing is deployed.**

## Implementation and retained dependencies

- Source-triggered Scout uses Harvester's current performance score or an applied,
  source-linked roster/availability record, plus independent entity resolution.
  Missing support produces a durable unused reason. No Scout classification LLM
  remains; the only model job is measured-profile articulation.
- Publication rechecks and locks the source receipt, publisher identity, entity
  link and applied-record eligibility. A source changed during articulation cannot
  publish. Existing identity/availability and statistical-update work remains the
  owner of later rating opportunities.
- Removed the unused palette, generated headline, duplicate comparison/band maps,
  flat material formatters, body validation duplication and inert slot registry.
  Deterministic title generation, admission and evidence guards remain. Existing
  coverage guards now inspect the actual JSON profile, not retired prompt phrases.
- The completed world and retained provenance determine the fingerprint. Regression
  checks change only measured memory, coverage, a profile limit or source revision;
  each changes the hash. Identical assembled inputs retain it.
- Evaluation/capture carries prepared parts and production options. Runtime fixture
  checks reject missing parts and package drift. Journalist derives report count
  from the prepared reports. Scout runs its request-specific parser. No-call
  dispositions skip the backend; null, malformed and rejected replies stay distinct.
  Version-1 Scout captures without parts require recapture.
- Scout still reads `evidence::memories` for historical-season policy and provenance.
  Raw personnel/availability/report records are retained in provenance rather than
  converted into unused prose. This loader is not dead code. Rolling fixture-window
  bounds are still time inputs, so a different bound can change the fingerprint.

No new dependencies, service, plugin trait, judge, classifier or prompt framework
were added. Harvester calibration and Journalist's manual are unchanged.

## Mechanical verification

- `cargo test --offline --all-targets`: 555 passed, 70 ignored across all targets
  (538 library tests; 14 evaluation-binary tests). Ignored tests are not counted as
  passing evidence.
- Isolated PostgreSQL 17, built from the checked-in schema through migration 286:
  four Harvester/delivery integration tests passed, plus the applied-identity and
  rating-fanout test. These cover deterministic source dispositions, wrong entity,
  direct missing-profile preparation, roster/availability records, delivery gates,
  source mutation during Scout articulation, source-world parity, repeated-claim
  fencing and publication.
  The stale-source check verifies that no stat summary is inserted.
- `cargo clippy --offline --all-targets` completes with existing warnings in
  unmodified mechanisms. The strict `-D warnings` run is not green; no repository-wide
  lint cleanup is included in this work.
- Byte-order, fixture reassembly, missing-part rejection, multi-report parser
  context, unsupported Scout numbers, malformed/declined output and backend-free
  no-call behavior have runnable regression checks.

The source integration test uses a fake backend that accepts only the measured
world and body schema; an eligibility prompt fails the test. The source replay
below uses that exact measured world, captured before test cleanup. This is
synthetic isolated-DB evidence, not a production-data canary.

## Local model evidence

Model: `alibayram/smollm3` (installed digest
`6463ebe1ce6ae2f6369498f54f136baad56b4380e5365bfecc938079f8e6733c`).
Ollama, thinking off, 4,096 context, temperature 0; Scout output budget 700.
No model comparison or tuning campaign was performed.

[Replay inputs](../fixtures/scout/closure-s62-inputs.jsonl) contain eight explicit
synthetic worlds and the source-triggered world.
[Results](../fixtures/scout/closure-s62-results.jsonl) retain the captures, actual
requests, available raw provider replies, parser errors and attempts. Reproduce with
`eval --task rating --replay-assignment fixtures/scout/closure-s62-inputs.jsonl`.

| World | Model calls | Retries | Request bytes, including system | Final mechanical result |
| --- | ---: | ---: | ---: | --- |
| Strong/comparable | 3 | 2 | 4,434–4,841 | Rejected: body ceiling |
| Sparse | 2 | 1 | 4,450–4,625 | Parser accepted; semantic failure |
| Composite only | 3 | 2 | 4,166–4,573 | Rejected: incomplete output |
| Missing profile | 0 | 0 | 0 | Correct no-call |
| Zero versus unknown | 3 | 2 | 4,343–4,693 | Rejected: incomplete output |
| Incompatible comparison | 3 | 2 | 4,494–4,958 | Rejected: body ceiling |
| Measured memory | 3 | 2 | 4,733–5,140 | Rejected: body ceiling |
| Withdrawn/disputed report | 1 | 0 | 4,689 | Parser accepted; semantic failure |
| Source trigger | 3 | 2 | 4,409–4,952 | Rejected: unsupported stability |

That is 21 calls, including 13 retries across eight callable worlds. Thirteen
completed replies report latency of 2.990–12.326 seconds per call. Eight incomplete
provider outputs were returned as errors: their requests and errors are retained,
but that provider path did not expose raw bodies, tokens or latency to this capture.
Those measurements are unavailable, not zero. The recorder now also measures wall
time on errors for subsequent runs; the retained run predates that addition.

Manual inspection found concrete failures even in the two accepted replies:

- **Sparse:** three stored games became “their first three games of the 2026 season”
  and “a promising start.” The input explicitly says thin source coverage; it does
  not establish early-season participation or future potential.
- **Withdrawn report:** the model denied a prior comparison despite supplied prior
  percentiles, claimed strong recent form with no trend, inferred a capable
  playmaker from team aggregates, and omitted the report's publisher/date. It did
  retain some availability uncertainty. Passing dimensions/numeric containment did
  not make the read faithful.
- Other replies exceeded the body ceiling or exhausted the output budget. The
  source-triggered read still invented cross-time stability after correction.

The [seven inherited Scout quality cases](../fixtures/scout/closure-s62-quality.txt)
also failed on all six callable cases; one had no measured profile and correctly
skipped inference. These fixtures contain pre-existing conversion defects, including
numeric strings in measurement identities and unknown-sample limits beside known
sample counts. They are not clean product evidence. The explicit synthetic controls
above isolate failures without relying on those malformed worlds. Repair the old
fixtures from coherent parts before treating them as a release gate.

The [current Influencer v3 replay](../fixtures/scout/closure-v3-influencer.txt) made
four calls, no retries: 0/4 passed its paragraph contract. Request sizes were
1,281–1,392 bytes; completed call latencies were 3.70–5.61 seconds. The CLI retains
only excerpts of rejected outputs, so this run establishes surface failure, not a
complete factual audit of those four replies. Do not restore its removed reaction
classifier or add a correction pipeline to make the gate appear green.

The [Journalist multi-report replay](../fixtures/scout/closure-n95-journalist.txt)
made one call, no retries: 933 request bytes, 293 input tokens, 92 output tokens,
2.80 seconds. All three reports stayed separate, with the supplied injury/unknown
return, appointment/contract and match result; 4/4 mechanical checks passed. The
appointment's Monday and result's Sunday were omitted, so this is evidence for the
affected keyed parsing path, not a new comprehensive fidelity certification. The
retained n95 history replay remains the broader evidence for that plugin.

## Remaining gate

Inspect and repair the smallest demonstrated plugin input/manual issue, then replay
these same cases without weakening evidence boundaries or surface limits. Preserve
failed captures as failures. Do not convert sparse coverage into playing-time claims,
use a numeric containment test as semantic proof, or add another model job to prepare
facts for articulation. Scout and Influencer product verification remains open;
Insider's implementation waits. Deployment is a separate decision.

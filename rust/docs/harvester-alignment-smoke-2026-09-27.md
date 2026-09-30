# Harvester v6 real-source smoke — September 27, 2026

**Worker/acquisition/storage smoke: passed. Selective routing quality: not demonstrated.**

The current local v6 code was compiled on archbox in a separate temporary source directory and exercised against real publisher records and the live Laya service. Production remained on clean checkout `3d7bcf7`, cognition PID `583111`, with shadow mode enabled and character delivery disabled. No deployed binary, service configuration, production queue or production record changed.

## Scope and execution

The source database connection enforced `default_transaction_read_only=on`. Selected source records were copied into `harvester_alignment_smoke_20260927_v6`, restored from the checked-in schema (288 migration records, no pending migrations). All claims, acquisitions, classifications and delivery checks ran there through the current `HarvesterHandler`; it was not a reimplementation of the worker. The disposable database was removed after evidence capture.

The bounded selection chose up to two retained publisher articles and one previously rejected article per sport, keeping every team-query edge for each selected article/sport. It yielded eight articles: three football, two NBA and three NFL. No fully known-negative NBA article met the selection rule. This is a smoke cohort enriched for available paragraphs and prior gate outcomes, not a random quality sample.

One admitted article per sport had its body omitted from the disposable copy to exercise fresh publisher acquisition. The other three admitted articles reused retained text. Two rejected articles stopped before acquisition. Publisher text, URLs and the private detailed report stayed on archbox. No character generation call was required for this Harvester-only smoke.

## Results

| Check | Result |
| --- | ---: |
| Articles completed / attempted | 8 / 8 |
| Query/headline decisions | 13 |
| Admitted query edges / theme calls | 7 / 7 |
| Fresh publisher fetches | 3 |
| Rejected articles with no acquisition | 2 |
| V6 contexts / exact source-and-provenance matches | 7 / 7 |
| Contexts with publication-date binding | 7 / 7 |
| Shadow character assignments / identity mentions | 0 / 0 |
| Pending smoke work after execution | 0 |
| Production delivery loader on a temporary test receipt | Passed |
| End-to-end elapsed time | 22.302 seconds |

All contexts matched retained-body SHA-256, exact headline, UTF-8 context and model-input byte slices, publisher/URL/date provenance, and the contained-input boundary. The largest Laya source excerpt was 609 bytes. No transport or classifier-contract failure occurred. Two headline edges had Laya's raw winning choice `irrelevant` but were admitted by the plugin's 0.25 probability policy, demonstrating that model choice does not own admission.

The delivery loader was exercised with one manually inserted receipt in the disposable database, then that receipt was deleted. This verifies source loading; it does not claim that a character product was generated or published. Shadow assignment counts above were checked before that explicit delivery probe.

Laya checkpoint: `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`.

| Stage | Calls | Mean / max wall time | Largest request / response |
| --- | ---: | ---: | ---: |
| Headline relevance | 13 | 346.5 / 398 ms | 805 / 3,255 bytes |
| Four theme predicates | 7 | 2,200.7 / 2,561 ms | 2,695 / 4,524 bytes |

Sizes include serialized protocol/provenance envelopes; they are not tokenizer counts. These are smoke timings, not a throughput benchmark.

## Quality finding and release boundary

Every one of the seven admitted contexts recommended all four characters at the existing 0.50 theme threshold:

| Theme | Routed contexts | Probability range |
| --- | ---: | ---: |
| Narrative | 7 / 7 | 0.6477–0.9500 |
| Performance / availability | 7 / 7 | 0.5565–0.9530 |
| Transfers / organizational change | 7 / 7 | 0.6609–0.9621 |
| Emotional charge | 7 / 7 | 0.5706–0.9532 |

These are varying model probabilities, not a tie-handling artifact. The plugin applied its policy correctly, but this sample does not demonstrate useful selectivity. There are no reviewed accuracy labels for this smoke, so broad routing is an observed quality flag, not a measured false-positive rate. Keep reviewed theme usefulness and source coverage as release gates. Thresholds, predicates and the checkpoint were not adjusted to make this sample look better.

This smoke adds real model, publisher and database evidence to the [implementation handoff](harvester-alignment-2026-09-27.md). It does not deploy v6, release character delivery, or complete the Journalist/other character alignment windows.

## Reproduction evidence

Private archbox directory: `/tmp/harvester-alignment-v6-smoke-20260927/`.

- `source.tar.gz`: exact implementation/schema snapshot plus the temporary bounded smoke harness.
- `rust/examples/harvest_worker_smoke.rs`: executed harness, retained with that snapshot rather than added as another production path.
- `worker-report.json`: per-case receipts and call measurements; keep on the host.
- `aggregate-report.json`, `theme-summary.json`: text-free aggregate results.
- `schema.log`, `migrations.log`: disposable database setup evidence.

Source archive SHA-256: `6ff9ab40be8bc5888ff9b859bdc0489cf8282f2175287b6fc09749ee5cb59efd`.

Smoke binary SHA-256: `95259d8647d1f0cdbe917b4cd2b5360aefdb8d47f76bf96b3248da632d021659`.

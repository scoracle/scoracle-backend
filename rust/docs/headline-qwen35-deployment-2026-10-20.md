# Headline pipeline and Qwen production deployment

Deployed October 20, 2026, at approximately 07:53 Eastern. Runtime source is
commit `e4fa9bd308e2ba26e5f273298ec32933fb90a7c5`, including the known-entity
headline pipeline from `8a9a5507`, the Qwen promotion, native schema-order fix,
queue-test cleanup and verified PostgreSQL 18 UTF-8 baseline through migration 315.
All source and benchmark findings were pushed to `main` before the full build.
This record was written after activation and does not change runtime behavior.

## Live configuration

Archbox runs the API, headline-only Classifier and hourly team RSS monitor.
Known teams, players and people extracted from the returned headlines retain
separate canonical identities. Reporting-only person work is enabled; statistical
products remain scoped to teams and players. Unknown-person registration remains
out of scope.

The Mac runs the six existing character stages. Influencer's `vibe-logic` uses
the promoted Qwen3.5-4B Q4_K_M server on localhost 8081; the other routes retain
SmolLM3 on 8080. The compact prompt is `vibe-frame-v30-compact-emotion`; score-first
schema decoding is preserved on the wire. Exact model provenance, comparisons,
limitations and recovery files are in the [promotion record](qwen35-production-rollout-2026-10-20.md).

The daily controller remains enabled with midnight America/Detroit cutoff and
its existing drain and character windows. No artificial edition or fictional
publication was introduced. The retired article source-worker and browser-reader
units are absent. The API binary watcher was restored; the cognition watcher
remains inactive as it was before this release. Model settings, d1 weights,
credentials and cron enrollment were retained.

## Safety scan and cutover

The scan found and fixed three release concerns: prose-first schema serialization
changed model abstention behavior; the schema snapshot lacked migrations 313–315;
and Go queue tests still enqueued the now-rejected article-acquisition stage.
Saved experimental artifacts were scanned for credential patterns and large
binary files before the explicit scoped commit. GitHub's only workflow runs CI;
it does not deploy. Automatic approval initially rejected a combined blanket
stage-and-push command; an explicit audited file manifest and separate exact-commit
push completed successfully.

The existing dirty production checkout was preserved in a named Git stash, a
14 MB repository bundle, a binary diff and configuration/binary backups. A fresh
7.5 GB custom-format database backup has a readable restore catalog and checksum.
Both worker hosts and restart watchers were stopped during coverage. Applied
exactly migration 315; a second migration run applied zero changes. Its retired
queue cleanup deleted zero rows. Recovery found zero abandoned Classifier claims.
Matching binaries were built before any placement, then installed and verified.
API, Classifier, RSS monitor and Mac character worker were resumed.

## Verification

- GitHub [CI run 39417579111](https://github.com/scoracle/scoracle-backend/actions/runs/39417579111)
  passed all runtime/database, schema lineage and shell/ShellCheck jobs.
- Local checks passed 347 Rust library tests, CLI checks and 59 database checks
  on disposable PostgreSQL 18 UTF-8, plus Go vet, SQL statement validation and
  the complete Go race suite. All-target Rust compilation, formatting and
  release/install simulation passed. The stale Go stage tests found by the first
  full run were fixed and the entire Go suite rerun successfully.
- Local API and public `https://api.scoracle.com` checks each passed 18/18,
  including health, saved product reads and CORS. API reports `e4fa9bd308e2`.
- Running Archbox API, Classifier and RSS executables match the staged files
  byte for byte. Each had zero automatic restarts at verification.
- The managed Mac worker connected to PostgreSQL, registered six stages and
  resolved the expected Qwen route. Its exact full-release adapter smoke read
  frustration and injury worry as negative and abstained on retained neutral
  reporting; all three outputs completed and parsed.
- Initial live observation after 07:53:06 Eastern: 112 new source-bound decision
  measurements and zero new measurement-error receipts. New spectra included
  103 team, 14 player and three person targets. These counts have different
  denominators because cached measurements can produce spectra. They establish
  live plumbing progress, not independent accuracy or prose quality.
- Legacy Harvester/Editor queue rows remain inactive; no retired acquisition
  worker or browser was restarted.

A complete natural nightly edition and sustained concurrent character throughput
have not been observed. Qwen can still add unsupported pronouns, affiliations or
other details, and can misattribute another target's emotion. Factual prose is
not qualified by decision scores or valid JSON. A full backup restore was not
rehearsed.

## Exact binaries and recovery

| Artifact | SHA-256 |
| --- | --- |
| Archbox API | `e984aa9d5bb8e9bb49cd28b2f17a9c84563b3b7296a6c6efce9af2f107ea8a3a` |
| Archbox Classifier worker | `c1388dadf761545f7d0a12c83090e849c8d03b74f5af7a9263c75911eb351968` |
| Archbox RSS pipeline | `684b24f391bb28eccb05539c687a84e4e081084dfbaa86917cd8371ad5cbfefe` |
| Mac character worker | `699d3ad923bb90b2d7950a350f67a1d266fa010fa7857063a9d5cb1e1edbf39a` |
| Pre-release database backup | `15939556381fe0bd78b7364f957f025528f81c6b0f375a2371704dd0404047ac` |

The Mac build reused an `unknown` build-info stamp from the isolated promotion
build cache. Its build log, committed source and exact installed binary hash
identify the deployed artifact; the stamp does not identify its contents.

Archbox recovery and receipts are under
`/mnt/data/backup/scoracle/releases/headline-qwen35-20261020/`: the exact source
checkout, seven staged binaries and hashes, fresh dump/catalog/checksum, former
checkout bundle/stash reference, environment/units/cron/binaries, build and
migration logs, schema preflight and both API smoke reports.

Mac recovery and checks are under
`~/scoracle-worker/releases/headline-qwen35-20261020/`: committed binaries/hashes,
previous launcher/environment/agent, full verification logs and model receipts.
The earlier promotion's matched model receipts are durably retained in
`~/scoracle-worker/releases/qwen35-emotion-20261020/`.

Rollback must coordinate both hosts and migration 315. Do not restore the older
article-acquisition binaries against its reader-retirement constraint. Retain the
new compatible workers while investigating, or restore the saved schema/data and
matching source in an isolated recovery environment before a coordinated restore.

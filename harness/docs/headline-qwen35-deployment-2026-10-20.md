# Headline pipeline and Qwen production deployment

Initially deployed October 20, 2026, at approximately 07:53 Eastern from
`e4fa9bd308e2ba26e5f273298ec32933fb90a7c5`, including migration 315, the known-entity
headline pipeline, Qwen promotion and native schema-order fix. Live observation
then exposed a shared worker scheduling stall. The corrected final runtime is
`17ba32b5dc5ed03a60c41195c81d0f85db95e320`, installed on both hosts at approximately
08:10 Eastern. All seven Archbox binaries and the Mac worker match that source.
This record was updated after activation and does not change runtime behavior.

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
  passed all runtime/database, schema lineage and shell/ShellCheck jobs. The final
  scheduler-fix [run 39420049423](https://github.com/scoracle/scoracle-backend/actions/runs/39420049423)
  also passed every job.
- Local checks passed 347 Rust library tests, CLI checks and 60 database checks
  on disposable PostgreSQL 18 UTF-8, plus Go vet, SQL statement validation and
  the complete Go race suite. All-target Rust compilation, formatting and
  release/install simulation passed. The stale Go stage tests found by the first
  full run were fixed and the entire Go suite rerun successfully.
- Local API and public `https://api.scoracle.com` checks each passed 18/18,
  including health, saved product reads and CORS. API and both workers report `17ba32b5dc5e`.
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

## Coverage stall found during live observation

After the initial successful scoring, the worker stopped polling an in-flight
publication while awaiting a replacement claim. That publication held the shared
coverage lock; the controller queued for its exclusive lock, and the new claim
queued behind the controller. PostgreSQL showed an idle publication transaction
and several waiting coverage operations. Scoring stopped after 275 provisional
measurements, and five NFL RSS discoveries timed out waiting to persist.

The scheduler now keeps polling existing work while retaining the same pinned
claim future. Completed jobs release stage/group capacity; claims are not canceled
and publication fences remain intact. A deterministic real-worker PostgreSQL test
reproduced the old five-second timeout and passed with the fix in 0.04 seconds.
The complete Rust unit/CLI suite, all 60 database checks and all-target compilation
passed after the change. Two offline memory tests initially lacked their Go
helper; the helper was built and the complete database suite rerun successfully.

The Classifier was paused to release the held boundary. Three abandoned claims
were recovered with its predecessor stopped, seven matching binaries installed,
and both managed workers resumed. RSS was restarted to retry affected teams.
Both API smoke suites again passed 18/18. By 08:13:57 the fixed worker had
produced 367 new provisional measurements with zero measurement-error receipts,
a current controller tick and zero advisory waiters older than 20 seconds. This
passes the original stall point while controller transitions continue. NBA RSS
completed with zero errors, NFL was in progress, and no post-fix RSS persistence
failure had occurred. The observation covers minutes, not a nightly edition;
receipts are retained in the private scheduler-fix directories.

A complete natural nightly edition and sustained concurrent character throughput
have not been observed. Qwen can still add unsupported pronouns, affiliations or
other details, and can misattribute another target's emotion. Factual prose is
not qualified by decision scores or valid JSON. A full backup restore was not
rehearsed.

## Exact binaries and recovery

| Artifact | SHA-256 |
| --- | --- |
| Archbox API | `61ed51e75f32d8e6cc5173c1ea23a3c4dbd8a672c04a80e8e7a1d7f7df6778f3` |
| Archbox Classifier worker | `05df351e10ab98ecb7805c175728bf0acaf381d5d88285840c5d46d34c903164` |
| Archbox RSS pipeline | `9f8d68fd6243d9a641b2b6f55525c52529ce1c0428ad87e981f96c1e58cfbe19` |
| Mac character worker | `98b8be8236a062795c35e9c57d2e1980fa39ec68e571c25a2f294f3db30ef42c` |
| Pre-release database backup | `15939556381fe0bd78b7364f957f025528f81c6b0f375a2371704dd0404047ac` |

The initial Mac build reused an `unknown` build-info stamp from its isolated
promotion cache. The final rebuild regenerated that metadata; the managed worker
now reports `17ba32b5dc5e`. Exact installed hashes above identify the final artifacts.

Archbox recovery and receipts are under
`/mnt/data/backup/scoracle/releases/headline-qwen35-20261020/`: the exact source
checkout, seven staged binaries and hashes, fresh dump/catalog/checksum, former
checkout bundle/stash reference, environment/units/cron/binaries, build and
migration logs, schema preflight and both API smoke reports.

Mac recovery and checks are under
`~/scoracle-worker/releases/headline-qwen35-20261020/`: committed binaries/hashes,
previous launcher/environment/agent, full verification logs and model receipts.
The `scheduler-fix/` subdirectories on both hosts retain the final binaries,
regression/build/check logs and observation receipts; initial binaries remain for
recovery. The earlier promotion's matched model receipts are durably retained in
`~/scoracle-worker/releases/qwen35-emotion-20261020/`.

Rollback must coordinate both hosts and migration 315. Do not restore the older
article-acquisition binaries against its reader-retirement constraint. Retain the
new compatible workers while investigating, or restore the saved schema/data and
matching source in an isolated recovery environment before a coordinated restore.

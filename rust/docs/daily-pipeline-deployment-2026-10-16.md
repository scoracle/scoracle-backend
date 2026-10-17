# Daily pipeline deployment

Deployed October 16, 2026, at approximately 23:24 Eastern, following the
[Graph retirement release](graph-retirement-deployment-2026-10-16.md).
Migrations 309–311 and matching Go/Rust binaries are live. Migration 310 backfilled
reporting dates on approximately 6.7 million historical momentum rows while
API and pipeline writers were paused. Historical evidence was retained.

## Runtime state

Archbox runs the API, publisher acquisition, Classifier and hourly RSS monitor.
The monitor sweeps immediately on startup and after coverage reopens. Its service
is enabled. Archbox character stages are disabled. The old RSS, statcommentary,
vibesynth and factsweep cron entries were removed; operational and data jobs remain.

The Mac worker is loaded with narratives, vibe, rating, transfers, momentum and
sigil. All six routes use the existing local SmolLM3 server, four inference slots
and a 32,768-token request envelope. No model, source budget or prompt tuning was
introduced during deployment. The managed SSH agent retains the model forwards
and adds database forwarding on local port 15433. Its previous unmanaged database
tunnel on port 15432 was left intact. The worker successfully reconnected after
the port change and passed a read-only production identity inspection.

Database coverage enrollment is enabled with midnight America/Detroit cutoff,
15-minute maximum drain and six-hour maximum character window. At verification
the phase was coverage and no character claims were running. The first natural
edition opens at midnight October 17 for the October 16 reporting date. The
production clock was not advanced to force an edition.

## Verification

- Full local check passed: Rust unit/CLI checks, 57 database integration tests,
  Go SQL validation and the Go race suite. Release preflight and mocks passed.
- Seven daily coverage regression tests passed against a disposable Archbox
  database, including frozen measurements, retries, publication fencing, DST
  boundaries and edition-scoped outbox completion. The database was dropped.
- API smoke passed 18/18 locally and 18/18 through `https://api.scoracle.com`,
  including product reads and production-origin CORS.
- Running Archbox API, acquisition, Classifier and monitor hashes matched the
  staged artifacts; each had zero automatic restarts at verification. Mac worker,
  model and managed tunnel were running and reachable.
- Initial live observation: 29 acquired bodies and 75 new spectrum rows across
  NBA, NFL and football. NBA and NFL RSS sweeps completed with zero RSS errors;
  the football sweep was continuing. Explicit extraction/HTTP/expiry skips and
  one deferred fetch were recorded. Two NFL jobs backed off at the existing
  128-canonical-identity guard; these remain unresolved rather than bypassed.
- Three native fictional-control model calls completed and were saved privately:
  Journalist late qualification passed; Scout was rejected for calling an elite
  percentile band strong; Insider parsed but failed the denial quality expectation
  by returning a reported finding. Only one of three passed all quality checks.

The first production nightly edition, sustained throughput and character output
quality remain unqualified. In particular, production parsing did not reject the
Insider denial mistake; parsing alone is not evidence of semantic correctness.

## Provenance and recovery

The source manifest identifies the deployed working-tree snapshot; these builds
are not a clean committed release. Build-info Git labels do not establish exact
provenance for the isolated Archbox build or cached Mac build metadata.

| Artifact | SHA-256 |
| --- | --- |
| Source archive | `762f1e6f4398dd39e90f3f80e18fbe5851bff868a32a59c0d3152b431362d11e` |
| Archbox cognition/acquisition | `28275fc339ebdf0918b19b542651fc1e3d7733c4c7764731186f578f671eebb9` |
| Archbox RSS pipeline | `4d0898b1b40d3b4dedc708158891d24bd3e64fc73a65209652131d348f48cd9b` |
| Archbox API | `de5cd5a5d9b86e3927038a85e14fc460089bcc259efc08961934857bd796a20c` |
| Mac cognition | `97bb9b059d1986602797e01e8b7d0b565e356e7c917cbe0b65b241b856809b4f` |

Archbox recovery artifacts are private under
`/mnt/data/backup/scoracle/daily-pipeline-20261016-311/`: source archive and manifest,
build/test/migration/install logs, previous checkout, binaries, environment,
systemd units, cron and a 7.0 GB custom-format database backup with readable
catalog and verified checksum. A full database restore was not rehearsed.

Mac artifacts are under `~/scoracle-worker/releases/daily-pipeline-20261016-311/`:
installed binaries and hashes, source manifest, prior launcher/environment/agent
files, model smoke requests/responses and the read-only database smoke result.

Rollback must coordinate migrations, services, both host configurations and cron;
restoring only the prior mixed-stage worker is incompatible with this schedule.
The deployment record and activation documentation were updated after the source
archive was frozen; those documentation edits do not change deployed behavior.

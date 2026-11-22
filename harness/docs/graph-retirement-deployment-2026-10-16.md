# Graph retirement deployment

Deployed October 16, 2026, at approximately 10:21 Eastern. Migrations 307 and
308 and the matching worker are live on Archbox. The Mac has the matching worker
installed and remains unloaded, as it was before deployment.

Graph is absent from the fleet, model routes and acquisition enqueue paths.
Migration 308 removed 15,354 outstanding Graph jobs: 15,308 pending and 46 failed.
The validated queue constraint rejects future Graph offers. The scheduled
maintenance wrapper no longer refreshes co-mention or typed links, promotes
legacy narrative persons, or reconciles their discovery nominations.

The release also activates the previously tested bounded acquisition policy in
migration 307, including durable skip/retry receipts and publisher URL mappings.
The Classifier criteria fixture is unchanged. Kev and the Mac SmolLM3 endpoint
retain their prior model identities.

## Worker and schedule state

| Component | After deployment |
| --- | --- |
| Archbox acquisition service | Active on the new binary |
| Archbox cognition service | Active on the new binary |
| Archbox cognition restart watcher | Inactive, matching its prior state |
| Mac cognition worker | Unloaded; launcher points to the new binary |
| API | Healthy; HTTP 200 from the database health endpoint |
| RSS and other cron entries | Unchanged |

Archbox retains `classifier,narratives,vibe,rating,transfers` enrollment, with
acquisition in its separate service. The paused Mac configuration retains
`investigate_entity`; its Graph stage and Graph route settings are removed.
No additional character or Investigator stage was enabled.

## Verification

Both Archbox processes were verified against the installed binary SHA-256 and
had zero automatic restarts during the initial check. At 108 seconds after resume,
five new source snapshots had produced ten spectrum rows, including article and
subject projections, with no new Classifier error receipts. Fresh source
obligations reached Journalist, Influencer and Scout. Journalist had one `used`
obligation; the observed other outcomes were explicit holds. This verifies live
handoffs, not factual output quality or sustained capacity.

Graph work remained zero. All nine protected historical table counts matched
exactly across migration while writers were stopped, including 83,150 Graph
extractions, 68,459 narrative events, 50,447 storyline article links, 35,823 entity
candidates and 2,072 persons. Existing sources, spectra and delivery history also
matched. No old active-stage claims were stranded after restart.

The first installation attempt stopped before schema changes because Archbox
lacked `rsync`. Standard-library copying replaced that step. Original service
states and recovery snapshots were retained, and the resumed cutover completed.

## Provenance and recovery

The tested source snapshot is based on local HEAD
`fb52582e363b1047b3139b9028b11fc0d091d628` plus the retained working-tree changes.
Its manifest, rather than the older Archbox checkout HEAD, identifies deployed
source. The isolated Archbox build reports an unknown Git commit; use these
hashes and the source manifest for exact provenance.

| Artifact | SHA-256 |
| --- | --- |
| Source archive | `9e2d2613ce7c0a587b7728f32801d057439e63ad430d3a7603650320caee335d` |
| Archbox worker | `8f8b15c6f6d4fcbba032827c7e48d56f29cc3e62494af3210263e029ef604b7e` |
| Mac worker | `8516448ec8a86f6b07989ab6a65485da24d2c852f4ebe23bd25809b8c5ec26ad` |

Private Archbox recovery files are under
`/mnt/data/backup/scoracle/graph-retirement/2026-10-16-release/`. They include a
5.7 GiB custom-format database dump with a readable archive catalog and verified
checksum, a stopped-writer queue/schema snapshot, the prior binary and modified
checkout, environment and service configuration, source manifests and deployment
receipts. A full database restore was not rehearsed during this release.

Mac recovery copies and its installation receipt are under
`~/scoracle-worker/releases/graph-retirement-20261016/`, with private prior files
in `before/`.

Restoring an old binary alone is incompatible with migration 308: its acquisition
path still offers Graph work. Any rollback must coordinate the schema constraint,
binary and configuration. Historical evidence was retained rather than purged.

Continuous RSS polling, daily edition freezing, the coverage cutoff and expanded
unknown entity search remain separate implementation work. The
[coverage plan](PLAN-continuous-coverage-daily-characters.md) describes that target.

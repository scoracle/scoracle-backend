# Headline plumbing deployment

Deployed October 19, 2026, at approximately 22:23 Eastern. Migrations 313–314,
the Archbox Classifier/RSS workers and the Mac character worker are live.
This release changes evidence plumbing. d1, SmolLM3, character route settings,
`voice.rs` and `meta.rs` remain unchanged. The isolated model trial was stopped;
its outputs were not published and its configuration was not adopted.

## Live flow

RSS metadata → d1 headline spectrum → character-specific delivery → daily edition
freeze → each character's `fresh.rs` → one JSON document → existing model route.
Journalist, Influencer and Insider receive `meta`, `task`, `fresh`, `memories`
and `voice`. Headline records retain publisher, publication time, URL and scores;
article bodies do not enter headline packages. Selection and preparation live in
`fresh.rs`; request assembly lives in `prompt.rs`.

`memories.rs` reads the earlier headline archive through the same character lens.
It excludes fresh IDs and decisions after the edition freeze, keeps one copy of
an identical publisher/headline pair, skips incomplete records, and normalizes
dates to UTC. Its existing seven-day, 20-headline and 6 KB bounds remain active.
The archive is filling; no historical headline memories were available in the
three initial live captures.

## Activation and checks

- Applied exactly migrations 313 and 314. Removed the unpublished reader-transition
  body rescan from 313; old source versions and products remain archived. Migration
  314 superseded 618 pending historical deliveries, retired 27,310 acquisition
  queue rows, and reconciled 96 existing Classifier jobs.
- Disabled `scoracle-classifier-source.service` and `scoracle-reader.service`.
  Removed the backed-up `scoracle-cognition.service.d/reader.conf` dependency so
  restarting cognition cannot reactivate the reader.
- Enabled `CLASSIFIER_REPLAY_BACKLOG=true`. Historical offers count existing
  runnable Classifier work against the 1,000-job batch budget. RSS discoveries
  remain independently eligible; the queue may exceed that budget during a sweep.
- Archbox cognition and the hourly RSS monitor are active, with zero automatic
  restarts at verification. The Mac worker connected to PostgreSQL, registered
  the six existing character stages and retained its exact environment file.
- A fresh disposable schema restored and migrated successfully. The real-record
  regression passed delivery loading, fresh preparation, memory dates, edition
  freezing, character isolation and the historical replay budget. The metadata,
  voice and memory assembly regression uses the same six stored headlines and
  authentic d1 receipts, with no invented headline labels.
- All-target Rust checks, formatting and diff checks passed. Both production
  binaries were built before placement.
- Captured actual live pending inputs for Journalist/Pittsburgh Steelers,
  Influencer/New York Jets and Insider/Buffalo Bills. Each package was one valid
  five-section JSON document, with 1, 1 and 3 fresh headlines respectively.
- The unchanged SmolLM3 template/tokenizer preserved those complete packages:
  671, 900 and 1,455 input tokens respectively, inside the existing 32,768-token
  context. These checks did not generate or publish character outputs.
- Initial live observation: 200 new headline spectra; zero rows containing an
  article body, mismatched headline text or wrong evidence scope; pending native
  deliveries for all three reporting characters; zero new decision error receipts;
  no retired acquisition jobs and no character claims during coverage.

The midnight America/Detroit schedule is unchanged. A complete natural nightly
publication has not yet been observed for this release. Classifier quality,
character writing fidelity and model selection are deferred to a separate session.
Successful packaging does not establish factual output quality.

## Provenance and recovery

This is a working-tree snapshot, not a clean committed release. Binary build-info
labels alone do not identify its exact contents; the source manifest and hashes do.

| Artifact | SHA-256 |
| --- | --- |
| Frozen source archive | `5fa831f82a92f624dc3b55cb38bc947fc7becb52d9015ee3e68f0d0afce487bf` |
| Archbox cognition | `8744c590f7887175648e7454c2f597f01d53880a38eb38e4bfee556947557152` |
| Archbox RSS pipeline | `020b6e97eb9c2dcbe6b46268f6673243b34308f76e9c07b8f5c0458a6d34a218` |
| Mac cognition | `15c05b48d82f4e734b75ea77a25418c5f5057a1d33e30e90c2b989354b76d4f4` |
| Mac read-only eval | `be11d73773c07f195760e1c6f3f927e995ca8bd32986d16d53218088bca0dae6` |
| Pre-release database dump | `ad30eff7939a508707748964dc116f4bf62aefac26f0332117f7cdc0d84487d2` |

Archbox recovery files are private under
`/mnt/data/backup/scoracle/releases/headline-corpus-20261019/`: frozen source and
manifest, previous binaries/environment/unit state, removed reader dependency,
full 6.0 GB database dump, readable restore catalog, hashes and migration/build logs.
A full production backup restore was not rehearsed.

Mac artifacts and the previous launcher/environment/agent files are under
`~/scoracle-worker/releases/headline-corpus-20261019/`. Local real-corpus receipts,
request captures and validation evidence are under
`/tmp/scoracle-headline-release-20261019/`. Documentation was finalized after the
runtime snapshot; its edits do not change the deployed behavior.

Keep workers coordinated during recovery. Restoring the old binaries alone would
reintroduce incompatible article-reading behavior. Retain historical evidence and
products; use the saved dump in an isolated recovery database before considering
a database restore.

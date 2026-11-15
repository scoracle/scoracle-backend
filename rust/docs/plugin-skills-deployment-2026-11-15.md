# Plugin skills production deployment — 2026-11-15

Runtime commit: `65ae6d500939`, branch `codex/character-skills`. Cutover completed at approximately 14:01 America/Detroit (19:01 UTC). The branch was pushed before deployment; main was not merged.

## Scope and state

Paused Archbox cognition, its restart watcher, collection, and both Mac Character launch agents before switching binaries. The API continued serving saved products during the build and restarted onto the release during cutover.

Preserved the existing production scope and policies:

- Archbox: Classifier skill and RSS collection; coverage controller remains configured, with the database schedule still disabled.
- Mac: Journalist (`narratives`) and Influencer (`vibe`), each retaining its one-claim limit and existing model/context settings. The other four Character skills are present in the release but were not newly activated.
- All workers register the three plugin owners. Caretaker has no enabled handlers. The 473 pending Investigator rows remain untouched and inactive.
- Existing time-limited content observation policy and its original expiry (`1794867053`) were retained, not extended.
- No schema migration, historical-data rewrite, queue reset, or prompt change occurred. Existing five failed Vibe jobs were not reset.

The installed crontab contained no factsweep entry. Existing production backup-script customizations and the original source checkout were preserved.

## Release locations

Archbox used an isolated detached checkout at `/home/sheneveld/scoracle/releases/plugin-skills-65ae6d50`. Its `bin/` contains all seven release binaries built from the commit using the release script's isolated build mode. Production API, collection and cognition select those binaries through `zz-plugin-skills.conf` systemd drop-ins. The cognition base unit was updated to the supported task list; the existing classifier-only override remains effective.

Mac release: `/Users/scotty/scoracle-worker/releases/plugin-skills-65ae6d50/`. Both launch-agent plists point to new launchers in that directory. The new launchers preserve the previous enabled skills, endpoints and observation policy. Logs remain at their established launch-agent destinations; decision receipts use the new release directory.

Each release contains `binary-hashes.json` and a `rollback/` directory. Archbox hashes:

| Binary | SHA-256 |
| --- | --- |
| API | `6b044f6781c20dca227c922d7e84606505a909b3e54f6d7936eac08b91874dbb` |
| Collection | `5ff1d8fca0f8ff51a8a0500907e775fa16054d5be0a7815dbe03147d92a5ce30` |
| Cognition | `466b0f0a114ea8253c569840e0b5b980a1a1ceb2f8df32b201b7814bb95009b9` |

## Verification

- Production schema preflight passed read-only.
- API `/health/db` returned healthy/connected; `/` reported `65ae6d500939`.
- Saved news endpoint returned HTTP 200; unauthenticated protected logout returned HTTP 401.
- Archbox API, Classifier and collection services are active. Both Mac launch agents are running.
- All three worker startup logs report `65ae6d500939`, database connection, the new plugin roster, and only their intended enabled skills. Both Character workers reached the existing Qwen server and subscribed to queue notifications.
- Read-only fixture smoke tests against the existing model endpoints passed: one Classifier relations case, Journalist `distinct-developments`, and Influencer `clearly-positive`. These calls used fixture mode with an unreachable database URL and published nothing.

**Remaining verification:** fresh RSS requests returned HTTP 503 throughout the initial observation window. No new Classifier/Character completions or generation-ledger rows were observed after cutover. Startup, serving and direct model paths are verified; a fresh collection → classification → Character publication cycle is not yet verified. Keep main unchanged until that cycle is observed or the collection problem is resolved. The RSS HTTP client code is unchanged from the preceding release commit; this alone does not establish the cause of the errors.

## Rollback

Pause the three processing workers and collection before rollback. On Archbox, remove only the new `zz-plugin-skills.conf` drop-ins for API, cognition and coverage, restore the saved cognition base unit from `rollback/`, reload systemd and restart the previously active services. The original release overrides and binaries remain available; the effective pre-cutover units were saved in `rollback/*.effective`.

On Mac, boot out both launch agents, restore their saved plists from the release's `rollback/` directory, then bootstrap those plists. Their original launcher scripts and binaries were not changed. No schema rollback is required.

## RSS investigation and pause (19:06 UTC)

The user approved pausing RSS collection. `scoracle-coverage.service` was stopped at 19:06 UTC; API, Classifier and both Character workers remain running. Caretaker remains inactive and main remains unchanged.

A single production-host probe returned HTTP 503 with Google's explicit automated-query refusal page ("your computer or network may be sending automated queries"). It supplied no `Retry-After` header. This is a provider refusal, not evidence of a plugin routing or model failure. The previous 13:00 UTC sweep recorded 583 calls with zero RSS errors; the HTTP client implementation is unchanged between releases. No alternate IP, proxy, or user-agent workaround was attempted.

The collector fix stops the entire sweep on HTTP 429/503, propagates the failure to the run ledger, and defers monitor retries for at least one hour (longer `Retry-After` values take precedence, with numeric values capped at one year to avoid overflow). An entity whose searches all fail now returns an error rather than a false success. Ordinary partial failures still preserve the existing collection cursor. Tests cover fail-fast behavior, delay parsing, coverage-window reopening, and database cursor preservation on refusal.

Operational limit: the monitor's cooldown is process-local. Restarting it or invoking manual ingestion bypasses that timer. Keep collection stopped during this refusal; do not restart before 20:06 UTC on November 15 (15:06 EST), and do not treat that time as proof that Google has lifted the refusal. No automatic restart has been scheduled.

Fresh collection-to-publication verification remains blocked. A recovery check must establish successful RSS retrieval, new Classifier completion, Character generation under `scoracle.character`, and the corresponding served product. Model fixture smoke tests and serving previously saved products do not meet that requirement.

Collector fix `30347216` was committed and pushed to `codex/character-skills`. Its Linux binary is staged at `/home/sheneveld/scoracle/releases/rss-backoff-30347216/bin/pipeline`; the coverage override selects it for the next start, but the service remains inactive. SHA-256: `63fd62f6b6e40d41da9e847ed42e4b064e223662d8d8c8f978f65a3169f2acd8`. The prior override is saved under that release's `rollback/`. The affected Go packages passed race-enabled tests, including the database acquisition suite on a fresh local PostgreSQL database. Final read-only production checks still show no post-cutover stage completions or generation rows; 473 Investigator jobs remain untouched and the five preexisting failed Vibe jobs remain unchanged.

## Recovery check after cooldown (20:06 UTC)

At the user's request, waited until the full cooldown elapsed, then made exactly one Google RSS request from Archbox at 20:06:36 UTC (15:06:36 EST) on November 15. The request used the collector's existing User-Agent and Accept headers. Google returned HTTP 503 at 20:06:43 UTC, with the same automated-query refusal page and no `Retry-After`. Response artifacts remain on Archbox at `/tmp/rss-recovery-20261115.headers` and `/tmp/rss-recovery-20261115.body`.

The condition for resuming collection was not met, so RSS remains paused; no retry loop or restart was scheduled. Fresh collection-to-publication verification is still blocked by the provider refusal. Caretaker remains inactive and the feature branch has not been merged to main.

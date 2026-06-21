# First GPT Audit — Session 1 Production Baseline

**Captured:** 2026-06-21 15:18–15:21 EDT

**Host:** `archbox`

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 1

**Product authority:** `/home/sheneveld/scoracleWiki/wiki/Product Narrative.md`

## Goal

Capture enough immutable production evidence to distinguish pre-existing defects from regressions
introduced by the remaining launch-hardening sessions, and prove that the pre-change database backup
can be restored.

## Repository baseline

- Branch: `main`
- Worktree before Session 1 edits: clean
- Local `HEAD`: `e4f0925454cee27b17abbad84c1e3bb45feca318`
- `origin/main`: `e4f0925454cee27b17abbad84c1e3bb45feca318`
- Synchronization: `git pull --ff-only` returned `Already up to date.`
- Baseline commit date: 2026-06-21 14:45:18 EDT
- Baseline commit subject: `docs: align backend audit with product narrative`

Session 1 changes only the restore drill and this progress document. No migration, service restart,
cron change, or production-data mutation was performed.

## Deployed artifacts

The current Go build does not embed VCS revision metadata: `go version -m` reports the module as
`(devel)`. Commit attribution therefore comes from binary timestamps, matching deployment/restart
records, and the commits that immediately recorded those deployments. Hashes are the durable
identity of what was actually deployed.

| Artifact | Timestamp (EDT) | SHA-256 | Source attribution |
|---|---:|---|---|
| `go/bin/scoracle-api` | 2026-06-19 23:16:05 | `e3e630d50f4a0b9531440ee788c5e397a45f375fdcdd5f84dd095722ed7aa297` | Working tree later committed as `102b6ab2e6d4bf0d14dcbe9252b3e3549bb279ba` (`remove bundled profile`) |
| `go/bin/pipeline` | 2026-06-18 09:34:22 | `aee8596028845c7079be5f463ace019effaeb257986d433ebed8d81965217fca` | `aff6fc5400c6ee036e0fd899f98b947eb10fb872` |
| `go/bin/statcommentary` | 2026-06-18 23:44:39 | `8f206d7abef5fda14195e12c522e501f66dbabf21d996b7bc2f67df58af1f15f` | Working tree later committed as `f27daee6edae4f04daa5bc8506a4fbc9b6dfed52` |
| `go/bin/vibesynth` | 2026-06-18 23:23:49 | `405b07a5483efaa2751fe1a8ff50b141ea5acf8b9a90853dd038f6d02d794134` | Working tree later committed as `8e15dada191bf39f2ece1830504d4811f300db90` |
| Python seeder | editable install | n/a | `.venv` package `scoracle-seed 0.1.0`, editable source at `seed/`; effective source is current checkout `e4f0925` |

The API process started at 2026-06-19 23:16:06 EDT and has not restarted since. This proves the
running API uses the hashed binary above. It also proves production is not running the later
`1e08b49` observability commit or the current `e4f0925` audit-only commit.

## Installed systemd state

| Unit | State | Installed SHA-256 |
|---|---|---|
| `scoracle-api.service` | active/running | `20c34c0d0f712d309d40f44b66e468d3832c46e56e5ac68f37ddcd3362e143cd` |
| `scoracle-api.path` | active/waiting | `657b2b65f8bd44045136c4f5f95f022da261efe8cb00a8722c4b3010df021478` |
| `scoracle-api-restart.service` | inactive/dead (oneshot) | `7698cbce056ca7edeec284ffad398456cd3aa26786da77a7a98db698ab4b211a` |
| `cloudflared.service` | active/running | `a77ff66a97d6c6f531c5b50fe737f5927b5bfc047b74b40cd5609606018acbb8` |

All installed unit hashes differ from the repository templates. Exact effective contents:

```ini
# ~/.config/systemd/user/scoracle-api.service
[Unit]
Description=Scoracle Data API (self-hosted)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=/home/sheneveld/scoracle/scoracle-backend
EnvironmentFile=-/home/sheneveld/scoracle/scoracle-backend/.env
EnvironmentFile=-/home/sheneveld/scoracle/scoracle-backend/.env.local
ExecStart=/home/sheneveld/scoracle/scoracle-backend/go/bin/scoracle-api
Restart=always
RestartSec=3
StartLimitIntervalSec=0
StandardOutput=journal
StandardError=journal
SyslogIdentifier=scoracle-api

[Install]
WantedBy=default.target

# ~/.config/systemd/user/scoracle-api.path
[Unit]
Description=Restart scoracle-api when its binary is rebuilt

[Path]
PathChanged=/home/sheneveld/scoracle-backend/go/bin/
Unit=scoracle-api-restart.service

[Install]
WantedBy=default.target

# ~/.config/systemd/user/scoracle-api-restart.service
[Unit]
Description=Restart scoracle-api (path-trigger target)

[Service]
Type=oneshot
ExecStart=/usr/bin/systemctl --user restart scoracle-api.service

# ~/.config/systemd/user/cloudflared.service
[Unit]
Description=Cloudflare Tunnel for Scoracle
After=network-online.target scoracle-api.service
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/cloudflared --no-autoupdate tunnel run
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
```

Baseline defect: the installed path unit watches the obsolete
`/home/sheneveld/scoracle-backend/go/bin/`, while the deployed binary lives under
`/home/sheneveld/scoracle/scoracle-backend/go/bin/`. Automatic restart-on-build is therefore broken.

## Installed cron

The installed crontab is the repository `scripts/hosting/crontab.example` at baseline commit
`e4f0925`, plus these exact trailing lines:

```cron
# Sigil (crown) synthesis — nightly, bounded (Optimization Ledger O2). 95% backlog drains over ~weeks at limit 150.
0 5 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-vibesynth.sh -mode nightly -limit 150 -throttle-ms 250 >> /home/sheneveld/scoracle/scoracle-backend/logs/vibesynth.log 2>&1
```

Timezone is `America/New_York`. The live cron therefore includes:

- football process daily at 23:00, hardcoded season 2025
- football fixtures/meta weekly Monday at 23:00/23:30, hardcoded season 2025
- pipeline corpus daily at 00:00
- tier recompute Monday at 02:00
- stat commentary daily at 03:00
- backup daily at 04:00
- Sigil synthesis daily at 05:00
- no NBA or NFL ingestion

## Database and migrations

- PostgreSQL: 18.4
- Database/user: `scoracle` / `scoracle`
- Current seasons: FOOTBALL 2025, NBA 2025, NFL 2025
- Applied migration rows: 99
- Exact ordered set: `001`–`041`, `042a`, `042`–`098`, with no missing numbered migration.
- `001`–`051` were bootstrapped at 2026-06-09 12:13:16 EDT.
- Latest applied migration: `098_decommission_tweets` at 2026-06-19 22:42:08 EDT.

## Row counts and freshness

Snapshot time is approximately 2026-06-21 15:18 EDT.

| Relation | Rows | Freshest timestamp |
|---|---:|---|
| `fixtures` | 24,991 | 2026-06-17 22:21:21 EDT |
| `event_box_scores` | 965,875 | 2026-06-17 22:21:21 EDT |
| `event_team_stats` | 49,964 | 2026-06-17 22:21:21 EDT |
| `player_stats` | 42,964 | 2026-06-17 22:22:03 EDT |
| `team_stats` | 1,176 | 2026-06-17 22:22:03 EDT |
| `news_articles` | 67,192 | 2026-06-21 00:02:41 EDT |
| `news_article_entities` | 116,441 | 2026-06-21 15:18:10 EDT |
| `transfer_rumors` | 48,901 | 2026-06-21 02:15:30 EDT |
| `news_summaries` | 5,263 | 2026-06-21 10:10:36 EDT |
| `vibe_scores` | 24,484 | 2026-06-21 14:55:42 EDT |
| `stat_summaries` | 3,207 | 2026-06-21 13:14:37 EDT |
| `sigil_synthesis` | 686 | 2026-06-21 10:06:29 EDT |

Stats-rail freshness by sport:

| Sport | Fixtures freshest | Event box scores freshest | Player stats freshest |
|---|---|---|---|
| FOOTBALL | 2026-06-17 22:21 EDT | 2026-06-17 22:21 EDT | 2026-06-17 22:22 EDT |
| NBA | 2026-05-30 13:57 EDT | 2026-05-30 13:57 EDT | 2026-06-10 18:22 EDT |
| NFL | 2026-05-30 15:35 EDT | 2026-05-30 15:35 EDT | 2026-06-10 18:22 EDT |

The NBA/NFL event freshness gap is pre-existing and matches the known absence of live ingestion
scheduling.

## Pipeline backlog

### Fixtures

- Due pending fixtures with `seed_attempts < 3`: 0
- Due retry-exhausted fixtures with `seed_attempts >= 3`: 0
- Fixture statuses: 24,982 seeded; 9 cancelled

### News scrub

| Sport | Unscrubbed links | Primary | Fuzzy |
|---|---:|---:|---:|
| FOOTBALL | 18,630 | 0 | 18,630 |
| NBA | 13,926 | 0 | 13,926 |
| NFL | 9,963 | 0 | 9,963 |
| **Total** | **42,519** | **0** | **42,519** |

### Rated entities missing current derived products

| Sport | Entity | Rated | No stat generation/commentary | No scored Sigil |
|---|---|---:|---:|---:|
| FOOTBALL | player | 2,051 | 1,428 | 2,051 |
| FOOTBALL | team | 96 | 96 | 96 |
| NBA | player | 253 | 0 | 9 |
| NBA | team | 30 | 30 | 14 |
| NFL | player | 1,040 | 0 | 1,040 |
| NFL | team | 32 | 32 | 32 |
| **Total** |  | **3,502** | **1,586** | **3,242** |

All current `stat_summaries` rows counted above are real commentary rows; no current-season marker-only
gap was observed. All 686 existing `sigil_synthesis` rows are scored.

## Backup and restore verification

- Fresh pre-change dump:
  `/mnt/data/backup/scoracle/scoracle-20260621T191924Z.dump`
- Archive creation: 2026-06-21 15:19:24 EDT
- Completed: 2026-06-21 15:20:35 EDT
- Size: 470,562,787 bytes (449 MiB)
- SHA-256: `d101ef019bdfd667dc002ea490291b65fdc96281daddac7ba53044b8246b74ca`
- Archive metadata: custom format, gzip compression, 484 TOC entries, PostgreSQL 18.4
- Backup filesystem: `/dev/nvme0n1p1`, 1.9 TiB total, 1.8 TiB available at capture

The restore drill was corrected before use:

- removed `pg_restore ... || true`
- enabled `pg_restore --exit-on-error`
- removed the decommissioned `tweets` table check
- added all Session 1 critical rail and derivation tables
- made missing/empty critical tables fatal
- made missing migration history fatal
- retained live-vs-restored count deltas as drift reporting because production remains writable
  while the drill runs

Restore result: **PASS**.

- `pg_restore --exit-on-error` completed successfully.
- All 13 critical relations matched the production row count exactly at verification time.
- Restored `schema_migrations`: 99 rows.
- Throwaway database: `scoracle_restore_drill_2950153`, dropped by the exit trap.
- API health after the drill: `/health` HTTP 200; `/health/db` HTTP 200.

## Findings carried into later sessions

1. Installed `scoracle-api.path` is stale and does not watch the deployed binary directory.
2. Go binaries are not revision-stamped; attribution relies on deployment records and hashes.
3. API and sibling job binaries are deployed from different commits.
4. Live cron is ahead of the checked-in template by the Sigil job.
5. Sigil is normally scheduled at 05:00 despite the Product Narrative requiring event-driven,
   debounced generation with cron only as repair/backfill.
6. NBA/NFL ingestion is absent and their event data is stale.
7. Football cron hardcodes season 2025.
8. The scrub backlog contains 42,519 fuzzy links.
9. Current-season Rating coverage is incomplete for football and all team types.
10. Current-season Sigil coverage is only 260 of 3,502 rated entities.
11. PostgreSQL data (`/mnt/data/postgres/data`) and backups (`/mnt/data/backup/scoracle`) are on the
    same `/mnt/data` filesystem (`/dev/nvme0n1p1`); this is not an off-disk or off-host
    disaster-recovery copy.

## Files changed

- `scripts/hosting/restore-drill.sh`
- `progress_docs/2026-06-21_first-gpt-audit-session-1-production-baseline.md`

## Verification

- `git pull --ff-only`: already synchronized with `origin/main`
- `bash -n scripts/hosting/restore-drill.sh`: pass
- missing dump path: exits non-zero
- fresh `pg_dump`: pass
- `pg_restore --exit-on-error` into throwaway database: pass
- 13 critical relation counts: exact match
- throwaway database cleanup: confirmed absent
- `/health`: HTTP 200
- `/health/db`: HTTP 200
- `git diff --check`: pass

## Result

Session 1 is complete. The repository now contains a dated production baseline and a restore drill
that fails on restore errors instead of masking them. No service, cron, migration, or application
data changes were made.

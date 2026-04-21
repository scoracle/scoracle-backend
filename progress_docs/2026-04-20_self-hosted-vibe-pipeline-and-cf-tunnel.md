# Self-hosted vibe pipeline + Cloudflare Tunnel to production

**Date:** 2026-04-20 (session spanned 2026-04-18 → 2026-04-20)

## Goal

Turn the Scoracle backend into a properly self-hosted service running
on the local Arch desktop, with a Gemma-driven vibe generation layer
wired end-to-end and the frontend at `scoracle.com` pulling from it
over a Cloudflare Tunnel.

Start state: fresh hardware. Local Postgres just installed on a new
2TB KingSpec NVMe. Ollama installed with Gemma 4 e4b. Backend code
previously lived on Railway + Neon.

End state: the API is systemd-managed on the dev box, Cloudflare
Tunnel exposes `api.scoracle.com`, and the deployed frontend (running
on Cloudflare Workers) is calling through it.

## Context

Several threads ran in parallel this session. Rather than list them
chronologically, grouping by area:

### Fresh local-host setup
- New Arch box with a KingSpec XG7000 2TB NVMe mounted at `/mnt/data`.
- Postgres 18.3 relocated to `/mnt/data/postgres/data` via a systemd
  drop-in.
- Ollama serving `gemma4:e4b` (~8GB VRAM, fits the consumer GPU).
- Backend repo pulled clean; schema from `sql/*.sql` applied to the
  local DB in sequence.

### Provider decisions (recapped from earlier in session)
- Evaluated api-sports vs BDL (NBA/NFL) and vs SportMonks (football):
  - Football: SportMonks is 1 call per fixture via `include=`. api-sports
    is 4–5 calls. Keep SportMonks.
  - NBA: BDL is 1 call per game box score. api-sports is 2. Keep BDL.
  - Metadata gap: api-sports is the only one with logos + headshots for
    NBA/NFL, and image CDN requests don't count toward quota. Narrow
    use: images only.
- X / Twitter: free tier has zero API credits post-2024. Looked up
  pay-per-use pricing ($0.005/post read). Cost projection landed at
  ~$30-80/mo at modest cache-miss traffic.

### Data-quality clean-up
- `metadata_refresh_queue.unique_pending_request` was `DEFERRABLE
  INITIALLY DEFERRED` — incompatible with the trigger's ON CONFLICT
  upsert. Migration 003 made it non-deferrable.
- `finalize_fixture()` was calling `mark_fixture_seeded(p_fixture_id)`
  without scores, leaving `fixtures.home_score / away_score` NULL
  forever. Migration 004 fixed it + backfilled ~5,900 existing seeded
  fixtures.
- `upsert_team` was using straight EXCLUDED overwrites, so load-fixtures
  nulled any city/country/league_id that meta-seed had populated.
  Changed to COALESCE; `teams.league_id` added to the dataclass +
  SQL.
- BDL's `/players` returns the all-time roster (~5,500 NBA / ~11,000
  NFL). New `scoracle-seed meta purge-inactive` drops anyone with no
  `event_box_scores` row older than `--grace-days`. Dropped **13,083**
  junk player rows. NBA 827, NFL 2,675, Football 3,254 after purge.

### News corpus for Gemma
- `news_articles` + `news_article_entities` tables (migration 006).
- Extended the existing news fetcher to write-through: every matched
  article persists, with cross-entity linking against a cached
  `NewsService` pool of teams + players per sport. Trade-rumor signal
  shows up naturally when an article mentions two entities.
- One-time backfill CLI (`scoracle-seed news backfill --teams-only` for
  NFL/football, full for NBA). Pulled 2,644 articles, 4,889 entity
  links across 484 distinct entities. Later **removed** from the CLI
  after Gemma was validated on the corpus — organic write-through
  keeps the corpus fresh in steady state.

### X / Twitter wiring + telemetry
- Three curated X List IDs (NBA, NFL, Football) added to `.env.local`.
- Fixed a real bug in the refresh path: X v2 only populates
  `meta.newest_id` when the request includes `since_id`, so cold
  pulls were leaving `since_id` empty and every subsequent refresh
  re-fetched all 100 tweets instead of the delta. Fix: derive
  `newest_id` from `data[0].id` when meta is empty.
- 24h tweet TTL purge ticker in `maintenance.go` (X ToS — no long-term
  storage of tweet content for ML).
- Per-sport counters on `twitter_lists` (migration 005): `calls_today`,
  `tweets_today`, reset at UTC midnight. Surfaced in
  `/api/v1/twitter/status` so we can watch actual spend.

### Gemma vibe generation
- New `go/internal/ml/` package: `OllamaClient` + `Generator` +
  `VibeRequest / VibeResult`.
- System prompt: ~140-char conversational fan blurb, news/tweets carry
  the sentiment, stats sprinkled in lightly. Tuned `num_predict=800`
  after discovering Gemma 4 needs reasoning headroom before it emits
  visible output.
- `vibe_scores` table (migration 007) stores blurb + input corpus IDs
  (news + tweet) + model_version + prompt_version for traceability.
- Two entry paths into the generator:
  1. **CLI** (`go/cmd/vibe` binary): `-mode single` for manual prompt
     iteration, `-mode batch` for nightly coverage of starter-tier
     entities.
  2. **Listener** (`go/internal/listener/vibe_worker.go`): subscribes
     to the existing `percentile_changed` channel, filters to
     `tier=headliner` + `new_percentile>=90`, debounces 30 min
     per entity, dispatches async so FCM + vibe paths don't serialize.
- HTTP endpoints: `GET /api/v1/{sport}/vibe/{type}/{id}` + `/history`.

### Entity tiering (scale guardrail)
- Before tiering, a single GPU couldn't keep up with the percentile
  event volume across all ~10k entities. Migration 008 added a `tier`
  column (`headliner` | `starter` | `bench` | `inactive`) + the
  `recompute_entity_tiers(sport, season)` function.
- Real-time vibe pool: 150 headliners per sport + all teams = **608
  entities** total. Fits GPU budget.
- Daily 03:00 ET batch covers `starter` tier for entities who played in
  the last 24h (~500 per busy day × 20s = ~2.8hr). Wraps before
  morning.
- Weekly 02:00 Monday `recompute-tiers.sh` refreshes rankings as the
  season progresses.

### Self-hosting ops
- `scripts/systemd/` — three user units:
  - `scoracle-api.service` — long-running API
  - `scoracle-api.path` — inotify watch on `go/bin/` that triggers a
    restart whenever `go build -o` writes a new binary. Solves the
    "I rebuilt but forgot to restart" gotcha.
  - `scoracle-api-restart.service` — oneshot helper the path unit
    actually triggers. Path units fire `systemctl start`, which is
    a no-op on an already-running service. The oneshot explicitly
    `restart`s.
  - `cloudflared.service` — CF Tunnel runner.
- `scripts/hosting/` — shell scripts + templates:
  - `install.sh` — idempotent installer (copies units, daemon-reload,
    prints remaining manual steps)
  - `cron-scoseed.sh` — wrapper that rebuilds venv + .env.local state
    so cron can invoke `scoracle-seed`
  - `backup-postgres.sh` — nightly `pg_dump` to `/mnt/data/backup/`
    with 14-daily + 12-monthly tiered retention
  - `restore-drill.sh` — restores a dump into a throwaway DB and
    diffs row counts (an untested backup is not a backup)
  - `recompute-tiers.sh`, `logrotate.conf`, `tunnel-smoke.sh`
  - `crontab.example` with TZ=America/New_York and all the entries
- Cron (via `cronie`, installed this session on Arch):
  - Daily 23:00 ET — football event drain
  - Weekly Monday 23:00 ET — football load-fixtures + meta seed
  - Daily 03:00 ET — vibe batch
  - Weekly Monday 02:00 ET — tier recompute
  - Daily 04:00 ET — pg_dump + retention prune

### Cloudflare Tunnel
- `cloudflared tunnel create scoracle` → UUID
  `b3a72114-d565-426d-8a94-9388fc015140`.
- DNS routed: `api.scoracle.com` → tunnel.
- User systemd unit manages `cloudflared tunnel run` under linger.
- Config at `~/.cloudflared/config.yml` maps `api.scoracle.com` →
  `http://localhost:8000` with a catch-all 404 for any other hostname.

### Frontend wiring
- `~/Scoracle/wrangler.jsonc` — flipped `PUBLIC_GO_API_URL` from the
  Railway URL to `https://api.scoracle.com/api/v1`.
- `~/Scoracle/scripts/fetch-autofill.mjs` — same URL swap for the
  build-time prefetch.
- Ran `npm run fetch-data` successfully: NBA 857 entities, NFL 2707,
  Football 3350 — matches the DB counts exactly, proving the tunnel
  works end-to-end.
- `npm run build && npm run cf:deploy` pushed the Cloudflare Worker.
  Custom domains `scoracle.com` + `www.scoracle.com` wired via CF
  dashboard (Settings → Domains & Routes).
- `ENVIRONMENT=production` flipped in `.env.local` so
  `CORS_PRODUCTION_ORIGINS` is included. Also added
  `https://scoracle.albapepper.workers.dev` to keep the workers.dev
  URL functional for debug.

## Decisions worth remembering

1. **Narrow scope for api-sports.** Rejected as an event-data replacement
   because of its per-fixture call multiplier. Kept only as an image
   source (teams.logo_url, players.photo_url for NBA/NFL).

2. **Tweet data is inference-only.** 24h hard TTL via maintenance
   ticker, documented inline so a future contributor doesn't extend
   the window for "more Gemma context." News is the training corpus;
   tweets are the realtime layer only.

3. **Vibe generator is lazy-only on tweets.** The worker reads the
   tweets table as-is; it never triggers a fresh X API call. All X
   refreshes originate from user-facing Twitter tab traffic.

4. **Tiering exists because a single GPU can't do 10k.** The filter
   `tier=headliner` on the real-time path is a scale guardrail, not
   a coverage compromise. Starters still get a daily batch blurb.

5. **Path unit watches the directory, not the binary.** Atomic
   rename from `go build -o` leaves the old inode orphaned; a
   file-path inotify watch never fires. Directory-level PathChanged
   catches the close-write on the temp file.

6. **Tunnel rather than public Postgres.** The Go API stays bound to
   localhost:8000. Only CF Tunnel can reach it. Postgres is never
   exposed.

## Quick reference

### Start / restart the stack
```bash
systemctl --user status scoracle-api scoracle-api.path cloudflared
systemctl --user restart scoracle-api        # manual restart
# or: touch any file under go/ and the path unit auto-restarts ~1s later
journalctl --user -u scoracle-api -f         # tail API logs
journalctl --user -u cloudflared -f          # tail tunnel logs
```

### Manual vibe for prompt iteration
```bash
./go/bin/vibe -entity-type player -entity-id 115 -sport NBA
./go/bin/vibe -mode batch -sport NBA -since-hours 24 -max 10
```

### Smoke the tunnel + API + CORS after any config change
```bash
scripts/hosting/tunnel-smoke.sh https://api.scoracle.com https://scoracle.com
# --full also exercises news + twitter write-through (spends provider quota)
```

### Backup + restore drill
```bash
scripts/hosting/backup-postgres.sh
scripts/hosting/restore-drill.sh /mnt/data/backup/scoracle/scoracle-<date>.dump
```

### Public endpoint surface
```
https://api.scoracle.com/health
https://api.scoracle.com/api/v1/{sport}/{entityType}/{id}          # profile
https://api.scoracle.com/api/v1/{sport}/meta
https://api.scoracle.com/api/v1/{sport}/vibe/{entityType}/{id}     # Gemma blurb
https://api.scoracle.com/api/v1/{sport}/vibe/{entityType}/{id}/history
https://api.scoracle.com/api/v1/news/{entityType}/{entityID}       # + persist
https://api.scoracle.com/api/v1/news/status
https://api.scoracle.com/api/v1/{sport}/twitter/feed
https://api.scoracle.com/api/v1/twitter/status                     # incl. calls_today
```

## Migrations applied this session

| # | File | Purpose |
|---|---|---|
| 003 | `metadata_queue_constraint_non_deferrable.sql` | Fix trigger UPSERT |
| 004 | `finalize_fixture_scores.sql` | Populate fixture home_score / away_score |
| 005 | `twitter_usage_telemetry.sql` | calls_today / tweets_today counters |
| 006 | `news_corpus.sql` | news_articles + news_article_entities |
| 007 | `vibe_scores.sql` | Vibe blurb storage with traceability |
| 008 | `entity_tiers.sql` | headliner/starter/bench/inactive tiering |

## Updated file layout

```
scoracle-data/
├── go/
│   ├── cmd/
│   │   ├── api/                                # Go API server
│   │   └── vibe/                               # NEW: manual + batch vibe CLI
│   └── internal/
│       ├── listener/
│       │   ├── listener.go                     # + vibe dispatch
│       │   └── vibe_worker.go                  # NEW: tier-gated, debounced
│       ├── ml/                                 # NEW: Ollama client + Generator
│       │   ├── ollama.go
│       │   └── vibe.go
│       └── api/handler/
│           └── vibe.go                         # NEW: GET vibe + history
├── scripts/
│   ├── systemd/                                # NEW: user units
│   │   ├── scoracle-api.service
│   │   ├── scoracle-api.path
│   │   ├── scoracle-api-restart.service
│   │   └── cloudflared.service
│   └── hosting/                                # NEW: ops scripts + templates
│       ├── install.sh
│       ├── cron-scoseed.sh
│       ├── backup-postgres.sh
│       ├── restore-drill.sh
│       ├── recompute-tiers.sh
│       ├── tunnel-smoke.sh
│       ├── crontab.example
│       ├── logrotate.conf
│       ├── cloudflared-config.example.yml
│       └── README.md
├── sql/migrations/
│   ├── 003 → 008                               # (new this session)
│   └── ...
├── seed/
│   ├── services/
│   │   └── meta/handlers/apisports_images.py   # NEW: NBA/NFL image seed
│   └── shared/
│       └── apisports_client.py                 # NEW
└── planning_docs/
    ├── SELF_HOSTING_OPS.md                     # NEW
    └── CRON_SEEDING_STRATEGY.md                # NEW
```

## Follow-ups

- **Off-disk backups.** `/mnt/data/backup/` is same-disk-as-Postgres. A
  drive failure still loses everything. Pointing `BACKUP_DIR` at a USB
  drive / NAS / R2 bucket closes this.
- **BDL webhooks for NBA + NFL.** Replaces the missing NBA/NFL cron
  entries once wired.
- **Frontend `SimilarityTab.tsx` → Compare.** The `migration/solidstart`
  branch still has the old Similarity tab calling `/api/v1/similarity/`,
  which no longer exists on the Go API. Compare-tab replacement lives
  on a different branch; needs porting to the Solid migration. Handed
  off to a frontend-repo Claude session.
- **Cross-entity match enrichment sweep.** Current write-through only
  links articles to the requested entity + anything matched at fetch
  time. A nightly sweep walking all sport entities per article would
  catch the long tail (retired players mentioned in news, etc.).
- **Prompt iteration loop.** `prompt_version=v1` is the first pass;
  stored on every vibe row so A/B comparisons are possible when we
  try v2.
- **Off-site tunnel redundancy.** If the home ISP drops, so does
  api.scoracle.com. Would need a fallback deploy (Railway warm
  standby, say) for real availability guarantees.

## What landed, concretely

Commits on `main` this session, roughly in order:

```
4a03c5e Add vibe blurb generator + vibe CLI
48f9eff Add HTTP vibe endpoints
6cd11e0 Wire vibe worker into percentile listener
bab9da1 Add 24h tweet TTL purge to maintenance ticker
915e821 Add per-sport Twitter API usage telemetry
ad07763 Add news_articles + news_article_entities tables
7ee805a Write-through persist news articles + entity links
cc657ea Add meta purge-inactive command for player cleanup
42a58ee Cross-entity linking in news write-through
1cfe6bb Add --teams-only flag to news backfill
7532fcd Add Ollama HTTP client for local Gemma inference
821c1b0 Add entity tier + daily vibe batch (scales to 10k entities)
ec99bdd Fix: pass interval hour counts as strings in batch query
374ba6a Add self-hosting scripts: systemd units, cron, backups, CF Tunnel
4463b6a Change backup default target to /mnt/data/backup/scoracle
224dbee Fix: systemd path unit actually restarts the API on rebuild
5c51f83 Remove news backfill CLI; add tunnel smoke test
513e4c4 Smoke test: flag <YOUR-DOMAIN> as placeholder more clearly
```

(plus earlier commits from the env consolidation + NewsAPI removal +
API_KEYS scrub chain that bridged us into this session)

Backend is live, tunneled, self-hosted, self-maintained. Frontend is
wired. The rig works.

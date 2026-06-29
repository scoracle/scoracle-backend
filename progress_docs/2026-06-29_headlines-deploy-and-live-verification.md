# 2026-06-29 — Headlines feature deploy + live verification

## Goals
- Apply migration 114 (headlines provenance columns) before any API restart.
- Restart the Go API and Rust cognition daemon with the `headlines` stage enabled.
- Verify entity and leaderboard headlines endpoints end-to-end.
- Verify the frontend News card Headlines scope rendering and instant scope switching.
- Deploy the frontend to Cloudflare Workers.

## What was done

### Backend
- Ran `./sql/migrate.sh` against the local Postgres. Migration 113 (`headlines` table) and 114 (provenance columns `input_news_ids`, `model_version`, `prompt_version`, `trigger_type`, `generated_at`) were applied.
- Updated `COGNITION_STAGES` to `scrub,headlines,transfers,narratives,vibe,sigil` in both `.env.local` and the systemd unit `~/.config/systemd/user/scoracle-cognition.service`, then reloaded systemd.
- Rebuilt the Rust `scoracle-cognition` binary and restarted the daemon; it registered all six stage handlers including `headlines`.
- Fixed a bug in `go/internal/db/db.go` where the `headlines_leaderboard` prepared statement referenced `u.headline_count`/`u.latest_at` in the outer `row_number()` ORDER BY, but the inner subquery aliases them as `score`/`generated_at`. Changed to `u.score DESC, u.generated_at DESC`.
- Committed and pushed the SQL fix as `2350d98`.
- Rebuilt the Go API with proper build stamps and restarted it; it now reports `commit=2350d98`.

### Frontend
- Ran `npm run dev` and used Playwright to verify the News scope selector options (`News`, `Transfers`, `Headlines`), the Headlines empty state, and instant switching between scopes.
- Verified SSR HTML for a populated entity includes the rendered headline title, category badge, source, and relative time.
- Ran `npm run cf:deploy` to build and deploy to Cloudflare Workers.
- Stopped the local Vite dev server and removed temporary verification artifacts.

### Pipeline verification
- Confirmed existing `public.headlines` rows carry full provenance:
  - `input_news_ids` populated with cited article IDs.
  - `model_version` = `mistral:7b`.
  - `prompt_version` = `h1`.
  - `trigger_type` = `news_spike`.
  - `generated_at` set.

## Decisions
- Kept the local `scoracle-cognition` binary as a debug build for now (placed by `release.sh --build-only`). It is larger but fully functional; can be swapped for a `--release` binary later if desired.
- Left `sql/migrations/099_team_rosters.sql` untouched as instructed.

## Accomplishments
- `GET /api/v1/{sport}/{entityType}/{id}/headlines` returns 200 with `headlines` array (empty or populated).
- `GET /api/v1/{sport}/leaderboard/headlines` returns the sport-wide ranked board.
- Live frontend at `https://scoracle.com` serves the Headlines scope.
- Rust cognition daemon is actively claiming and persisting the `headlines` stage.

## Quick reference

### Services
```bash
systemctl --user status scoracle-api.service
systemctl --user status scoracle-cognition.service
journalctl --user -u scoracle-api -f
journalctl --user -u scoracle-cognition -f
```

### Useful endpoints
```bash
curl https://api.scoracle.com/api/v1/nba/player/666633/headlines
curl https://api.scoracle.com/api/v1/nba/leaderboard/headlines
```

### Frontend deploy
```bash
cd ~/scoracle/scoracle-frontend
npm run cf:deploy
```

## Files touched
- `scoracle-backend/go/internal/db/db.go` — fixed `headlines_leaderboard` ORDER BY aliases.
- `scoracle-backend/.env.local` — added `headlines` to `COGNITION_STAGES`.
- `~/.config/systemd/user/scoracle-cognition.service` — added `headlines` to `COGNITION_STAGES`.

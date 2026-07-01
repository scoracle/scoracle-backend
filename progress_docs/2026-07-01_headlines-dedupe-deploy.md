# 2026-07-01 — Headlines duplicate cleanup + deploy

## Goal
- Stop the new Headlines product from showing repeated bullets when one source article
  produces multiple model rows or repeated generation runs.
- Verify the reported Chelsea case end-to-end.

## What changed
- Rust `headline` generation now dedupes rows before persistence by:
  - source URL when present;
  - otherwise source name + normalized title.
- Headline inserts use `ON CONFLICT DO NOTHING` so database uniqueness backstops do not
  turn duplicate model output into failed stage work.
- Go headline reads dedupe at query time for:
  - `GET /api/v1/{sport}/{entityType}/{id}/headlines`;
  - `GET /api/v1/{sport}/leaderboard/headlines`.
- Added migration `116_headlines_dedupe` to clean historical duplicates and add unique
  indexes on entity + source URL and entity + source/title.

## Deployment
- Ran `git pull --rebase origin main`; branch was already up to date.
- Applied pending migrations with `./sql/migrate.sh`:
  - `115_model_neutral_ai_labels`;
  - `116_headlines_dedupe`.
- Migration 116 cleanup removed:
  - 365 duplicate rows by source URL;
  - 2 duplicate rows by source/title.
- Validated prepared statements with:
  - `GOCACHE=/tmp/scoracle-go-build go run ./cmd/validate-stmts`
- Deployed backend with:
  - `scripts/hosting/release.sh`
- Release completed successfully:
  - API healthy and serving commit `cb6bc4fc83d6`;
  - `scoracle-cognition` restarted active.

## Chelsea smoke test
- Entity: `FOOTBALL team/18` (`Chelsea`).
- Before API deploy, raw live data showed 12 rows but the new dedupe query collapsed them to 2.
- After migration + deploy:
  - `https://api.scoracle.com/api/v1/football/team/18/headlines` returns 2 headlines.
  - Database count for Chelsea headlines in the 2-day window is 2.
- Remaining rows:
  - The New York Times: Tyrique George / Everton deal.
  - Al Jazeera: Sam Kerr / Gotham FC after Chelsea exit.

## Files
- `rust/src/headline.rs` — generation-time dedupe + insert conflict guard.
- `go/internal/db/db.go` — API read-time dedupe.
- `sql/migrations/116_headlines_dedupe.sql` — cleanup + unique indexes.

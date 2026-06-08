# 2026-06-07 — Leaderboard exposes available_seasons

## Goals
The frontend leaderboard needs a season dropdown for the Rating board. The
endpoint already accepts `?season=` (and the handler parses it), but the response
didn't enumerate which seasons have data — so the dropdown had no options source.

## Decisions
- Additive to the existing `leaderboard` prepared statement: a new `avail_seasons`
  CTE aggregates the DISTINCT seasons with a non-null `rating_composite` for the
  requested sport + entity_type (newest first), surfaced as `available_seasons` in
  the response JSON alongside the already-present resolved `season`. No new
  endpoint, no handler change (season param already wired), no API contract break.

## Accomplishments
- `go/internal/db/db.go` `leaderboard` stmt: added `avail_seasons` CTE +
  `'available_seasons'` output field.
- Validated against prod before restart (PREPARE plans/analyzes the full statement;
  EXECUTE confirmed runtime): NBA player → `season=2025`,
  `available_seasons=[2025..2018]`; FOOTBALL team → `[2025..2020]`; explicit
  `season=2023` resolves to 2023.
- Rebuilt `bin/scoracle-api`, `systemctl --user restart scoracle-api` (clean, active,
  /health 200). Live: `/nba/leaderboard?...` returns `available_seasons`; `&season=2023`
  → Luka Dončić / Jokić / Haliburton.

## Quick reference
```bash
cd go && go build -o bin/scoracle-api ./cmd/api
# validate the stmt before restart (avoid degraded mode):
#   PREPARE lb_test AS <leaderboard stmt>; EXECUTE lb_test('NBA',NULL,'composite',NULL,NULL,5,'player',NULL,NULL);
systemctl --user restart scoracle-api.service
```

## Files
`go/internal/db/db.go` (leaderboard prepared statement).

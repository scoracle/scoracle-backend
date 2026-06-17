# 2026-06-13 — NBA/NFL raster team logos (for the native apps)

## Goal
Serve raster (PNG) team logos for NBA/NFL in the `/meta` bundle so the native
iOS/Android apps can render them. The apps use `AsyncImage` / Coil, which can't
render the Wikimedia SVGs currently stored in `teams.logo_url`.

## Background
- The web renders the SVG logos fine (browsers display SVG natively), so this is
  app-driven — but the fix is in shared data, so both platforms benefit.
- `seed/services/meta/handlers/apisports_images.py` already fetches **api-sports
  PNG team logos** (and player photos), but writes `logo_url` only when NULL.
- NBA/NFL `logo_url` holds Wikimedia SVGs (from `scripts/ops/update_team_logos.sql`),
  so the PNG seeder skips them every run.
- Football already serves PNGs (SportMonks), so it's unaffected.

## The change
- New op: `scripts/ops/null_nba_nfl_team_logos.sql` — clears the Wikimedia SVG
  `logo_url`s for NBA/NFL so the api-sports seeder repopulates PNGs.
- Supersedes `update_team_logos.sql` for NBA/NFL — do **not** re-run that after.

## Runbook (operator)
Requires `DATABASE_PRIVATE_URL` + `API_SPORTS_KEY`. Use the current season year.

1. **Dry-run the image seeder** — confirms every team matches api-sports and the
   logo URLs are PNG (writes nothing; small quota cost):
   ```bash
   scoracle-seed meta images nba --season 2025 --dry-run
   scoracle-seed meta images nfl --season 2025 --dry-run
   ```
   Confirm `teams_unmatched=0` and the logged `would set ... logo_url=...png`.

2. **Clear the SVGs:**
   ```bash
   psql "$DATABASE_PRIVATE_URL" -f scripts/ops/null_nba_nfl_team_logos.sql
   ```
   (NOTICE should report `0` teams still on a Wikimedia SVG.)

3. **Seed the PNGs:**
   ```bash
   scoracle-seed meta images nba --season 2025
   scoracle-seed meta images nfl --season 2025
   ```
   Confirm `team_logos_written` ≈ 30 (NBA) / 32 (NFL), `teams_unmatched=0`.

4. **Verify:**
   ```sql
   SELECT sport,
          COUNT(*) FILTER (WHERE logo_url LIKE '%.png') AS png,
          COUNT(*) FILTER (WHERE logo_url IS NULL)      AS null_logos
   FROM teams WHERE sport IN ('NBA','NFL') GROUP BY sport;
   ```
   Expect `png` ≈ team count, `null_logos = 0` (hand-fix any unmatched team).

5. The Go `/meta` endpoint's in-memory cache refreshes on its TTL — restart the
   API to pick it up immediately. The iOS app then renders NBA/NFL logos with no
   app change (it already filters non-raster URLs and hydrates from `/meta`).

## Notes
- **Player photos**: NBA/NFL players have no api-sports photo, so they stay NULL
  and the apps fall back to the (now PNG) team logo — matching the web.
- **Frontend**: the web's bundled meta JSON (Workers Static Assets) regenerates
  from the DB on its next build; until then the web keeps SVG (renders fine).
- **Tradeoff**: SVG (crisp, scalable) → PNG (universal). Deliberate, for
  cross-platform parity + share-card image rendering.

## Files
- New: `scripts/ops/null_nba_nfl_team_logos.sql`,
  `progress_docs/2026-06-13_nba-nfl-raster-logos.md`.

## Status
Authored, **not executed** — the operator runs the runbook above (DB write +
api-sports quota). Surfaced from the iOS MetaShell-images work (scoracle-ios #5).

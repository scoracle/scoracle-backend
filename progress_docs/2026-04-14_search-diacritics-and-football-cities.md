# Search: Diacritic Normalization + Football Team Cities

**Date:** 2026-04-14

## Problems

1. Searching "estevao" returned no results for the football player "Estêvão" — tokens
   stored diacritics verbatim so substring matching failed.
2. Football teams had no `city` populated (99/99), so city search (e.g. "munich" → FC Bayern)
   never matched. The SportMonks parser read `venue.city` but the API actually returns
   `venue.city_name`.

## Fixes

### 1. Unaccent extension + view tokens

- Enabled `unaccent` extension in `sql/shared.sql`.
- Each `*.autofill_entities` materialized view (NBA, NFL, Football) now emits both
  the original `LOWER(...)` form AND an `unaccent(LOWER(...))` form for player name
  tokens (first_name, last_name, name_no_spaces) and team name tokens
  (name_no_spaces, city). Both diacritic and ASCII-only queries match.

### 2. SportMonks football team city

- `_parse_team()` now reads `venue.city_name` (falling back to `city`) and sets
  both `Team.city` and `meta["venue_city"]`. Previously only stored `meta["venue_city"]`,
  and even that was empty because of the wrong field name.

### 3. Backfill

- One-shot `tmp/backfill_football_cities.py` walked `teams WHERE sport='FOOTBALL' AND
  city IS NULL`, hit SportMonks `/teams/{id}?include=venue`, and wrote `venue.city_name`
  into `teams.city`. **97 of 99 football teams updated** (2 had no venue city in provider).

## Verification (production DB)

| Query | Match |
|-------|-------|
| token `"estevao"` | Estêvão (player 37701999) |
| token `"munich"` | FC Bayern München (team 503) |
| token `"detroit"` | Pistons (team 9) |

Sample Bayern tokens: `["fcbayernmünchen", "fcb", "munich", "germany", "bundesliga", "fcbayernmunchen", "munich"]`.

## Files Changed

| File | Change |
|------|--------|
| `sql/shared.sql` | `CREATE EXTENSION unaccent` |
| `sql/nba.sql` | Unaccented name tokens in both player & team sides of view |
| `sql/nfl.sql` | Same |
| `sql/football.sql` | Same |
| `seed/services/event/handlers/sportmonks_football.py` | Read `venue.city_name`, populate `Team.city` |

## Follow-ups

- City not leaked into player tokens — players only carry `team_name`, and for NBA/NFL
  that's just the mascot ("Pistons", not "Detroit Pistons"), so city search still matches
  only the team entity. For football, team name frequently contains the city ("Leicester
  City") — acceptable since that's the canonical team name.
- Frontend must re-fetch `/meta` to pick up the new tokens (or invalidate its client cache).

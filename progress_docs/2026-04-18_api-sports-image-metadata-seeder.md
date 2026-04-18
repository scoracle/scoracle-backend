# api-sports image metadata seeder (NBA + NFL)

**Date:** 2026-04-18

## Goal

Populate `teams.logo_url` and `players.photo_url` for NBA and NFL,
which BDL does not provide. Do it without disturbing the existing
event/stat pipeline and without burning the 100-call/day free tier.

## Context

Earlier in the session we evaluated api-sports as a replacement for
BDL (NBA/NFL) and SportMonks (football) event data. Result:

- **Football:** api-sports requires 4–5 calls per fixture (no `include`
  support). SportMonks bundles the same coverage into 1 call. Keep
  SportMonks.
- **NBA:** api-sports requires 2 calls per game (teams + players). BDL
  does it in 1. Keep BDL.
- **Metadata gap:** BDL returns no team logos or player photos.
  api-sports does, and image URLs served from `media.api-sports.io`
  don't count toward the quota. Cheap win.

So api-sports is narrowly scoped to **images only**. Event data stays
with BDL/SportMonks.

## Decisions

1. **New command lives under `meta`, not `event`.** Runs once per
   season, not continuously. Clean separation from the stat pipeline.
2. **`provider_entity_map` is the bridge.** api-sports IDs never leak
   into the canonical tables; matching happens once, mapping persists
   forever. Subsequent runs are O(1) lookups.
3. **Match order:** existing map row → team short_code → normalized
   team name → player (first+last+team) with DOB tiebreaker. Anything
   unmatched gets logged, not hard-failed.
4. **Only set columns when NULL.** `logo_url` / `photo_url` never
   overwrite existing values — keeps the seeder idempotent and
   protects against stomping a future better source.
5. **`--dry-run` flag.** Runs the full match + logs would-be writes.
   Still consumes API quota (fetches are required to see shapes), but
   nothing hits the DB. Useful for verifying the exact player-photo
   field name on first run (api-sports docs are JS-rendered and hard
   to scrape; we'd rather see the real payload).
6. **Polite 1s throttle** even though the free tier has no per-minute
   cap. The quota is daily; we just don't want to thrash.

## Accomplishments

- `seed/shared/apisports_client.py` — minimal httpx client, `x-apisports-key`
  auth, 1s throttle, logs `x-ratelimit-requests-remaining` on every call.
- `seed/services/meta/handlers/apisports_images.py` — matcher + seeder.
  Shared `_seed_images()` core powers both `seed_nba_images()` and
  `seed_nfl_images()`.
- `seed/services/meta/cli.py` — new `images` subcommand (`nba` / `nfl`,
  `--season`, `--dry-run`).
- `seed/shared/config.py` — `api_sports_key` field reads `API_SPORTS_KEY`.
- `seed/META_ENDPOINTS.md` — documented the api-sports surface.

## Quick reference

```bash
# First time — dry run to see what matches and what the player
# payload actually looks like. Consumes ~31 calls for NBA.
scoracle-seed meta images nba --season 2025 --dry-run

# Real run. Writes provider_entity_map + logo/photo URLs.
scoracle-seed meta images nba --season 2025
scoracle-seed meta images nfl --season 2025
```

Expected call cost per run:

| Sport | Calls | Why |
|---|---:|---|
| NBA | ~31 | 1 /teams + ~30 /players per team |
| NFL | 33 | 1 /teams + 32 /players per team |

Free tier is 100/day. Running NBA + NFL back-to-back = ~64 calls,
leaves headroom.

## Files changed

- `seed/shared/config.py` — added `api_sports_key`
- `seed/shared/apisports_client.py` — new
- `seed/services/meta/handlers/__init__.py` — new (package marker)
- `seed/services/meta/handlers/apisports_images.py` — new
- `seed/services/meta/cli.py` — added `images` subcommand
- `seed/META_ENDPOINTS.md` — documented api-sports images section

## Follow-ups

- Verify player-photo field name on first `--dry-run`. The handler
  already probes `photo` / `image` / `picture` / `headshot` /
  `image_url` / `photo_url` — whichever is populated wins. If none
  match, the `_extract_player_photo` function needs one more key added
  based on the actual payload.
- NFL league id is hard-coded to `1`. If api-sports ever changes that
  (unlikely — it's their NFL constant), flip the constant in
  `apisports_images.py:NFL_LEAGUE_ID`.
- If we ever want college/draft info from api-sports, the same
  `/players` call already returns it — could extend `upsert_player`
  writes to COALESCE those fields too. Not doing it now.

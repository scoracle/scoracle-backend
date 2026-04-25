# Team full names + logo backfill

## Goal

Two team-data cleanups:

1. `public.teams.name` was storing nicknames only ("Hawks", "Falcons") for
   NBA and NFL, which caused downstream display issues. The provider
   (BallDontLie) returns both `name` and `full_name`; the seeder was
   stashing `full_name` in `meta` and writing the nickname to
   `teams.name`. Goal: store the full name as the primary name.
2. The fresh local DB had `logo_url = NULL` for every team. The old Neon
   instance still had logos for ~half of them. Backfill what we can.

## Decisions

**Names — fix at the seeder, not at read-time.** The seeder is supposed
to be a thin pass-through (per CLAUDE.md). Picking nickname over
`full_name` was an unnecessary transformation. Switching the source
field is the smallest possible change and keeps SQL/views/handlers
untouched. SportMonks (football) already returns full club names, so
only the BDL handlers needed the fix.

**Preserve the nickname for search.** News matching relies on
`search_aliases` containing nickname-only strings ("Hawks beat Knicks").
Stash the nickname in `meta.short_name` and have
`generate_team_aliases` pull it the same way it already pulls
`meta.full_name`. Aliases now contain `[short_code, nickname]` for every
NBA/NFL team.

**Schema is already normalized — leave it alone.** While digging, I
noticed `nba.team` / `nfl.team` show three rows per team in the DB
viewer. They're **views** that LEFT JOIN `public.teams` (1 row per team)
with `public.team_stats` (1 row per team-season). Storage is not
duplicated; the row multiplication is just how the view projects when
multiple seasons exist. Profile prepared statements already filter
`ORDER BY season DESC LIMIT 1`. No refactor needed.

**Logo transfer — name-only matching.** The Neon DB has corrupt
`sport_id` labels (NBA teams duplicated under `sport_id='NFL'` with the
logos sitting on the NFL-labeled rows). Original `scripts/transfer_logos.py`
matched by `(sport, short_code)` and produced cross-sport collisions
(Hawks → Falcons via "ATL"). Patched it to drive from the local side and
match by normalized `city + name` against Neon's `name`, ignoring Neon's
`sport_id` entirely. 34 of 62 teams matched cleanly; the rest were never
in Neon and need to be sourced manually.

## Accomplishments

- `seed/services/event/handlers/bdl_nba.py` — `_parse_team()` now uses
  `raw["full_name"]` for `Team.name` and stashes nickname in
  `meta.short_name`.
- `seed/services/event/handlers/bdl_nfl.py` — same change.
- `seed/shared/aliases.py` — `generate_team_aliases()` now also pulls
  `meta.short_name` into the alias list (mirrors existing `full_name`
  handling).
- `scripts/transfer_logos.py` — rewrote matching strategy (drive from
  local, match by `city + name` against Neon, ignore Neon's
  `sport_id`). Applied: 34 logos transferred to `public.teams.logo_url`.
- Re-ran `meta seed nba` + `meta seed nfl` for season 2025; all 30 NBA
  + 32 NFL rows now have full names.

## Verification

Before:
```
(1, 'NBA', 'Hawks',    'Atlanta')
(1, 'NFL', 'Falcons',  'Atlanta')
```

After re-seed:
```
(1, 'NBA', 'Atlanta Hawks',    'Atlanta', ['ATL', 'Hawks'])
(1, 'NFL', 'Atlanta Falcons',  'Atlanta', ['ATL', 'Falcons'])
```

`nba.team` view sample (still one row per team-season, full name now
flows through):
```
(1, 'Atlanta Hawks',  'Atlanta', 2025)
(1, 'Atlanta Hawks',  'Atlanta', 2024)
(1, 'Atlanta Hawks',  'Atlanta', 2023)
```

Sanity check (city always contained in name): 0 mismatches across all
62 NBA+NFL rows.

## Quick reference

```bash
# Re-seed team metadata after a name-format change
scoracle-seed meta seed nba --season 2025
scoracle-seed meta seed nfl --season 2025

# Logo backfill (one-shot, dry-run by default)
NEON_URL='postgres://...' python scripts/transfer_logos.py
NEON_URL='postgres://...' python scripts/transfer_logos.py --apply
```

```sql
-- Check name + alias coverage
SELECT name, search_aliases, meta->>'short_name' AS short_name
FROM public.teams WHERE id = 1 AND sport = 'NBA';
-- ('Atlanta Hawks', {ATL, Hawks}, 'Hawks')

-- Find teams still missing a logo
SELECT sport, name FROM public.teams
WHERE sport IN ('NBA','NFL') AND logo_url IS NULL
ORDER BY sport, name;
```

## Follow-ups

- 28 teams still have `logo_url = NULL` (most NFL teams + Jazz, Falcons,
  Panthers). Neon never had logos for these; user is sourcing manually.
- Frontend may need an adjustment if it currently composes `city + name`
  for display headings (would now read "Atlanta Atlanta Hawks"). The API
  contract for `entity.name` is now the full canonical name.
- `meta.full_name` is technically redundant now that it equals
  `name` — left in place for backwards-compat with anything reading it.
- `transfer_logos.py` is a one-shot. Once the remaining logos are sourced,
  it can be deleted.

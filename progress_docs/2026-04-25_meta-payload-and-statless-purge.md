# Meta payload completeness + stat-less purge automation

## Goal

Three related cleanups to the NBA + NFL meta surface:

1. Replace scrambled team logo URLs (Atlanta Falcons was pointing at the
   Spurs logo, etc.) with sourced Wikimedia URLs.
2. Stop serving up the entire BDL historical roster on `/meta`. NBA was
   returning 5,534 player entities and NFL 10,966, while only 827 / 2,675
   had ever logged a stat in the seeded seasons.
3. Make `/meta` serve every available data point per entity (frontend
   curates display, backend doesn't editorialize).

## Decisions

**Logos — direct UPDATE, not a migration.** The change is data-only and
re-applies cleanly; lives at `scripts/ops/update_team_logos.sql`. NFL all
matched on name; NBA's "LA Clippers" needed a one-off mapping
(markdown said "Los Angeles Clippers"). 5 source URLs were broken at
fetch time (Cavaliers, Pistons, Knicks, Jazz, Titans) — replaced with
working Wikimedia paths.

**Stat-less purge happens in the BDL provider shim, not in the CLI.**
The purge trigger sits inside `_seed_nba_metadata` / `_seed_nfl_metadata`
in `seed/services/meta/cli.py` so a future provider swap (e.g. NBA on
api-sports) replaces the shim and the purge goes with it. The CLI is
provider-agnostic. Football's SportMonks shim doesn't trigger a purge —
SportMonks's roster is season-scoped, no historical bloat.

**Rookie exemptions are sport-specific, derived from BDL meta.**
- NBA: `meta.draft_year = sports.current_season` (current draft class).
- NFL: `meta.experience ILIKE 'rookie%'` (BDL's own label).
- BDL doesn't return `draft_year` for NFL or `experience` for NBA, so
  the signals don't overlap. NFL had 478 rookies preserved; NBA had 2.

**Matview filter mirrors the purge filter exactly.** Same WHERE clause
shape on `nba.autofill_entities` and `nfl.autofill_entities`. Even if
the physical purge ran late, the API never sees stat-less rows. This is
load-bearing — the matview filter is the real user-facing guarantee;
the DELETE is housekeeping.

**Full meta blob passes through — no curation in the matview.**
Replaced `jsonb_build_object('jersey_number', …, 'college', …, …)` with
`COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name',
p.name)`. Whatever BDL returns now reaches the frontend without a
schema change to add new keys.

**Capture every BDL field, store the raw response too.** Both NBA and
NFL `_parse_player` now mirror every non-promoted key into `meta` and
pass the unmodified payload as `Player.raw`. `upsert_player` persists
that as `players.raw_response`. Two consequences: (a) new BDL fields
appear automatically without parser edits, (b) historic queries against
the raw payload are possible without re-seeding.

**Skipped migration 010** (`team_logo_url` denormalized onto player
rows). Frontend can resolve player → team logo via `team_id` lookup in
the same `/meta` payload; saves a matview swap and keeps `photo_url`
free for actual player headshots when api-sports lands later.

## Accomplishments

### Logos
- `scripts/ops/update_team_logos.sql` — UPDATE for 30 NBA + 32 NFL
  teams, transactional, with a guard that fails the run if any row ends
  with a non-Wikimedia URL.

### Stat-less purge
- `seed/services/meta/cli.py` — `_purge_statless()` helper + sport-aware
  rookie clauses; `purge-inactive` rewritten to use it; `meta seed`
  shims own the trigger via `purge_statless=True` default. Purge happens
  inside the BDL shim; CLI just dispatches and logs the count.
- `nba.autofill_entities` / `nfl.autofill_entities` rebuilt with the
  EXISTS-stats-OR-rookie filter (`scripts/ops/purge_statless_autofill.sql`).
- Canonical `sql/nba.sql` + `sql/nfl.sql` updated to inherit the filter
  on a fresh DB rebuild.

### Full meta passthrough
- `seed/shared/models.py` — `Player.raw` field added.
- `seed/shared/upsert.py` — `upsert_player` writes `raw_response`.
- `seed/services/event/handlers/bdl_nba.py` — `_parse_player` captures
  every BDL field into `meta`, passes raw through.
- `seed/services/event/handlers/bdl_nfl.py` — same. Also sets
  `Player.detailed_position` from `position_abbreviation`.
- Matview `meta` column passes `p.meta || {display_name}` through
  verbatim; no curated key list.

## Verification

Final state on local DB after re-seed + auto-purge:

```
 sport | players | with_raw_response | total_entities (/meta)
-------+---------+-------------------+------------------------
 NBA   |    829  |       829 (100%)  |  859 (829 players + 30 teams)
 NFL   |  3,152  |     3,152 (100%)  | 3184 (3152 players + 32 teams)
```

Per-row coverage:
- Team logos: 30/30 NBA + 32/32 NFL → all `upload.wikimedia.org`, all 200.
- Player → `team_id`: 829/829 NBA + 3,152/3,152 NFL.

Sample `/meta` player payloads:
```json
// NBA
{"name":"Aaron Gordon", "team_id":8, "team_abbr":"DEN", "team_name":"Denver Nuggets",
 "meta":{"college":"Arizona","draft_year":2014,"draft_round":1,
         "draft_number":4,"jersey_number":"32","display_name":"Aaron Gordon"}}

// NFL
{"name":"Aaron Banks", "team_id":22, "team_abbr":"GB", "team_name":"Green Bay Packers",
 "meta":{"age":28,"college":"Notre Dame","experience":"5th Season",
         "jersey_number":"65","position_abbreviation":"G",
         "display_name":"Aaron Banks"}}
```

## Provider-agnostic boundary

```
                     ┌───────────────┐
   meta seed ──────► │  CLI dispatch │  (provider-agnostic)
                     └───────┬───────┘
                             │
       ┌─────────────────────┼─────────────────────────────┐
       ▼                     ▼                             ▼
  _seed_nba_metadata   _seed_nfl_metadata    _seed_football_metadata
  (BDL shim)            (BDL shim)            (SportMonks shim)
       │                     │                             │
       ├── BDL fetch         ├── BDL fetch                 ├── SportMonks
       ├── upsert            ├── upsert                    ├── upsert
       └── _purge_statless   └── _purge_statless           └── (no purge)
```

If we swap NBA off BDL: replace `_seed_nba_metadata`. The CLI doesn't
change. The new shim chooses whether to call `_purge_statless` based on
the new provider's behavior.

## Quick reference

```bash
# Re-seed metadata (auto-purges stat-less rows for NBA/NFL)
scoracle-seed meta seed nba --season 2025
scoracle-seed meta seed nfl --season 2025

# Skip the auto-purge (e.g. when meta-seeding before any event-seed)
scoracle-seed meta seed nba --season 2025 --no-purge-statless

# Standalone purge (idempotent, rookie-aware)
scoracle-seed meta purge-inactive nba --grace-days 0
scoracle-seed meta purge-inactive nfl --grace-days 0
```

```sql
-- Check coverage after a re-seed
SELECT sport,
       COUNT(*) AS players,
       COUNT(*) FILTER (WHERE raw_response IS NOT NULL) AS with_raw
FROM players WHERE sport IN ('NBA','NFL') GROUP BY sport;

-- Manually re-apply matview filter (e.g. after a fresh DB build)
\i scripts/ops/purge_statless_autofill.sql
```

## Follow-ups

- BDL doesn't return `date_of_birth` for either sport, `nationality`
  for NFL, or `experience` / `age` for NBA. To close those gaps we'd
  need a 2nd source — `seed/services/meta/handlers/apisports_images.py`
  already integrates with api-sports for photos; extending it (or
  splitting off a `meta enrich` command) would bring DOB + nationality.
- Player `photo_url` is intentionally left empty — reserved for
  api-sports headshots when that integration lands.
- `scripts/ops/update_team_logos.sql` is a one-shot; Wikimedia URLs
  drift over time (rebrands, CDN path moves). Consider a periodic
  sanity check that HEADs each URL and flags 404s.

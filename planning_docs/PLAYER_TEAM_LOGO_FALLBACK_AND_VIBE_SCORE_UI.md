# Player Team-Logo Fallback + Vibe Score UI

Date: 2026-04-24
Touches: scoracle-data (SQL), Scoracle frontend (Solid components)

## Goals

1. **Player image fallback.** NBA and NFL player rows have no headshot URL.
   Wire the parent team's crest through the meta pipeline so the profile
   widget can display the team logo when `photo_url` is empty. Wire only —
   team logos themselves are still being backfilled on the backend; the
   fallback goes live the moment that data lands.
2. **Vibe tab visual refresh.** The Vibe pivot from blurb → 1-100 score
   (migration 009) left the tab still rendering the legacy blurb shape.
   Replace it with a large emoji + numeric score, matching the visual
   weight of the pizza chart, and surface a distinct empty state when the
   score is `null` (insufficient news to grade).

## Decisions

### Backend — autofill_entities materialized view

- Add `team_logo_url` column to `nba.autofill_entities`,
  `nfl.autofill_entities`, and `football.autofill_entities`.
  - Player rows: `t.logo_url AS team_logo_url`.
  - Team rows: `NULL::text AS team_logo_url` (the team's logo is already
    surfaced via the existing `photo_url` column for team entries).
- Materialized views can't gain a column via `REFRESH CONCURRENTLY`, so the
  rollout is a `DROP MATERIALIZED VIEW … CREATE MATERIALIZED VIEW …`
  migration — `010_autofill_team_logo.sql`.
- Canonical view bodies in `sql/{nba,nfl,football}.sql` were updated to
  match, so a fresh schema bootstrap stays in sync.

### Frontend — meta build + EntityMeta widget

- `Scoracle/scripts/fetch-autofill.mjs` reads `item.team_logo_url` from the
  Go meta endpoint and threads it onto `player.team.logo_url` in the
  generated `{sport}-meta.json` files. `TeamReference.logo_url` already
  existed in `src/lib/types/index.ts`, so no type changes were needed.
- `EntityMeta.tsx::resolvePlayer` falls back to `meta.team?.logo_url` when
  `meta.photo_url` is empty. Pure UI fallback — no API contract change.

### Frontend — VibesTab rebuild

- Five score buckets, each 20 points wide:

  | Score   | Emoji | Tier label    |
  | ------- | ----- | ------------- |
  | 1–20    | 😞    | Down bad      |
  | 21–40   | 😟    | Cooling off   |
  | 41–60   | 😐    | Neutral       |
  | 61–80   | 🙂    | Trending up   |
  | 81–100  | 🤩    | On fire       |

- Tier accents reuse the existing `--percentile-*` palette so the score
  card matches pizza chart hue language.
- Each tier carries 4–5 randomized blurbs templated with the entity name
  and, when present, the team name (e.g. `"People are pumped on
  {team} {name}!"`). The pick is held in a `createMemo` keyed off
  `(vibe, names)` so the blurb is stable for the page load and only
  rerolls if the data identity changes.
- Empty states:
  - **Fetch error / 404** → existing "Model is training" placeholder.
  - **`sentiment === null`** → lonely-robot variant (eyes dropped, frown
    arc) with "Not enough news yet" copy. The robot SVG is parameterized
    with a `lonely` flag rather than duplicated.
- Emoji sized via `clamp(8rem, 32vw, 13rem)` so the card occupies real
  estate similar to the 360 px pizza chart on the stats side.

## Files Touched

### scoracle-data

- `sql/nba.sql` — added `team_logo_url` column to player + team branches
  of the autofill MV (canonical body).
- `sql/nfl.sql` — same.
- `sql/football.sql` — same.
- `sql/migrations/010_autofill_team_logo.sql` — new. DROP + CREATE all
  three MVs with the new column. Apply with:

  ```bash
  psql $DATABASE_PRIVATE_URL -f sql/migrations/010_autofill_team_logo.sql
  ```

### Scoracle (frontend)

- `scripts/fetch-autofill.mjs` — pass `team_logo_url` into
  `player.team.logo_url`.
- `src/components/solid/EntityMeta.tsx` — `logoUrl` falls back to
  `meta.team?.logo_url`.
- `src/components/solid/VibesTab.tsx` — full rebuild around the sentiment
  score, tier emoji, randomized blurbs, and lonely-robot null state.
- `src/components/solid/VibesTab.css` — emoji sizing, tier accents,
  layout, mobile breakpoint.

## Rollout Steps

1. Apply migration 010 against the DB:
   `psql $DATABASE_PRIVATE_URL -f sql/migrations/010_autofill_team_logo.sql`.
2. From the Scoracle frontend repo, regenerate the meta JSON so the new
   `team_logo_url` field appears in `public/data/{sport}-meta.json`:
   `node scripts/fetch-autofill.mjs`.
3. Backfill team logos on the backend (separate workstream — already in
   progress). Once a team's `logo_url` is populated, the fallback will
   light up automatically on every player profile for that team.
4. Verify Vibe tab renders correctly for:
   - a high-score entity (🤩 with hype blurb),
   - a low-score entity (😞 with down-bad blurb),
   - an entity with no vibe row (model is training),
   - an entity whose latest row has `sentiment IS NULL` (lonely robot).

## Notes

- The fallback is intentionally invisible until team logos populate. We
  preferred wiring everything end-to-end now over staging the change;
  it's a one-line UI tweak with no behavior change in the meantime.
- The robot SVG was kept inline (not promoted to a shared icon) to avoid
  pulling icon infrastructure for a single, intentionally-quirky
  illustration. Revisit if a third use case appears.

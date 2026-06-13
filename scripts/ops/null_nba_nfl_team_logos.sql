-- Null NBA + NFL team logo_url so the api-sports image seeder repopulates them
-- with raster PNGs.
--
-- Why: update_team_logos.sql installed Wikimedia source SVGs. Browsers render
-- SVG natively (the web is fine) — but the native iOS/Android apps use
-- AsyncImage / Coil, which can't render SVG. api-sports already provides PNG
-- team logos (seed/services/meta/handlers/apisports_images.py fetches them), but
-- that seeder writes logo_url only when NULL, so it skips the SVGs every run.
-- This clears them so the next `scoracle-seed meta images {nba,nfl}` fills PNGs.
--
-- Deliberate SVG -> PNG swap (crisp/scalable vs universal/raster). For a
-- web + iOS + Android product raster is the pragmatic choice, and it's required
-- to draw logos into share-card images later.
--
-- The Go /meta endpoint reads logo_url from SQL with an in-memory cache, so the
-- bundle reflects the change on the next cache refresh (or an API restart) — no
-- static bundle to regenerate for the app.
--
-- Supersedes update_team_logos.sql for NBA/NFL: do NOT re-run that after this
-- (it reinstalls SVGs and asserts every team has one).
--
-- Run:  psql "$DATABASE_PRIVATE_URL" -f scripts/ops/null_nba_nfl_team_logos.sql
-- Then: scoracle-seed meta images nba --season <YEAR>
--       scoracle-seed meta images nfl --season <YEAR>

BEGIN;

UPDATE teams
SET logo_url = NULL,
    updated_at = NOW()
WHERE sport IN ('NBA', 'NFL')
  AND logo_url LIKE 'https://upload.wikimedia.org/%';

-- Report what's left (should be 0 NBA/NFL teams still on a Wikimedia SVG).
DO $$
DECLARE
    remaining INT;
BEGIN
    SELECT COUNT(*) INTO remaining
    FROM teams
    WHERE sport IN ('NBA', 'NFL')
      AND logo_url LIKE 'https://upload.wikimedia.org/%';
    RAISE NOTICE 'NBA/NFL teams still on a Wikimedia SVG: %', remaining;
END $$;

COMMIT;

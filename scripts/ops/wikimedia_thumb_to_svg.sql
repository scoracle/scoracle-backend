-- Wikimedia tightened their hotlink policy: thumbnail URLs of the form
-- /wikipedia/<en|commons>/thumb/<a/bc>/<name.svg>/NNNpx-<name>.svg.png
-- now return HTTP 400 with body
--   "Use thumbnail steps listed on https://w.wiki/GHai"
-- The original SVGs at the un-thumbnailed paths still return 200, and
-- browsers render SVG natively at any CSS size — no thumbnail needed.
--
-- This script rewrites every NBA + NFL team logo from the broken thumb
-- form to the source SVG path:
--   .../wikipedia/en/thumb/2/24/Atlanta_Hawks_logo.svg/150px-Atlanta_Hawks_logo.svg.png
--   ↓
--   .../wikipedia/en/2/24/Atlanta_Hawks_logo.svg
--
-- Re-runnable: the regex only matches the broken form, so source URLs
-- are left alone.
--
-- Run: psql "$DATABASE_PRIVATE_URL" -f scripts/ops/wikimedia_thumb_to_svg.sql

BEGIN;

UPDATE teams
SET logo_url = regexp_replace(
        logo_url,
        '/thumb/([0-9a-f]/[0-9a-f]{2}/[^/]+\.svg)/[0-9]+px-[^/]+\.png$',
        '/\1'),
    updated_at = NOW()
WHERE sport IN ('NBA','NFL')
  AND logo_url ~ '/thumb/.*\.svg/[0-9]+px-.*\.png$';

-- Sanity guard: every NBA + NFL row must end in '.svg' on upload.wikimedia.org
DO $$
DECLARE
    bad INT;
BEGIN
    SELECT COUNT(*) INTO bad
    FROM teams
    WHERE sport IN ('NBA','NFL')
      AND (logo_url IS NULL
           OR logo_url NOT LIKE 'https://upload.wikimedia.org/wikipedia/%.svg');
    IF bad > 0 THEN
        RAISE EXCEPTION 'Aborting: % NBA/NFL teams have a non-SVG logo_url', bad;
    END IF;
END $$;

COMMIT;

REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;

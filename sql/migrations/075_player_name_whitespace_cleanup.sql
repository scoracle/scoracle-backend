-- ============================================================================
-- 075 — Strip whitespace (incl. trailing U+00A0 nbsp) from player names.
--
-- SportMonks `display_name` ships some football players with a trailing
-- non-breaking space (U+00A0) — 153 rows incl. Harry Kane, Kevin De Bruyne,
-- Jordan Henderson. The seeder's _parse_player used display_name verbatim (only
-- the firstname+lastname FALLBACK path called .strip()), so the nbsp survived
-- into players.name and rendered as a stray trailing space (e.g. the share post
-- "Check out Harry Kane 's report"). The seeder is fixed forward in this commit
-- (strip display_name in sportmonks_football._parse_player); this migration
-- backfills the rows already in the table.
--
-- The nbsp here is purely leading/trailing (verified: 0 names carry an INTERNAL
-- nbsp), so btrim over {space, nbsp, tab, LF, CR} exactly replicates Python
-- str.strip() for this data. Data-only: no function/column change, no recompute,
-- no Go prepared-statement impact → no API restart needed. NBA/NFL names are
-- already clean (0 dirty); the WHERE predicate self-limits to dirty rows, so the
-- statement is sport-agnostic and a no-op everywhere already clean.
--
-- NOTE: this fixes players.name in the DB / Go /meta output immediately. The
-- frontend autocomplete bundle (public/data/football*.json) bakes the old names
-- in — regenerate it (node scripts/fetch-autofill.mjs) + redeploy to surface the
-- clean names client-side.
--
-- Apply with: ./sql/migrate.sh
-- ============================================================================

BEGIN;

UPDATE players
SET name = btrim(name, ' ' || chr(160) || chr(9) || chr(10) || chr(13)),
    updated_at = now()
WHERE name IS DISTINCT FROM btrim(name, ' ' || chr(160) || chr(9) || chr(10) || chr(13));

DO $$
DECLARE v_dirty INTEGER;
BEGIN
    SELECT count(*) INTO v_dirty
    FROM players
    WHERE name ~ '^\s' OR name ~ '\s$' OR name LIKE '%' || chr(160) || '%';
    IF v_dirty > 0 THEN
        RAISE EXCEPTION '075 FAIL: % dirty player names remain', v_dirty;
    END IF;
    RAISE NOTICE '075 OK: player names trimmed (leading/trailing whitespace + nbsp stripped)';
END $$;

COMMIT;

-- 230_seeder_era_provider_map_demolition.sql
--
-- Removes the last of the paid-provider seeding layer from the box-score path.
--
-- Scott, 2026-08-23: "We used to have a paid third party data provider we seeded from. Since
-- we're only box score derived now (public event, no licensing requirement) I want that
-- scraped from public websites… the old seeding layer was pruned."
--
-- Mig 222 already dropped `provider_entity_map` (its driver was `seed/shared/upsert.py`, named
-- in mig 215's DRIVER comment). This finishes the job for its fixture-grain twin, which mig 222
-- left standing because the box-score path still read it.
--
-- ## Why this is safe to drop rather than repair
--
-- `provider_fixture_map` holds 29,537 rows (12,561 FOOTBALL / 10,202 NBA / 6,774 NFL) and — the
-- claim that actually matters — **zero for any of the 159 queued box-score fixtures**, verified
-- against prod 2026-08-23.
--
-- It is NOT empty for season 2026, and an earlier draft of this comment wrongly said it was:
-- 272 NFL rows survive. Every one is a `scheduled` fixture kicking off 2026-09-09 .. 2027-01-10,
-- written in a single batch on 2026-06-22 at schedule release. None is completed, so none was
-- ever a box-score candidate — which is why the operative zero above still holds. They are
-- forfeited here deliberately: the vendor clients are deleted and `BALLDONTLIE_API_KEY` is
-- unset, so a provider id buys nothing that public-source discovery will not have to find on
-- its own anyway. Data dumped to `archbox:~/scoracle_backups/provider_fixture_map_20260823.sql`
-- (29,537 column-INSERTs, verified) before this ran, so the forfeit is recoverable.
--
-- Nothing in any repo writes this table — a grep across all ten scoracle repos finds only DDL,
-- a dead resolver, and comments — so the residue cannot regrow. Its only consumers were:
--
--   1. `resolve_provider_fixture_id()` — dead, nothing calls it. Dropped here.
--   2. `enqueue_fixture_boxscore_on_provider_map` (mig 189) — a trigger that enqueued box-score
--      work when a mapping appeared. With no writer it can never fire again. Dropped here.
--   3. `fixture_boxscore_input_version()` — LIVE, called by `enqueue_fixture_boxscore` (mig
--      225). Re-issued below without the provider leg.
--   4. `boxscore.rs::load_fixture`'s LEFT JOIN — removed in the same commit.
--
-- The box-score work rows keep arriving through `enqueue_fixture_boxscore`, which triggers on
-- fixture status and never needed a mapping. That path is untouched.
--
-- ## The fingerprint changes on purpose, and the prefix says so
--
-- `fbf1:` → `fbf2:`. The provider leg leaves the string, so every fixture's `input_version`
-- moves. That is intentional and harmless: the 159 outstanding rows are all `pending`, and mig
-- 225's conflict clause preserves a pending row's `available_at`, so they keep their FIFO place
-- and are not restamped to the back of the line. Bumping the prefix rather than silently
-- reshaping the string is the same discipline as `pk:` and `rating:s…` — a fingerprint whose
-- meaning changed should not be mistaken for one that drifted.

BEGIN;

-- 1. The trigger and its function. Ordering matters only for legibility; dropping the table
--    would take the trigger with it, but naming it here is what makes the demolition auditable.
DROP TRIGGER IF EXISTS fixture_boxscore_enqueue_on_provider_map ON public.provider_fixture_map;
DROP FUNCTION IF EXISTS public.enqueue_fixture_boxscore_on_provider_map();

-- 2. The dead resolver.
DROP FUNCTION IF EXISTS public.resolve_provider_fixture_id(integer, text, text);

-- 3. Re-issue the live fingerprint without the provider leg. Everything else is byte-identical
--    to the incumbent.
CREATE OR REPLACE FUNCTION public.fixture_boxscore_input_version(p_fixture_id integer)
    RETURNS text
    LANGUAGE sql
    STABLE
AS $function$
    SELECT 'fbf2:' ||
           f.sport || ':' ||
           f.season::text || ':' ||
           COALESCE(f.league_id, 0)::text || ':' ||
           f.home_team_id::text || ':' ||
           f.away_team_id::text || ':' ||
           COALESCE(f.home_score::text, '') || ':' ||
           COALESCE(f.away_score::text, '')
    FROM public.fixtures f
    WHERE f.id = p_fixture_id;
$function$;

COMMENT ON FUNCTION public.fixture_boxscore_input_version(integer) IS
    'Queue fingerprint for a fixture box-score demand: sport, season, league, both teams and '
    'the final score. fbf2 (mig 230) dropped the provider-map leg with the seeding layer — the '
    'score is what says "this fixture is final and these are its numbers", which is the whole '
    'signal the fingerprint needs. Public-source discovery replaces the provider id.';

-- 4. The table itself.
DROP TABLE IF EXISTS public.provider_fixture_map;

-- 5. Assert the demolition actually happened before committing (the 045 parity-gate habit; a
--    silent no-op here is a cleanup that half-landed).
DO $$
BEGIN
    IF to_regclass('public.provider_fixture_map') IS NOT NULL THEN
        RAISE EXCEPTION 'demolition aborted: provider_fixture_map still exists';
    END IF;
    IF to_regproc('public.resolve_provider_fixture_id') IS NOT NULL THEN
        RAISE EXCEPTION 'demolition aborted: resolve_provider_fixture_id still exists';
    END IF;
    IF public.fixture_boxscore_input_version(
           (SELECT id FROM public.fixtures ORDER BY id LIMIT 1)) NOT LIKE 'fbf2:%' THEN
        RAISE EXCEPTION 'demolition aborted: fixture_boxscore_input_version did not re-issue';
    END IF;
END $$;

INSERT INTO public.schema_migrations(version) VALUES ('230_seeder_era_provider_map_demolition')
    ON CONFLICT DO NOTHING;

COMMIT;

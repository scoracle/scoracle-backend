-- 207_surfaces_include_persons.sql
--
-- WHAT: `refresh_entity_name_surfaces()` now folds `persons` (full_name + search_aliases,
-- entity_type 'person') into `entity_name_surfaces` (PLAN-one-rail.md 1.8).
--
-- WHY: the living database compounds only if a verified person becomes RESOLVABLE — tomorrow's
-- article about the same coach must exact-match on nrm() instead of re-nominating (or worse,
-- fuzzy-matching a player, the T9 failure class mig 198 measured). Persons enter the same
-- surfaces table under the same rules: exact match on (norm, sport) is the only automatic
-- path; trigram ranks for review.
--
-- NULL-sport persons are skipped: entity_name_surfaces.sport is NOT NULL (resolution is
-- sport-scoped, §1a resolved.links carry sport). A person gains surfaces when its sport is
-- known — until then it simply stays unresolvable, which is the honest state.
--
-- Function body derived from the CURRENT prod definition (mig 199's, with the ANALYZE — the
-- migration runner verifies prod matches before this applies; see the template rule). The
-- migration ends by RUNNING the refresh: person surfaces = 0 rows today (persons is empty),
-- which proves the wiring without changing behavior — and a gate RAISES if that expectation
-- breaks.
--
-- Deploy order: additive, function body only. persons is empty, so the rebuilt table is
-- byte-identical to before.

BEGIN;

CREATE OR REPLACE FUNCTION public.refresh_entity_name_surfaces() RETURNS bigint
    LANGUAGE plpgsql AS $$
DECLARE
    n bigint;
BEGIN
    DELETE FROM public.entity_name_surfaces;

    WITH nxt AS (
        SELECT 'team'::text AS entity_type, t.id AS entity_id, t.sport,
               public.nrm(t.name) AS norm, 'name'::text AS surface_kind
          FROM public.teams t WHERE public.nrm(t.name) <> ''
        UNION
        SELECT 'team', t.id, t.sport, public.nrm(a), 'alias'
          FROM public.teams t, unnest(t.search_aliases) a WHERE public.nrm(a) <> ''
        UNION
        SELECT 'player', p.id, p.sport, public.nrm(p.name), 'name'
          FROM public.players p WHERE public.nrm(p.name) <> ''
        UNION
        SELECT 'player', p.id, p.sport, public.nrm(a), 'alias'
          FROM public.players p, unnest(p.search_aliases) a WHERE public.nrm(a) <> ''
        UNION
        -- Persons (mig 203): the Investigator's verified people. NULL-sport persons carry no
        -- surfaces — resolution is sport-scoped, and the FK on sport is NOT NULL here.
        SELECT 'person', pe.id, pe.sport, public.nrm(pe.full_name), 'name'
          FROM public.persons pe
         WHERE pe.sport IS NOT NULL AND public.nrm(pe.full_name) <> ''
        UNION
        SELECT 'person', pe.id, pe.sport, public.nrm(a), 'alias'
          FROM public.persons pe, unnest(pe.search_aliases) a
         WHERE pe.sport IS NOT NULL AND public.nrm(a) <> ''
    )
    -- A canonical name and an alias can normalize to the same string (AC Milan carries alias
    -- 'Milan'; 'Atletico de Madrid' folds onto its own accented name). The PK is per surface, so
    -- collapse to one row per (entity, norm) and let 'name' win -- it is the stronger provenance.
    INSERT INTO public.entity_name_surfaces (entity_type, entity_id, sport, norm, surface_kind)
    SELECT DISTINCT ON (entity_type, entity_id, sport, norm)
           entity_type, entity_id, sport, norm, surface_kind
      FROM nxt
     ORDER BY entity_type, entity_id, sport, norm,
              (surface_kind = 'name') DESC;

    GET DIAGNOSTICS n = ROW_COUNT;

    -- Without this the planner sees an empty table and seq-scans every resolver lookup until
    -- autovacuum notices. See mig 199's header: 38 ms vs 2 ms on the same query.
    ANALYZE public.entity_name_surfaces;

    RETURN n;
END;
$$;

COMMENT ON FUNCTION public.refresh_entity_name_surfaces() IS
    'Full rebuild of entity_name_surfaces from teams/players/persons (persons since mig 207; '
    'NULL-sport persons are skipped — resolution is sport-scoped). Idempotent; ANALYZEs on the '
    'way out (mig 199). Call after any roster, alias, or person change.';

COMMENT ON TABLE public.entity_name_surfaces IS
    'Normalized name/alias surfaces for teams, players, and persons (mig 207). Rebuilt by '
    'refresh_entity_name_surfaces(), which ANALYZEs on the way out (mig 199 -- the rebuild '
    'otherwise leaves empty-table statistics and every lookup seq-scans). Exact match on '
    '(norm, sport) is the ONLY automatic resolution path; the trigram index is for ranking and '
    'review, never an unattended write. See mig 198 for the measured reason (spain -> team 394 '
    'clears any margin gate and is wrong).';

-- Run it, and gate the expectation: persons is empty today, so person surfaces MUST be 0 and
-- the rebuild must still produce a non-empty table (teams/players as before). RAISES on
-- mismatch, rolling back the whole migration (template rule 2).
DO $$
DECLARE
    total   bigint;
    persons bigint;
BEGIN
    SELECT public.refresh_entity_name_surfaces() INTO total;
    SELECT count(*) INTO persons
      FROM public.entity_name_surfaces WHERE entity_type = 'person';
    IF persons <> 0 THEN
        RAISE EXCEPTION 'mig 207 gate: expected 0 person surfaces (persons is empty), got %',
            persons;
    END IF;
    IF total < 10000 THEN
        RAISE EXCEPTION 'mig 207 gate: refresh returned only % surfaces (expected ~16,700 -- '
            'did the rebuild lose teams/players?)', total;
    END IF;
    RAISE NOTICE 'mig 207: refresh rebuilt % surfaces, 0 person rows as expected', total;
END;
$$;

INSERT INTO public.schema_migrations(version) VALUES ('207_surfaces_include_persons')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

-- 199_refresh_surfaces_analyze.sql
--
-- WHAT: `refresh_entity_name_surfaces()` now ANALYZEs the table it just rewrote.
--
-- WHY: the function DELETEs and re-INSERTs all ~16,690 rows, which leaves the planner with
-- statistics describing an empty table until autovacuum catches up. Measured on Archbox
-- immediately after mig 198 applied, the identical trigram lookup planned two different ways:
--
--   stale stats  -> Seq Scan, 16,684 rows removed by filter   38.2 ms
--   after ANALYZE-> Bitmap Index Scan on the GIN trgm index     2.3 ms
--
-- Same query, same data, 17x. Left alone this is the worst kind of latency bug: intermittent,
-- self-healing on autovacuum's schedule, and it would present as "B1 is sometimes slow" long after
-- anyone remembers a refresh ran. The rebuild is ~19k rows, so ANALYZE is cheap enough to be
-- unconditional.
--
-- ## Correcting a claim in mig 198's header
--
-- 198 said the GIN trigram index costs "a fraction" of what A1 declined. That is true in absolute
-- terms and wrong as a ratio. Measured:
--
--   entity_name_surfaces table          1,144 kB
--   idx_entity_name_surfaces_trgm       5,096 kB   (4.5x the table)
--   idx_entity_name_surfaces_lookup       880 kB
--
-- A GIN trigram index is simply large relative to short text. The reason to pay it here is still
-- sound -- 5 MB against 16,690 entity names, versus indexing the title column of a 150,566-row
-- article corpus -- but the honest framing is "5 MB absolute", not "a fraction of the table".
-- A1's refusal to index `news_articles.title` is reinforced by this ratio, not weakened by it.
--
-- Deploy order: additive, function body only. Nothing reads it yet.

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
    -- autovacuum notices. See the header: 38 ms vs 2 ms on the same query.
    ANALYZE public.entity_name_surfaces;

    RETURN n;
END;
$$;

COMMENT ON TABLE public.entity_name_surfaces IS
    'Normalized name/alias surfaces for teams and players. Rebuilt by refresh_entity_name_surfaces(), '
    'which ANALYZEs on the way out (mig 199 -- the rebuild otherwise leaves empty-table statistics '
    'and every lookup seq-scans). Exact match on (norm, sport) is the ONLY automatic resolution '
    'path; the trigram index is for ranking and review, never an unattended write. See mig 198 for '
    'the measured reason (spain -> team 394 clears any margin gate and is wrong).';

INSERT INTO public.schema_migrations(version) VALUES ('199_refresh_surfaces_analyze')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

-- 198_entity_name_resolution.sql
--
-- WHAT: `public.nrm(text)` (the one normalizer), `entity_name_surfaces` (every name and alias a
-- team or player answers to, normalized), its exact-lookup and GIN-trigram indexes, and a hand-
-- verified alias seed.
--
-- WHY: the Editor emits `relevant_entities` as NAME STRINGS. Nothing maps them back to our
-- entities, so the richest thing it produces is discarded. B1 is that map. Measured on the 07-27
-- incident cohort (6,377 articles, 26,018 emitted names, only 8,652 DISTINCT): exact resolution
-- against this table recovers 15,771 links -- 9,818 that already exist at vetted = FALSE and 5,910
-- the ingest query never proposed, 4,403 of them players.
--
-- ## ONE normalizer, in SQL, on purpose
--
-- `nrm` is IMMUTABLE and lives in the database because the index and the query MUST agree. A Rust
-- copy would be a second normalizer that drifts from this one, and the cost of two functions that
-- claim to be the same query is already written down here as trap T-A5: the corpus cap sat on
-- `load_vetted_corpus` for weeks while production called a sibling that had never received it.
-- There is also no `unaccent` in the Rust dependency tree, so a copy would have to reimplement it.
--
-- Folding punctuation is not cosmetic. `paris saint-germain` (46 occurrences in the cohort) misses
-- `Paris Saint Germain` on a bare lower+unaccent and matches it exactly here. That converts a fuzzy
-- match into a deterministic one, which is always the better trade.
--
-- ## The GIN trigram index A1 deferred -- on a DIFFERENT column, and this is the distinction
--
-- A1 installed pg_trgm and deliberately created NO index, because a GIN trgm index on
-- `news_articles.title` costs ~the size of the column and the thresholds were not settled. That
-- reasoning still stands and is NOT reversed here. This index is on 18,753 short entity names --
-- a fraction of the cost -- and it is the case the plan meant by "pg_trgm's real job: resolving a
-- short list of model-extracted names, not scanning article bodies." Measured: a per-name
-- sequential scan is 20-35ms, and a 300-name batch join times out at two minutes. The live rail
-- cannot pay that; the index is what makes B1 affordable.
--
-- ## Trigram is a SUGGESTION channel, never an automatic link. Measured.
--
-- The 120 most frequent unresolved names were scored against every surface, sport-scoped, and the
-- top-2 margin inspected by hand. The margin gate does NOT separate right from wrong, because the
-- dominant error is not a tie -- it is a CONFIDENT SINGLE match to an entity that is simply not the
-- one named:
--
--   spain             -> team 394 'spa'          sim 0.429, margin 0.429, no runner-up   WRONG
--   pep guardiola     -> player 'sergi guardiola' sim 0.500, margin 0.500                 WRONG
--   sheffield wednesday -> team 21 'sheffield utd' sim 0.417, margin 0.417                WRONG (a rival)
--   charlton athletic -> team 13258 'athletic club' sim 0.455, margin 0.455               WRONG (Bilbao)
--   lee child         -> team 71 'lee'            sim 0.400, margin 0.400                 WRONG (a novelist)
--
-- Every one of those clears a margin gate more comfortably than `vinicius jr -> Vinicius Junior`
-- (margin 0.082) clears it, and that one is CORRECT. National teams, coaches, and people who are
-- not in the DB at all fuzzy-match to real players with no runner-up to hold them back. So this
-- migration creates the index for RANKING and REVIEW; the automatic write path is exact match only,
-- and unresolved names belong to B3's capture table.
--
-- The one thing a margin gate does catch is the true tie -- `inter milan` scores 0.500 against BOTH
-- Inter (2930) and AC Milan (113). A top-1 pick would link Inter Milan stories to their rivals on a
-- coin flip. That is why ambiguity is REFUSED rather than broken: measured, exact-match ties are 59
-- of 15,654 resolutions (0.38%), all same-sport player namesakes, zero team/player collisions.
--
-- ## The alias seed
--
-- Derived from the cohort's unresolved names, then HAND-VERIFIED one by one against the entity row.
-- These are not disambiguation problems; they are missing aliases wearing a disambiguation costume.
-- `Inter Milan` and `Vinicius Jr` are unambiguous to any reader -- they are ambiguous only to
-- trigram, and an alias fixes them permanently at zero inference cost.
--
-- DELIBERATELY EXCLUDED, with reasons, so nobody re-adds them:
--   madrid              -- Real (3468) or Atletico (7980). Genuinely ambiguous. Refuse.
--   spain/germany/portugal/france -- national teams; we do not carry them.
--   fortuna sittard     -- NOT team 1092 'Fortuna'. Different club.
--   sheffield wednesday -- NOT team 21 'Sheffield Utd'. Different club, same city.
--   charlton athletic   -- NOT team 13258 'Athletic Club'. Different country.
--   gironde             -- a French department, not team 231 'Girona'.
--   every coach/manager -- Xabi Alonso, Pep Guardiola, Jose Mourinho, Sean McVay, John Harbaugh,
--                          Andy Reid, Ruben Amorim, David Moyes, Kyle Shanahan. We store PLAYERS.
--                          Each of them fuzzy-matches a real player. This is what the future
--                          discovery seat is for -- see PLAN-ingest-simplification.md.
--
-- Deploy order: additive and INERT. Nothing reads `entity_name_surfaces` until `bin/remap` or B1
-- ships. The alias rows change `teams.search_aliases` / `players.search_aliases`, which Go's
-- matcher DOES read -- that is intended (it widens candidate generation, which B2 wants) and is
-- additive to a column that already carries short codes.

BEGIN;

-- ---------------------------------------------------------------------------
-- The one normalizer.
-- ---------------------------------------------------------------------------

-- IMMUTABLE is required for the functional index below. `unaccent` is only STABLE by default
-- (it resolves a dictionary by name), so it is pinned to the schema-qualified two-argument form,
-- which IS immutable. Without this the index cannot be created.
CREATE OR REPLACE FUNCTION public.nrm(t text) RETURNS text
    LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT AS $$
    SELECT btrim(
             regexp_replace(
               regexp_replace(
                 lower(public.unaccent('public.unaccent'::regdictionary, t)),
                 '[^a-z0-9]+', ' ', 'g'),
               '\s+', ' ', 'g'))
$$;

COMMENT ON FUNCTION public.nrm(text) IS
    'The single name normalizer: lower, unaccent, fold punctuation to spaces, collapse whitespace. '
    'Used by entity_name_surfaces, its indexes, and every resolver query. Do NOT reimplement in '
    'Rust or Go -- two normalizers that disagree is the failure mode mig 198 exists to avoid.';

-- ---------------------------------------------------------------------------
-- Every surface an entity answers to.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.entity_name_surfaces (
    entity_type text    NOT NULL CHECK (entity_type IN ('team','player')),
    entity_id   integer NOT NULL,
    sport       text    NOT NULL REFERENCES public.sports(id),
    norm        text    NOT NULL,
    -- 'name' = the canonical row name; 'alias' = an entry in search_aliases. Kept so a resolver can
    -- prefer a canonical hit over an alias hit without re-deriving where the surface came from.
    surface_kind text   NOT NULL CHECK (surface_kind IN ('name','alias')),
    PRIMARY KEY (entity_type, entity_id, sport, norm)
);

COMMENT ON TABLE public.entity_name_surfaces IS
    'Normalized name/alias surfaces for teams and players. Rebuilt by refresh_entity_name_surfaces(). '
    'Exact match on (norm, sport) is the ONLY automatic resolution path -- the trigram index is for '
    'ranking and review, never for an unattended write. See the migration header for the measured '
    'reason (spain -> team 394 clears any margin gate and is wrong).';

-- Exact lookup: the automatic path.
CREATE INDEX IF NOT EXISTS idx_entity_name_surfaces_lookup
    ON public.entity_name_surfaces (norm, sport);

-- Fuzzy ranking: the review path. 18,753 short rows, not the article corpus.
CREATE INDEX IF NOT EXISTS idx_entity_name_surfaces_trgm
    ON public.entity_name_surfaces USING gin (norm gin_trgm_ops);

-- ---------------------------------------------------------------------------
-- Rebuild. Idempotent, and a full swap rather than an incremental merge -- entity renames and
-- alias removals must DROP surfaces, which an upsert-only refresh silently would not.
-- ---------------------------------------------------------------------------

-- A CTE rather than a TEMP TABLE, deliberately: `CREATE TEMP TABLE ... ON COMMIT DROP` only drops
-- at COMMIT, so a second call inside the SAME transaction fails with "relation already exists".
-- The migration's own dry run hit exactly that, and any caller that refreshes twice in one
-- transaction (the backfill does, once per stage) would have hit it in production.
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
    RETURN n;
END;
$$;

COMMENT ON FUNCTION public.refresh_entity_name_surfaces() IS
    'Full rebuild of entity_name_surfaces from teams/players. Idempotent. Call after any roster or '
    'alias change; cheap enough (~19k rows) to run on a schedule.';

-- ---------------------------------------------------------------------------
-- Alias seed. Hand-verified; see the header for what was refused and why.
-- ---------------------------------------------------------------------------

-- NOTE ON SHAPE: the seed is aggregated per entity before the UPDATE. `UPDATE ... FROM` joins each
-- target row ONCE, so when two seed rows name the same entity Postgres applies one and silently
-- discards the other -- no error, no warning. The dry run caught it: Tottenham took 'Tottenham' and
-- lost 'Spurs', and Vinicius Junior took 'Vinicius Jr' and lost 'Vini Jr'. A per-entity array_agg
-- is what makes a multi-alias seed actually land.

-- Teams. Each pair was checked against the entity row (name, country, short_code) before landing.
WITH seed(entity_id, sport, alias) AS (VALUES
    (2930, 'FOOTBALL', 'Inter Milan'),        -- teams.name = 'Inter' (Italy). The English name.
    (6,    'FOOTBALL', 'Tottenham'),          -- 'Tottenham Hotspur'
    (6,    'FOOTBALL', 'Spurs'),              -- the universal short form
    (52,   'FOOTBALL', 'Bournemouth'),        -- 'AFC Bournemouth'
    (7980, 'FOOTBALL', 'Atletico Madrid'),    -- 'Atletico de Madrid'; nrm folds the accent
    (37,   'FOOTBALL', 'AS Roma'),            -- 'Roma' (Italy)
    (277,  'FOOTBALL', 'Leipzig'),            -- 'RB Leipzig'
    (591,  'FOOTBALL', 'Paris Saint-Germain') -- 'Paris Saint Germain'; distinct from 4508 'Paris'
), grouped AS (
    SELECT entity_id, sport, array_agg(alias) AS aliases FROM seed GROUP BY 1,2
)
UPDATE public.teams t
   SET search_aliases = (
        SELECT array_agg(DISTINCT x)
          FROM unnest(t.search_aliases || g.aliases) x
       ),
       updated_at = now()
  FROM grouped g
 WHERE t.id = g.entity_id AND t.sport = g.sport;

-- Players.
WITH seed(entity_id, sport, alias) AS (VALUES
    (600687,   'FOOTBALL', 'Vinicius Jr'),    -- 'Vinicius Junior'. Distinct from Vinicius Tobias,
    (600687,   'FOOTBALL', 'Vini Jr'),        --   who shares Real Madrid, so roster context ties.
    (9967153,  'FOOTBALL', 'Lee Kang-in'),    -- 'Kang-in Lee' -- Western name order
    (37632336, 'FOOTBALL', 'Eli Junior Kroupi'), -- 'Eli Kroupi'
    (115,      'NBA',      'Steph Curry'),    -- 'Stephen Curry'. Distinct from Seth Curry.
    (666541,   'NBA',      'Lu Dort'),        -- 'Luguentz Dort'
    (65,       'NFL',      'Gardner Minshew') -- 'Gardner Minshew II'
), grouped AS (
    SELECT entity_id, sport, array_agg(alias) AS aliases FROM seed GROUP BY 1,2
)
UPDATE public.players p
   SET search_aliases = (
        SELECT array_agg(DISTINCT x)
          FROM unnest(p.search_aliases || g.aliases) x
       ),
       updated_at = now()
  FROM grouped g
 WHERE p.id = g.entity_id AND p.sport = g.sport;

-- Build the table now that the seed is in.
SELECT public.refresh_entity_name_surfaces();

INSERT INTO public.schema_migrations(version) VALUES ('198_entity_name_resolution')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

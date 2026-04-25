-- Filter NBA + NFL autofill_entities to drop player rows that have no stats
-- in any seeded season. NBA additionally preserves the current-season draft
-- class (rookies who haven't logged a stat yet); NFL has no draft_year in
-- players.meta so the strict filter applies.
--
-- This script preserves the LOCAL matview column list (no team_logo_url —
-- migration 010 has not been applied here). Each per-sport block is wrapped
-- in BEGIN/COMMIT so the swap is atomic from a reader's perspective.
--
-- Run: psql "$DATABASE_PRIVATE_URL" -f scripts/ops/purge_statless_autofill.sql

-- NBA --------------------------------------------------------------------
BEGIN;

DROP MATERIALIZED VIEW IF EXISTS nba.autofill_entities;
CREATE MATERIALIZED VIEW nba.autofill_entities AS
    SELECT
        p.id,
        'player'::text AS type,
        p.name,
        p.first_name,
        p.last_name,
        p.position,
        p.detailed_position,
        p.nationality,
        p.date_of_birth::text AS date_of_birth,
        p.height,
        p.weight,
        p.photo_url,
        p.team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        t.name AS team_name,
        jsonb_build_array(
            LOWER(p.first_name),
            LOWER(p.last_name),
            LOWER(REPLACE(p.name, ' ', '')),
            LOWER(COALESCE(t.short_code, '')),
            LOWER(COALESCE(t.name, '')),
            unaccent(LOWER(p.first_name)),
            unaccent(LOWER(p.last_name)),
            unaccent(LOWER(REPLACE(p.name, ' ', ''))),
            unaccent(LOWER(COALESCE(t.name, '')))
        ) AS search_tokens,
        -- Pass the full player meta blob through. Frontend curates display.
        COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NBA'
      AND (
          EXISTS (
              SELECT 1 FROM public.player_stats ps
              WHERE ps.player_id = p.id AND ps.sport = p.sport
          )
          -- Rookie exemption: keep current-season draft class even with no
          -- stats yet, so we don't lose rookies who haven't played.
          OR (p.meta->>'draft_year')::int = (
              SELECT current_season FROM public.sports WHERE id = 'NBA'
          )
      )
UNION ALL
    SELECT
        t.id,
        'team'::text AS type,
        t.name,
        NULL::text AS first_name,
        NULL::text AS last_name,
        t.conference AS position,
        t.division AS detailed_position,
        t.country AS nationality,
        NULL::text AS date_of_birth,
        NULL::text AS height,
        NULL::text AS weight,
        t.logo_url AS photo_url,
        NULL::int AS team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        jsonb_build_array(
            LOWER(REPLACE(t.name, ' ', '')),
            LOWER(t.short_code),
            LOWER(t.city),
            LOWER(t.country),
            unaccent(LOWER(REPLACE(t.name, ' ', ''))),
            unaccent(LOWER(t.city))
        ) AS search_tokens,
        jsonb_build_object(
            'display_name', t.name,
            'abbreviation', t.short_code,
            'city', t.city,
            'country', t.country,
            'conference', t.conference,
            'division', t.division,
            'founded', t.founded,
            'venue_name', t.venue_name,
            'venue_capacity', t.venue_capacity
        ) AS meta
    FROM public.teams t
    WHERE t.sport = 'NBA'
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_nba_autofill_pk
    ON nba.autofill_entities (id, type);

COMMIT;

-- NFL --------------------------------------------------------------------
BEGIN;

DROP MATERIALIZED VIEW IF EXISTS nfl.autofill_entities;
CREATE MATERIALIZED VIEW nfl.autofill_entities AS
    SELECT
        p.id,
        'player'::text AS type,
        p.name,
        p.first_name,
        p.last_name,
        p.position,
        p.detailed_position,
        p.nationality,
        p.date_of_birth::text AS date_of_birth,
        p.height,
        p.weight,
        p.photo_url,
        p.team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        t.name AS team_name,
        jsonb_build_array(
            LOWER(p.first_name),
            LOWER(p.last_name),
            LOWER(REPLACE(p.name, ' ', '')),
            LOWER(COALESCE(t.short_code, '')),
            LOWER(COALESCE(t.name, '')),
            unaccent(LOWER(p.first_name)),
            unaccent(LOWER(p.last_name)),
            unaccent(LOWER(REPLACE(p.name, ' ', ''))),
            unaccent(LOWER(COALESCE(t.name, '')))
        ) AS search_tokens,
        -- Pass the full player meta blob through. Frontend curates display.
        COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NFL'
      AND (
          EXISTS (
              SELECT 1 FROM public.player_stats ps
              WHERE ps.player_id = p.id AND ps.sport = p.sport
          )
          -- Rookie exemption: BDL labels first-year players "Rookie" in
          -- meta.experience, so unplayed rookies stay in autofill.
          OR p.meta->>'experience' ILIKE 'rookie%'
      )
UNION ALL
    SELECT
        t.id,
        'team'::text AS type,
        t.name,
        NULL::text AS first_name,
        NULL::text AS last_name,
        t.conference AS position,
        t.division AS detailed_position,
        t.country AS nationality,
        NULL::text AS date_of_birth,
        NULL::text AS height,
        NULL::text AS weight,
        t.logo_url AS photo_url,
        NULL::int AS team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        jsonb_build_array(
            LOWER(REPLACE(t.name, ' ', '')),
            LOWER(t.short_code),
            LOWER(t.city),
            LOWER(t.country),
            unaccent(LOWER(REPLACE(t.name, ' ', ''))),
            unaccent(LOWER(t.city))
        ) AS search_tokens,
        jsonb_build_object(
            'display_name', t.name,
            'abbreviation', t.short_code,
            'city', t.city,
            'country', t.country,
            'conference', t.conference,
            'division', t.division,
            'founded', t.founded,
            'venue_name', t.venue_name,
            'venue_capacity', t.venue_capacity
        ) AS meta
    FROM public.teams t
    WHERE t.sport = 'NFL'
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_nfl_autofill_pk
    ON nfl.autofill_entities (id, type);

COMMIT;

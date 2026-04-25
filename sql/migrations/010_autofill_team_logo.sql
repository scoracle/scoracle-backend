-- Add team_logo_url to autofill_entities materialized views so the frontend
-- can fall back to the parent team's crest when a player has no headshot
-- (NBA/NFL don't ship player photos yet). Adding a column to a materialized
-- view requires DROP + CREATE; CONCURRENTLY refresh can't change the schema.
--
-- Canonical view bodies live in sql/{nba,nfl,football}.sql — this migration
-- mirrors them with the team_logo_url column added.

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
        t.logo_url AS team_logo_url,
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
        jsonb_build_object(
            'display_name', p.name,
            'jersey_number', p.meta->>'jersey_number',
            'draft_year', (p.meta->>'draft_year')::int,
            'draft_pick', (p.meta->>'draft_pick')::int,
            'years_pro', (p.meta->>'years_pro')::int,
            'college', p.meta->>'college'
        ) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NBA'
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
        NULL::text AS team_logo_url,
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
        t.logo_url AS team_logo_url,
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
        jsonb_build_object(
            'display_name', p.name,
            'jersey_number', p.meta->>'jersey_number',
            'draft_year', (p.meta->>'draft_year')::int,
            'draft_pick', (p.meta->>'draft_pick')::int,
            'years_pro', (p.meta->>'years_pro')::int,
            'college', p.meta->>'college'
        ) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NFL'
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
        NULL::text AS team_logo_url,
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

DROP MATERIALIZED VIEW IF EXISTS football.autofill_entities;
CREATE MATERIALIZED VIEW football.autofill_entities AS
    SELECT * FROM (
        SELECT DISTINCT ON (p.id)
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
            ps.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            t.name AS team_name,
            t.logo_url AS team_logo_url,
            jsonb_build_array(
                LOWER(p.first_name),
                LOWER(p.last_name),
                LOWER(REPLACE(p.name, ' ', '')),
                LOWER(COALESCE(t.short_code, '')),
                LOWER(COALESCE(t.name, '')),
                LOWER(COALESCE(l.name, '')),
                unaccent(LOWER(p.first_name)),
                unaccent(LOWER(p.last_name)),
                unaccent(LOWER(REPLACE(p.name, ' ', ''))),
                unaccent(LOWER(COALESCE(t.name, '')))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', p.name,
                'jersey_number', p.meta->>'jersey_number',
                'foot', p.meta->>'foot',
                'market_value', (p.meta->>'market_value')::bigint,
                'contract_until', p.meta->>'contract_until'
            ) AS meta
        FROM public.players p
        LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
        LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
        LEFT JOIN public.leagues l ON l.id = ps.league_id
        WHERE p.sport = 'FOOTBALL'
        ORDER BY p.id, ps.season DESC NULLS LAST
    ) football_players
UNION ALL
    SELECT * FROM (
        SELECT DISTINCT ON (t.id)
            t.id,
            'team'::text AS type,
            t.name,
            NULL::text AS first_name,
            NULL::text AS last_name,
            NULL::text AS position,
            NULL::text AS detailed_position,
            t.country AS nationality,
            NULL::text AS date_of_birth,
            NULL::text AS height,
            NULL::text AS weight,
            t.logo_url AS photo_url,
            NULL::int AS team_id,
            ts.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            NULL::text AS team_name,
            NULL::text AS team_logo_url,
            jsonb_build_array(
                LOWER(REPLACE(t.name, ' ', '')),
                LOWER(t.short_code),
                LOWER(t.city),
                LOWER(t.country),
                LOWER(COALESCE(l.name, '')),
                unaccent(LOWER(REPLACE(t.name, ' ', ''))),
                unaccent(LOWER(t.city))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', t.name,
                'abbreviation', t.short_code,
                'city', t.city,
                'country', t.country,
                'founded', t.founded,
                'venue_name', t.venue_name,
                'venue_capacity', t.venue_capacity
            ) AS meta
        FROM public.teams t
        LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
        LEFT JOIN public.leagues l ON l.id = ts.league_id
        WHERE t.sport = 'FOOTBALL'
        ORDER BY t.id, ts.season DESC NULLS LAST
    ) football_teams
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_football_autofill_pk
    ON football.autofill_entities (id, type);

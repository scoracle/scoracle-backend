-- ============================================================================
-- 044_autofill_position.sql
-- Re-expose player `position` on the {sport}.autofill_entities materialized
-- views, so the /meta endpoint (→ frontend EntityMeta + bundled meta JSON)
-- shows a player's position again.
--
-- Background: migration 013 moved position OUT of public.players and INTO
-- player_stats.position (per-season, owned by the stats domain). That same
-- migration recreated these MVs from `players` — which no longer has a position
-- column — so the MVs silently dropped `position`. The frontend reads top-level
-- `item.position` (scripts/fetch-autofill.mjs + EntityMeta), which has been null
-- ever since. This restores it from player_stats (latest season per player).
--
-- STRICTLY ADDITIVE: adds one top-level `position` column to each MV (players
-- get the latest-season player_stats.position; teams get NULL — they already
-- carry conference/division in `meta`). The NFL "Unknown" sentinel is NULLIF'd
-- so the meta reads clean. CREATE ... AS populates immediately; we recreate the
-- unique (id,type) index each MV needs for REFRESH CONCURRENTLY in
-- finalize_fixture. No Go change — /meta serves these via row_to_json.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/044_autofill_position.sql
-- ============================================================================

BEGIN;

-- ── NBA ─────────────────────────────────────────────────────────────────────
DROP MATERIALIZED VIEW IF EXISTS nba.autofill_entities;
CREATE MATERIALIZED VIEW nba.autofill_entities AS
 SELECT p.id,
    'player'::text AS type,
    p.name,
    p.first_name,
    p.last_name,
    p.nationality,
    p.date_of_birth::text AS date_of_birth,
    p.height,
    p.weight,
    p.photo_url,
    p.team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    t.name AS team_name,
    t.logo_url AS team_logo_url,
    NULLIF(pos.position, 'Unknown'::text) AS position,
    jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), unaccent(lower(p.first_name)), unaccent(lower(p.last_name)), unaccent(lower(replace(p.name, ' '::text, ''::text))), unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
    COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
   FROM players p
     LEFT JOIN teams t ON t.id = p.team_id AND t.sport = p.sport
     LEFT JOIN LATERAL ( SELECT ps.position
           FROM player_stats ps
          WHERE ps.player_id = p.id AND ps.sport = p.sport
          ORDER BY ps.season DESC NULLS LAST
          LIMIT 1) pos ON true
  WHERE p.sport = 'NBA'::text AND ((EXISTS ( SELECT 1
           FROM player_stats ps
          WHERE ps.player_id = p.id AND ps.sport = p.sport)) OR ((p.meta ->> 'draft_year'::text)::integer) = (( SELECT sports.current_season
           FROM sports
          WHERE sports.id = 'NBA'::text)))
UNION ALL
 SELECT t.id,
    'team'::text AS type,
    t.name,
    NULL::text AS first_name,
    NULL::text AS last_name,
    t.country AS nationality,
    NULL::text AS date_of_birth,
    NULL::text AS height,
    NULL::text AS weight,
    t.logo_url AS photo_url,
    NULL::integer AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    NULL::text AS team_name,
    NULL::text AS team_logo_url,
    NULL::text AS position,
    jsonb_build_array(lower(replace(t.name, ' '::text, ''::text)), lower(t.short_code), lower(t.city), lower(t.country), unaccent(lower(replace(t.name, ' '::text, ''::text))), unaccent(lower(t.city))) AS search_tokens,
    jsonb_build_object('display_name', t.name, 'abbreviation', t.short_code, 'city', t.city, 'country', t.country, 'conference', t.conference, 'division', t.division, 'founded', t.founded, 'venue_name', t.venue_name, 'venue_capacity', t.venue_capacity) AS meta
   FROM teams t
  WHERE t.sport = 'NBA'::text;
CREATE UNIQUE INDEX idx_nba_autofill_pk ON nba.autofill_entities USING btree (id, type);

-- ── NFL ─────────────────────────────────────────────────────────────────────
DROP MATERIALIZED VIEW IF EXISTS nfl.autofill_entities;
CREATE MATERIALIZED VIEW nfl.autofill_entities AS
 SELECT p.id,
    'player'::text AS type,
    p.name,
    p.first_name,
    p.last_name,
    p.nationality,
    p.date_of_birth::text AS date_of_birth,
    p.height,
    p.weight,
    p.photo_url,
    p.team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    t.name AS team_name,
    t.logo_url AS team_logo_url,
    NULLIF(pos.position, 'Unknown'::text) AS position,
    jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), unaccent(lower(p.first_name)), unaccent(lower(p.last_name)), unaccent(lower(replace(p.name, ' '::text, ''::text))), unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
    COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
   FROM players p
     LEFT JOIN teams t ON t.id = p.team_id AND t.sport = p.sport
     LEFT JOIN LATERAL ( SELECT ps.position
           FROM player_stats ps
          WHERE ps.player_id = p.id AND ps.sport = p.sport
          ORDER BY ps.season DESC NULLS LAST
          LIMIT 1) pos ON true
  WHERE p.sport = 'NFL'::text AND ((EXISTS ( SELECT 1
           FROM player_stats ps
          WHERE ps.player_id = p.id AND ps.sport = p.sport)) OR (p.meta ->> 'experience'::text) ~~* 'rookie%'::text)
UNION ALL
 SELECT t.id,
    'team'::text AS type,
    t.name,
    NULL::text AS first_name,
    NULL::text AS last_name,
    t.country AS nationality,
    NULL::text AS date_of_birth,
    NULL::text AS height,
    NULL::text AS weight,
    t.logo_url AS photo_url,
    NULL::integer AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    NULL::text AS team_name,
    NULL::text AS team_logo_url,
    NULL::text AS position,
    jsonb_build_array(lower(replace(t.name, ' '::text, ''::text)), lower(t.short_code), lower(t.city), lower(t.country), unaccent(lower(replace(t.name, ' '::text, ''::text))), unaccent(lower(t.city))) AS search_tokens,
    jsonb_build_object('display_name', t.name, 'abbreviation', t.short_code, 'city', t.city, 'country', t.country, 'conference', t.conference, 'division', t.division, 'founded', t.founded, 'venue_name', t.venue_name, 'venue_capacity', t.venue_capacity) AS meta
   FROM teams t
  WHERE t.sport = 'NFL'::text;
CREATE UNIQUE INDEX idx_nfl_autofill_pk ON nfl.autofill_entities USING btree (id, type);

-- ── FOOTBALL (nested DISTINCT ON — latest-season ps row carries position) ─────
DROP MATERIALIZED VIEW IF EXISTS football.autofill_entities;
CREATE MATERIALIZED VIEW football.autofill_entities AS
 SELECT football_players.id,
    football_players.type,
    football_players.name,
    football_players.first_name,
    football_players.last_name,
    football_players.nationality,
    football_players.date_of_birth,
    football_players.height,
    football_players.weight,
    football_players.photo_url,
    football_players.team_id,
    football_players.league_id,
    football_players.league_name,
    football_players.team_abbr,
    football_players.team_name,
    football_players.team_logo_url,
    football_players.position,
    football_players.search_tokens,
    football_players.meta
   FROM ( SELECT DISTINCT ON (p.id) p.id,
            'player'::text AS type,
            p.name,
            p.first_name,
            p.last_name,
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
            NULLIF(ps.position, 'Unknown'::text) AS position,
            jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), lower(COALESCE(l.name, ''::text)), unaccent(lower(p.first_name)), unaccent(lower(p.last_name)), unaccent(lower(replace(p.name, ' '::text, ''::text))), unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
            jsonb_build_object('display_name', p.name, 'jersey_number', p.meta ->> 'jersey_number'::text, 'foot', p.meta ->> 'foot'::text, 'market_value', (p.meta ->> 'market_value'::text)::bigint, 'contract_until', p.meta ->> 'contract_until'::text) AS meta
           FROM players p
             LEFT JOIN teams t ON t.id = p.team_id AND t.sport = p.sport
             LEFT JOIN player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
             LEFT JOIN leagues l ON l.id = ps.league_id
          WHERE p.sport = 'FOOTBALL'::text
          ORDER BY p.id, ps.season DESC NULLS LAST) football_players
UNION ALL
 SELECT football_teams.id,
    football_teams.type,
    football_teams.name,
    football_teams.first_name,
    football_teams.last_name,
    football_teams.nationality,
    football_teams.date_of_birth,
    football_teams.height,
    football_teams.weight,
    football_teams.photo_url,
    football_teams.team_id,
    football_teams.league_id,
    football_teams.league_name,
    football_teams.team_abbr,
    football_teams.team_name,
    football_teams.team_logo_url,
    football_teams.position,
    football_teams.search_tokens,
    football_teams.meta
   FROM ( SELECT DISTINCT ON (t.id) t.id,
            'team'::text AS type,
            t.name,
            NULL::text AS first_name,
            NULL::text AS last_name,
            t.country AS nationality,
            NULL::text AS date_of_birth,
            NULL::text AS height,
            NULL::text AS weight,
            t.logo_url AS photo_url,
            NULL::integer AS team_id,
            ts.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            NULL::text AS team_name,
            NULL::text AS team_logo_url,
            NULL::text AS position,
            jsonb_build_array(lower(replace(t.name, ' '::text, ''::text)), lower(t.short_code), lower(t.city), lower(t.country), lower(COALESCE(l.name, ''::text)), unaccent(lower(replace(t.name, ' '::text, ''::text))), unaccent(lower(t.city))) AS search_tokens,
            jsonb_build_object('display_name', t.name, 'abbreviation', t.short_code, 'city', t.city, 'country', t.country, 'founded', t.founded, 'venue_name', t.venue_name, 'venue_capacity', t.venue_capacity) AS meta
           FROM teams t
             LEFT JOIN team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
             LEFT JOIN leagues l ON l.id = ts.league_id
          WHERE t.sport = 'FOOTBALL'::text
          ORDER BY t.id, ts.season DESC NULLS LAST) football_teams;
CREATE UNIQUE INDEX idx_football_autofill_pk ON football.autofill_entities USING btree (id, type);

-- ── Smoke: position now populated for players ────────────────────────────────
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN
        SELECT 'NBA' AS sport, count(*) FILTER (WHERE type='player' AND position IS NOT NULL) AS with_pos,
               count(*) FILTER (WHERE type='player') AS players FROM nba.autofill_entities
        UNION ALL
        SELECT 'NFL', count(*) FILTER (WHERE type='player' AND position IS NOT NULL),
               count(*) FILTER (WHERE type='player') FROM nfl.autofill_entities
        UNION ALL
        SELECT 'FOOTBALL', count(*) FILTER (WHERE type='player' AND position IS NOT NULL),
               count(*) FILTER (WHERE type='player') FROM football.autofill_entities
    LOOP
        RAISE NOTICE '044 %: % / % players have position', r.sport, r.with_pos, r.players;
    END LOOP;
END $$;

COMMIT;

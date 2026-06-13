-- ============================================================================
-- 076_autofill_current_club.sql
-- Meta/search showed a player's most-recently-SEEDED club instead of their
-- most-recently-PLAYED club (Pulisic→Chelsea, Bellingham→Dortmund, Olise→Crystal
-- Palace), because the {sport}.autofill_entities materialized views join teams on
-- the stale public.players.team_id. That field is clobbered by an OLD season
-- whenever the seeder backfills it (seed/shared/upsert.py:
-- `team_id = COALESCE(EXCLUDED.team_id, players.team_id)`), so it is "last seeded",
-- not "current".
--
-- Fix: resolve current club from the latest-season public.player_stats row
-- (player_stats.team_id is correctly per-season — it is what the rating
-- leaderboard already uses). One canonical source — public.player_current_team —
-- so the autofill MVs (this file), the news/vibes leaderboards (Go db.go) and the
-- transfer identity card (Go ml/transfer.go) cannot drift apart.
--
-- STRICTLY a club-resolution swap: every other column (position, league,
-- search_tokens, meta, the team branch) is byte-for-byte migration 044. Club is
-- COALESCE(player_current_team.team_id, players.team_id): the latest-PLAYED club
-- wins for anyone with a seeded stats row (~99.5%), and the old players.team_id
-- stays as a fallback only for the handful with NO stats row yet (NBA draftees /
-- NFL rookies, ~40 football players) — so nobody loses a club, the stale value
-- just stops winning.
--
-- CREATE ... AS populates immediately; the unique (id,type) index each MV needs
-- for REFRESH CONCURRENTLY in finalize_fixture is recreated. No Go-side schema
-- dependency — /meta serves these via row_to_json. Apply BEFORE restarting the Go
-- API (db.New prepares statements at boot).
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/076_autofill_current_club.sql
-- ============================================================================

BEGIN;

-- ── Canonical "current club" source ──────────────────────────────────────────
-- The single definition of a player's current club: the team_id on their
-- latest-season player_stats row that actually carries one. Index-backed by
-- player_stats_pkey (player_id, sport, season, …). Per-request read paths (Go
-- db.go vibes/news boards) inline this same rule as a LATERAL for index pushdown;
-- keep them in sync with this view.
CREATE OR REPLACE VIEW public.player_current_team AS
SELECT DISTINCT ON (ps.player_id, ps.sport)
       ps.player_id,
       ps.sport,
       ps.team_id
  FROM public.player_stats ps
 WHERE ps.team_id IS NOT NULL
 ORDER BY ps.player_id, ps.sport, ps.season DESC NULLS LAST;

COMMENT ON VIEW public.player_current_team IS
  'Canonical current club per player: latest-season player_stats.team_id. Use instead of players.team_id (which is last-seeded, not current — see migration 076).';

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
    COALESCE(pct.team_id, p.team_id) AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    t.name AS team_name,
    t.logo_url AS team_logo_url,
    NULLIF(pos.position, 'Unknown'::text) AS position,
    jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), unaccent(lower(p.first_name)), unaccent(lower(p.last_name)), unaccent(lower(replace(p.name, ' '::text, ''::text))), unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
    COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
   FROM players p
     LEFT JOIN public.player_current_team pct ON pct.player_id = p.id AND pct.sport = p.sport
     LEFT JOIN teams t ON t.id = COALESCE(pct.team_id, p.team_id) AND t.sport = p.sport
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
    COALESCE(pct.team_id, p.team_id) AS team_id,
    NULL::integer AS league_id,
    NULL::text AS league_name,
    t.short_code AS team_abbr,
    t.name AS team_name,
    t.logo_url AS team_logo_url,
    NULLIF(pos.position, 'Unknown'::text) AS position,
    jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), unaccent(lower(p.first_name)), unaccent(lower(p.last_name)), unaccent(lower(replace(p.name, ' '::text, ''::text))), unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
    COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
   FROM players p
     LEFT JOIN public.player_current_team pct ON pct.player_id = p.id AND pct.sport = p.sport
     LEFT JOIN teams t ON t.id = COALESCE(pct.team_id, p.team_id) AND t.sport = p.sport
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

-- ── FOOTBALL (nested DISTINCT ON — latest-season ps row carries league + position;
--    current club now from public.player_current_team, NOT the stale p.team_id) ──
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
            COALESCE(pct.team_id, p.team_id) AS team_id,
            ps.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            t.name AS team_name,
            t.logo_url AS team_logo_url,
            NULLIF(ps.position, 'Unknown'::text) AS position,
            jsonb_build_array(lower(p.first_name), lower(p.last_name), lower(replace(p.name, ' '::text, ''::text)), lower(COALESCE(t.short_code, ''::text)), lower(COALESCE(t.name, ''::text)), lower(COALESCE(l.name, ''::text)), unaccent(lower(p.first_name)), unaccent(lower(p.last_name)), unaccent(lower(replace(p.name, ' '::text, ''::text))), unaccent(lower(COALESCE(t.name, ''::text)))) AS search_tokens,
            jsonb_build_object('display_name', p.name, 'jersey_number', p.meta ->> 'jersey_number'::text, 'foot', p.meta ->> 'foot'::text, 'market_value', (p.meta ->> 'market_value'::text)::bigint, 'contract_until', p.meta ->> 'contract_until'::text) AS meta
           FROM players p
             LEFT JOIN public.player_current_team pct ON pct.player_id = p.id AND pct.sport = p.sport
             LEFT JOIN teams t ON t.id = COALESCE(pct.team_id, p.team_id) AND t.sport = p.sport
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

-- ── Smoke: club now reflects latest-played, not last-seeded ───────────────────
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN
        SELECT 'NBA' AS sport, count(*) FILTER (WHERE type='player') AS players,
               count(*) FILTER (WHERE type='player' AND team_name IS NOT NULL) AS with_club FROM nba.autofill_entities
        UNION ALL
        SELECT 'NFL', count(*) FILTER (WHERE type='player'),
               count(*) FILTER (WHERE type='player' AND team_name IS NOT NULL) FROM nfl.autofill_entities
        UNION ALL
        SELECT 'FOOTBALL', count(*) FILTER (WHERE type='player'),
               count(*) FILTER (WHERE type='player' AND team_name IS NOT NULL) FROM football.autofill_entities
    LOOP
        RAISE NOTICE '076 %: % players, % with a current club', r.sport, r.players, r.with_club;
    END LOOP;

    -- Spot-check the names that motivated the fix (football). Expect latest-played club.
    FOR r IN
        SELECT name, team_name FROM football.autofill_entities
        WHERE type='player' AND id IN (32438 /*Pulisic*/, 37255840 /*Jude Bellingham*/, 24799984 /*Olise*/)
        ORDER BY name
    LOOP
        RAISE NOTICE '076 spot-check: % -> %', r.name, r.team_name;
    END LOOP;
END $$;

COMMIT;

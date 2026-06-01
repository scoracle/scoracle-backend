-- 027_rating_engine_z.sql
--
-- THE KEYSTONE BUILD — the Scoracle Rating Engine (z-score positionless).
-- Canonical spec: planning_docs/SCORACLE_RATING_ENGINE.md (LANDED & LOCKED 2026-05-31).
--
-- Replaces the audited scoring-volume "composite" with a principled, bias-free
-- player rating that works across NBA, football and NFL from public-domain box
-- scores only. One operation rates every entity, all sports:
--
--   z-score each de-duped box-score datapoint against the POSITIONLESS population.
--   Composite  = Σ z (breadth → grinders/all-rounders)
--                [NFL: category-balanced — mean-of-z per phase facet, summed (§1.5)]
--   Specialist = peak z + skill label (irreplaceability → difference-makers)
--   No weighting, no gating, no hand-picked baselines. The z does the scarcity
--   weighting (a rare skill sits further from the mean → larger z, automatically).
--
-- BOTTOM-UP / EVENT-AS-BASE: rating datapoints are expressions over a box-score
-- `stats` JSONB, defined ONCE in rating_datapoints(sport, stats) and applied at
-- BOTH grains — player_stats.stats (season → this file) and event_box_scores.stats
-- (per-event → starline, migration 028). The season aggregate is the faithful
-- event→season rollup that already normalizes the provider's omit-zeros sparsity
-- into explicit zeros, so (stats->>key)::numeric is a real number incl. 0 for dense
-- stats (a 0 is real signal, enters mean/sd) and NULL for role-exclusive stats
-- (GK saves: absent for outfielders ⇒ AVG/STDDEV_POP measure it among keepers and
-- COALESCE(z,0) zeroes non-participants — the "uniform drag").
--
-- This migration is ADDITIVE: it introduces new rating_* columns and leaves the
-- legacy season_composite_score lineage untouched (the live frontend keeps working;
-- it migrates to rating_* via the new leaderboard/meta payloads, then the legacy can
-- be retired). Validated read-only against live 2025 data: reproduces the
-- user-confirmed boards exactly (NBA Wemby rim-z 6.11; NFL category-balanced top-10
-- Stafford→Garrett→M.Jones→McCaffrey→C.Williams→Love→Nacua→Maye→Burns→Anderson).
--
-- New columns (player_stats, players-only — the rating engine is a player dataset):
--   rating_composite        NUMERIC  Σz breadth (NFL: facet-balanced). The engine value.
--   rating_specialist       NUMERIC  peak z over the positive counting set. Irreplaceability.
--   rating_specialty        TEXT     argmax datapoint label (e.g. 'Rim Protection', 'Sacks').
--   rating_composite_rank   NUMERIC  positionless percent_rank*100 of rating_composite (0–100).
--   rating_specialist_rank  NUMERIC  positionless percent_rank*100 of rating_specialist (0–100).
--
-- Players AND teams: the same columns exist on team_stats, populated by a SEPARATE
-- flat scarcity-z team engine (a team is multi-phase → always flat, even NFL; no
-- facet-balancing). The player engine is left byte-identical — the team functions
-- are additive so the validated player boards are never perturbed.
--
-- Functions:
--   rating_datapoints(sport, stats)       NEW — player z-sets (single source of truth).
--   compute_rating(sport, season)         NEW — player season Composite/Specialist + ranks.
--   rating_datapoints_team(sport, stats)  NEW — team z-sets (flat; margin = point/goal diff).
--   compute_team_rating(sport, season)    NEW — team season Composite/Specialist + ranks.
--   finalize_fixture(fixture_id)          MODIFIED — calls compute_rating +
--                                         compute_team_rating after the percentile recompute.
--
-- Lifecycle (§9.3): compute_rating runs on every finalize_fixture for the affected
-- (sport, season) — the same whole-season-recompute cadence the percentile engine
-- already uses. The O(M²)-per-fixture optimization is a documented future item; this
-- build matches the existing engine's behavior, no worse.

BEGIN;

-- ---------------------------------------------------------------------------
-- Columns
-- ---------------------------------------------------------------------------
ALTER TABLE player_stats
    ADD COLUMN IF NOT EXISTS rating_composite       NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist      NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialty       TEXT,
    ADD COLUMN IF NOT EXISTS rating_composite_rank  NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist_rank NUMERIC;

-- Leaderboard read paths: ordered scans within (sport, season).
CREATE INDEX IF NOT EXISTS idx_player_stats_rating_composite
    ON player_stats (sport, season, rating_composite DESC)
    WHERE rating_composite IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_player_stats_rating_specialist
    ON player_stats (sport, season, rating_specialist DESC)
    WHERE rating_specialist IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_player_stats_rating_specialty
    ON player_stats (sport, season, rating_specialty)
    WHERE rating_specialty IS NOT NULL;

-- Team rating columns (same shape; the team board is flat scarcity-z, see below).
ALTER TABLE team_stats
    ADD COLUMN IF NOT EXISTS rating_composite       NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist      NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialty       TEXT,
    ADD COLUMN IF NOT EXISTS rating_composite_rank  NUMERIC,
    ADD COLUMN IF NOT EXISTS rating_specialist_rank NUMERIC;

CREATE INDEX IF NOT EXISTS idx_team_stats_rating_composite
    ON team_stats (sport, season, rating_composite DESC)
    WHERE rating_composite IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_team_stats_rating_specialist
    ON team_stats (sport, season, rating_specialist DESC)
    WHERE rating_specialist IS NOT NULL;

-- ---------------------------------------------------------------------------
-- rating_datapoints — THE single source of truth for the per-sport z-sets.
-- Returns the de-duped datapoint long-format for one box-score `stats` blob, so
-- the SAME definition feeds the season rating (this file) and the per-event
-- starline (migration 028).
--   label   : friendly, unique per datapoint — group key + Specialist display label
--   value   : the datapoint expression (NULL ⇒ non-participant ⇒ z=0)
--   in_comp : votes in the Composite (breadth)
--   in_spec : eligible for the Specialist (positive counting production only)
--   sign    : +1, or -1 for negatives entered as -z in the Composite
--   facet   : phase facet for NFL category-balancing ('all' elsewhere)
-- LANGUAGE sql so the planner can inline it at either grain. v1 z-sets FROZEN (§4/§9).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION rating_datapoints(p_sport TEXT, p_stats JSONB)
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA (flat-z): pts, reb, ast, stl, blk, fg3m, +plus_minus, -turnover, -pf
    SELECT * FROM (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all'),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NBA'

    UNION ALL
    -- FOOTBALL (flat-z): top-5 leagues pooled, GK in the same positionless pool.
    SELECT * FROM (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all'),
        ('Shooting',        NULLIF(p_stats->>'shots_total','')::numeric,      TRUE, TRUE,   1, 'all'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Tackling',        NULLIF(p_stats->>'tackles','')::numeric,          TRUE, TRUE,   1, 'all'),
        ('Interceptions',   NULLIF(p_stats->>'interceptions','')::numeric,    TRUE, TRUE,   1, 'all'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       TRUE, TRUE,   1, 'all'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           TRUE, TRUE,   1, 'all'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    TRUE, TRUE,   1, 'all'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      TRUE, TRUE,   1, 'all'),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all'),
        -- GK-exclusive: absent for outfielders ⇒ NULL ⇒ scarcity measured among keepers.
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all'),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all'),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL'

    UNION ALL
    -- NFL (category-balanced: offense / defense / special facets). All 17 positions,
    -- one pool. "A yard is a yard / a TD is a TD" — return yardage & return TDs fold
    -- into Total Yards / Touchdowns (no standalone return slot — it manufactured
    -- thin-population freak z's). total_tackles is provider-given.
    SELECT * FROM (VALUES
        -- OFFENSE
        ('Total Yards',      COALESCE((p_stats->>'passing_yards')::numeric,0)
                           + COALESCE((p_stats->>'rushing_yards')::numeric,0)
                           + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                           + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                           + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0),
                                                                                  TRUE, TRUE,   1, 'offense'),
        ('Touchdowns',       COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                           + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0),
                                                                                  TRUE, TRUE,   1, 'offense'),
        ('Receiving',        NULLIF(p_stats->>'receptions','')::numeric,         TRUE, TRUE,   1, 'offense'),
        ('Giveaways',        COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                           + COALESCE((p_stats->>'fumbles_lost')::numeric,0),    TRUE, FALSE, -1, 'offense'),
        -- DEFENSE
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,      TRUE, TRUE,   1, 'defense'),
        ('Tackles For Loss', NULLIF(p_stats->>'tackles_for_loss','')::numeric,   TRUE, TRUE,   1, 'defense'),
        ('Sacks',            NULLIF(p_stats->>'defensive_sacks','')::numeric,     TRUE, TRUE,   1, 'defense'),
        ('Pass Defense',     NULLIF(p_stats->>'passes_defended','')::numeric,     TRUE, TRUE,   1, 'defense'),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense'),
        ('Fumble Recovery',  NULLIF(p_stats->>'fumbles_recovered','')::numeric,   TRUE, TRUE,   1, 'defense'),
        -- SPECIAL TEAMS
        ('Field Goals',      NULLIF(p_stats->>'field_goals_made','')::numeric,    TRUE, TRUE,   1, 'special'),
        ('Punting',          NULLIF(p_stats->>'punts_inside_20','')::numeric,     TRUE, TRUE,   1, 'special')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL';
$$;

-- ---------------------------------------------------------------------------
-- compute_rating — season Composite/Specialist for one (sport, season).
-- Reads the qualified population from player_stats, derives datapoints via the
-- shared rating_datapoints(), z-scores positionlessly, writes the rating columns.
-- Guards (§6): NULLIF(sd,0)+COALESCE(z,0). Floors: NBA ≥30 GP & ≥20 MPG;
-- FOOTBALL ≥15 apps; NFL ≥8 GP.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION compute_rating(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated  INTEGER := 0;
    -- Category-balanced Composite ONLY where players are single-phase (NFL).
    -- NBA + FOOTBALL stay flat-z (multi-phase players). See spec §1.5.
    v_balanced BOOLEAN := (p_sport = 'NFL');
BEGIN
    UPDATE player_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL);

    DROP TABLE IF EXISTS _rating_dp;
    CREATE TEMP TABLE _rating_dp (
        player_id INTEGER, league_id INTEGER, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    -- Qualified population × the shared datapoint definitions. Floors per §6.
    INSERT INTO _rating_dp
    SELECT ps.player_id, COALESCE(ps.league_id, 0),
           dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM player_stats ps
    CROSS JOIN LATERAL rating_datapoints(p_sport, ps.stats) dp
    WHERE ps.sport = p_sport AND ps.season = p_season
      AND CASE p_sport
            WHEN 'NBA'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0) >= 30
                             AND COALESCE((ps.stats->>'minutes')::numeric, 0)      >= 20
            WHEN 'FOOTBALL' THEN COALESCE((ps.stats->>'appearances')::numeric, 0)  >= 15
            WHEN 'NFL'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0)  >= 8
            ELSE FALSE
          END;

    -- Positionless z, then Composite (flat or facet-balanced) + Specialist (peak + label).
    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _rating_dp GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _rating_dp d JOIN pop p USING (label)
    ),
    comp_flat AS (
        SELECT player_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY player_id, league_id
    ),
    comp_facet AS (
        SELECT player_id, league_id, SUM(facet_mean) AS composite
        FROM (
            SELECT player_id, league_id, facet, AVG(sign * zr) AS facet_mean
            FROM z WHERE in_comp GROUP BY player_id, league_id, facet
        ) fm
        GROUP BY player_id, league_id
    ),
    composite AS (
        SELECT player_id, league_id, composite FROM comp_flat  WHERE NOT v_balanced
        UNION ALL
        SELECT player_id, league_id, composite FROM comp_facet WHERE     v_balanced
    ),
    spec AS (
        SELECT DISTINCT ON (player_id, league_id)
               player_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY player_id, league_id, zr DESC
    )
    UPDATE player_stats ps SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (player_id, league_id)
    WHERE ps.player_id = c.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = c.league_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    -- Display ranks: positionless percent_rank (0–100) over the raw z scores —
    -- a friendly 0–100 for the meta card / leaderboard; the raw z stays the engine.
    WITH r AS (
        SELECT player_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS crank,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS srank
        FROM player_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE player_stats ps SET rating_composite_rank = r.crank, rating_specialist_rank = r.srank
    FROM r
    WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = r.league_id;

    RETURN v_updated;
END;
$$;

-- ---------------------------------------------------------------------------
-- TEAM rating engine — flat scarcity-z. Same de-duping discipline + z-scoring as
-- the player engine, but ALWAYS flat (a team is multi-phase, so no facet-balancing
-- — even NFL). Kept as separate functions so the validated player path is never
-- perturbed. Results enter via the point/goal margin (the team plus_minus analog);
-- NBA margin COALESCEs point_differential (season) → plus_minus (event), so one
-- definition works at both grains.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION rating_datapoints_team(p_sport TEXT, p_stats JSONB)
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA team
    SELECT * FROM (VALUES
        ('Scoring',           NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1),
        ('Rebounding',        NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1),
        ('Playmaking',        NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1),
        ('Steals',            NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1),
        ('Rim Protection',    NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1),
        ('3PT Shooting',      NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1),
        ('Point Differential',COALESCE((p_stats->>'point_differential')::numeric,
                                       (p_stats->>'plus_minus')::numeric),TRUE, FALSE,  1),
        ('Ball Security',     NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1)
    ) v(label, value, in_comp, in_spec, sign) WHERE p_sport = 'NBA'

    UNION ALL
    -- NFL team (flat — no offense/defense/special facets for teams)
    SELECT * FROM (VALUES
        ('Total Yards',       NULLIF(p_stats->>'total_yards','')::numeric,           TRUE, TRUE,   1),
        ('Point Differential',NULLIF(p_stats->>'point_differential','')::numeric,    TRUE, FALSE,  1),
        ('Tackling',          NULLIF(p_stats->>'total_tackles','')::numeric,         TRUE, TRUE,   1),
        ('Sacks',             NULLIF(p_stats->>'defensive_sacks','')::numeric,       TRUE, TRUE,   1),
        ('Pass Defense',      NULLIF(p_stats->>'passes_defended','')::numeric,       TRUE, TRUE,   1),
        ('Interceptions',     NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1),
        ('Giveaways',         NULLIF(p_stats->>'turnovers','')::numeric,             TRUE, FALSE, -1)
    ) v(label, value, in_comp, in_spec, sign) WHERE p_sport = 'NFL'

    UNION ALL
    -- FOOTBALL team
    SELECT * FROM (VALUES
        ('Goals For',         NULLIF(p_stats->>'goals_for','')::numeric,       TRUE, TRUE,   1),
        ('Goal Difference',   NULLIF(p_stats->>'goal_difference','')::numeric, TRUE, FALSE,  1),
        ('Shooting',          NULLIF(p_stats->>'shots_on_target','')::numeric, TRUE, TRUE,   1),
        ('Creation',          NULLIF(p_stats->>'key_passes','')::numeric,      TRUE, TRUE,   1),
        ('Tackling',          NULLIF(p_stats->>'tackles','')::numeric,         TRUE, TRUE,   1),
        ('Interceptions',     NULLIF(p_stats->>'interceptions','')::numeric,   TRUE, TRUE,   1),
        ('Clearances',        NULLIF(p_stats->>'clearances','')::numeric,      TRUE, TRUE,   1),
        ('Possession Lost',   NULLIF(p_stats->>'possession_lost','')::numeric, TRUE, FALSE, -1)
    ) v(label, value, in_comp, in_spec, sign) WHERE p_sport = 'FOOTBALL';
$$;

CREATE OR REPLACE FUNCTION compute_team_rating(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL);

    DROP TABLE IF EXISTS _team_dp;
    CREATE TEMP TABLE _team_dp (
        team_id INTEGER, league_id INTEGER, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER
    ) ON COMMIT DROP;

    INSERT INTO _team_dp
    SELECT ts.team_id, COALESCE(ts.league_id, 0),
           dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign
    FROM team_stats ts
    CROSS JOIN LATERAL rating_datapoints_team(p_sport, ts.stats) dp
    WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats <> '{}'::jsonb;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.in_comp, d.in_spec, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    composite AS (
        SELECT team_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY team_id, league_id
    ),
    spec AS (
        SELECT DISTINCT ON (team_id, league_id)
               team_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY team_id, league_id, zr DESC
    )
    UPDATE team_stats ts SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (team_id, league_id)
    WHERE ts.team_id = c.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = c.league_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    WITH r AS (
        SELECT team_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS crank,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS srank
        FROM team_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE team_stats ts SET rating_composite_rank = r.crank, rating_specialist_rank = r.srank
    FROM r
    WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = r.league_id;

    RETURN v_updated;
END;
$$;

-- ---------------------------------------------------------------------------
-- finalize_fixture — add the in-season rating recompute. Body is the live
-- definition verbatim plus a single PERFORM compute_rating after the existing
-- event-percentile recompute (the season aggregate is fresh at that point).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.finalize_fixture(p_fixture_id integer)
 RETURNS TABLE(players_updated integer, teams_updated integer)
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_sport TEXT;
    v_season INTEGER;
    v_league_id INTEGER;
    v_home_team_id INTEGER;
    v_away_team_id INTEGER;
    v_home_score INTEGER;
    v_away_score INTEGER;
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    SELECT f.sport, f.season, COALESCE(f.league_id, 0),
           f.home_team_id, f.away_team_id
    INTO v_sport, v_season, v_league_id, v_home_team_id, v_away_team_id
    FROM fixtures f WHERE f.id = p_fixture_id;

    IF v_sport IS NULL THEN
        RAISE EXCEPTION 'fixture % not found', p_fixture_id;
    END IF;

    IF v_sport = 'NBA' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id, 'NBA', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nba.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'NBA', v_season, v_league_id,
            COALESCE(nba.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();

    ELSIF v_sport = 'NFL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id, 'NFL', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nfl.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'NFL', v_season, v_league_id,
            COALESCE(nfl.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();

    ELSIF v_sport = 'FOOTBALL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id, 'FOOTBALL', v_season, v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(football.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'FOOTBALL', v_season, v_league_id,
            COALESCE(football.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();
    END IF;

    -- Season-level percentile recompute (existing).
    SELECT rp.players_updated, rp.teams_updated
    INTO v_players, v_teams
    FROM recalculate_percentiles(v_sport, v_season) rp;

    -- Event-level percentile recompute (migration 017).
    -- Re-ranks all events for the (sport, season), then rolls
    -- season_composite_score back onto player_stats / team_stats.
    PERFORM recalculate_event_percentiles(v_sport, v_season);

    -- Rating engine recompute (migration 027). Reads the fresh season aggregate;
    -- writes rating_composite / rating_specialist / rating_specialty + ranks.
    PERFORM compute_rating(v_sport, v_season);

    -- Team rating recompute (migration 027 — flat scarcity-z over team_stats).
    PERFORM compute_team_rating(v_sport, v_season);

    IF v_sport = 'NBA' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF v_sport = 'NFL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF v_sport = 'FOOTBALL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    SELECT score INTO v_home_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_home_team_id;
    SELECT score INTO v_away_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_away_team_id;

    PERFORM mark_fixture_seeded(p_fixture_id, v_home_score, v_away_score);

    RETURN QUERY SELECT v_players, v_teams;
END;
$function$;

-- ---------------------------------------------------------------------------
-- Backfill: rate every existing (sport, season) so the columns populate on apply.
-- ---------------------------------------------------------------------------
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats ORDER BY sport, season LOOP
        PERFORM compute_rating(r.sport, r.season);
        PERFORM compute_team_rating(r.sport, r.season);
    END LOOP;
END $$;

COMMIT;

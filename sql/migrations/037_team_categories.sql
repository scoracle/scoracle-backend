-- ============================================================================
-- 037_team_categories.sql
-- Team rating: OFFENSE / DEFENSE categories (display layer) + opposition defense
-- terms + NBA Foul Drawing (fta), and DROP the result-only margins.
--
-- Decisions (2026-06-02..03 streaming session; see RATING_DATAPOINT_AUDIT.md):
--  * Composite stays FLAT Σz. `facet` is a DISPLAY grouping (offense/defense) +
--    a new per-category sub-score; it does NOT category-balance the team composite.
--  * DROP Point Differential (NBA/NFL) and Goal Difference (FOOTBALL): outcomes,
--    not "the HOW". Teams are now rated purely on production/process.
--  * ADD to the team COMPOSITE only gate-checked, distinct terms:
--      NBA  Foul Drawing  = fta                     (team corr 0.37 vs pts)
--      NFL  Yards Allowed = opp total_yards (-z)     (corr <=0.34 vs splash plays)
--      FOOT SoT Allowed   = opp shots_on_target (-z) (corr <=0.59 vs def actions)
--  * Everything else fan-legible rides as DISPLAY-ONLY (in_comp=F, in_spec=F):
--    shown with volume + percentile, never summed — TDs/FGs/first downs, the
--    opponent rates (RZ-def%, opp FG%), the oreb/dreb split, football cards/injuries.
--  * PLAYER engine: add NBA "Foul Drawing" (fta) as SPECIALIST-ONLY (in_spec, not
--    in_comp) — fta corr 0.87 vs pts at player grain, so it surfaces as a peak skill
--    + percentile without polluting the breadth sum. Player composite byte-identical.
--
-- New column: team_stats.rating_categories JSONB = {facet -> {z, pct}} over the
--   in_comp terms of each facet (the "percentile in each category").
--
-- Touches 3 functions: rating_datapoints (player), rating_datapoints_team (signature
--   gains `facet` -> DROP+CREATE), compute_team_rating (facet-aware breakdown +
--   categories). compute_team_event_starline (028) is UNCHANGED: it selects explicit
--   columns from rating_datapoints_team and ignores facet; new display terms are
--   in_comp/in_spec=F so they don't affect the event-grain composite/specialist.
-- ============================================================================

BEGIN;

ALTER TABLE team_stats ADD COLUMN IF NOT EXISTS rating_categories JSONB;

-- ---------------------------------------------------------------------------
-- rating_datapoints (player) — 027 verbatim + NBA "Foul Drawing" (fta), which is
-- SPECIALIST-ONLY (in_comp=FALSE, in_spec=TRUE): rim pressure / getting to the line
-- as a peak skill + pizza percentile, kept out of the (pts-correlated) breadth sum.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION rating_datapoints(p_sport TEXT, p_stats JSONB)
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA (flat-z): pts, reb, ast, stl, blk, fg3m, +plus_minus, -turnover, -pf; +fta (spec-only)
    SELECT * FROM (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all'),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all'),
        ('Foul Drawing',    NULLIF(p_stats->>'fta','')::numeric,        FALSE, TRUE,  1, 'all')
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
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all'),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all'),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL'

    UNION ALL
    -- NFL (category-balanced: offense / defense / special facets). Unchanged.
    SELECT * FROM (VALUES
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
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,      TRUE, TRUE,   1, 'defense'),
        ('Tackles For Loss', NULLIF(p_stats->>'tackles_for_loss','')::numeric,   TRUE, TRUE,   1, 'defense'),
        ('Sacks',            NULLIF(p_stats->>'defensive_sacks','')::numeric,     TRUE, TRUE,   1, 'defense'),
        ('Pass Defense',     NULLIF(p_stats->>'passes_defended','')::numeric,     TRUE, TRUE,   1, 'defense'),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense'),
        ('Fumble Recovery',  NULLIF(p_stats->>'fumbles_recovered','')::numeric,   TRUE, TRUE,   1, 'defense'),
        ('Field Goals',      NULLIF(p_stats->>'field_goals_made','')::numeric,    TRUE, TRUE,   1, 'special'),
        ('Punting',          NULLIF(p_stats->>'punts_inside_20','')::numeric,     TRUE, TRUE,   1, 'special')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL';
$$;

-- ---------------------------------------------------------------------------
-- rating_datapoints_team — gains `facet` (offense/defense + display-only
-- discipline/squad). Signature change ⇒ DROP first. Composite = the `C`/`Cn` rows
-- (in_comp); Specialist = `C` rows (in_spec); display-only rows carry pct only.
-- ---------------------------------------------------------------------------
DROP FUNCTION IF EXISTS rating_datapoints_team(TEXT, JSONB);

CREATE FUNCTION rating_datapoints_team(p_sport TEXT, p_stats JSONB)
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA team — offense / defense. +Foul Drawing (fta, composite: team corr 0.37).
    -- Margin (point_differential) DROPPED. oreb/dreb + opp FG% are display-only.
    SELECT * FROM (VALUES
        ('Scoring',            NULLIF(p_stats->>'pts','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Playmaking',         NULLIF(p_stats->>'ast','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('3PT Shooting',       NULLIF(p_stats->>'fg3m','')::numeric,        TRUE,  TRUE,   1, 'offense'),
        ('Foul Drawing',       NULLIF(p_stats->>'fta','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Ball Security',      NULLIF(p_stats->>'turnover','')::numeric,    TRUE,  FALSE, -1, 'offense'),
        ('Offensive Rebounds', NULLIF(p_stats->>'oreb','')::numeric,        FALSE, FALSE,  1, 'offense'),
        ('Rim Protection',     NULLIF(p_stats->>'blk','')::numeric,         TRUE,  TRUE,   1, 'defense'),
        ('Steals',             NULLIF(p_stats->>'stl','')::numeric,         TRUE,  TRUE,   1, 'defense'),
        ('Rebounding',         NULLIF(p_stats->>'reb','')::numeric,         TRUE,  TRUE,   1, 'defense'),
        ('Defensive Rebounds', NULLIF(p_stats->>'dreb','')::numeric,        FALSE, FALSE,  1, 'defense'),
        ('Opp FG%',            NULLIF(p_stats->>'def_fg_pct','')::numeric,  FALSE, FALSE, -1, 'defense'),
        ('Opp 3PT%',           NULLIF(p_stats->>'def_fg3_pct','')::numeric, FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NBA'

    UNION ALL
    -- NFL team — offense / defense. +Yards Allowed (composite -z). Margin DROPPED.
    SELECT * FROM (VALUES
        ('Total Yards',        NULLIF(p_stats->>'total_yards','')::numeric,                TRUE,  TRUE,   1, 'offense'),
        ('Giveaways',          NULLIF(p_stats->>'turnovers','')::numeric,                  TRUE,  FALSE, -1, 'offense'),
        ('Touchdowns',         COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                             + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0),      FALSE, FALSE,  1, 'offense'),
        ('First Downs',        NULLIF(p_stats->>'first_downs','')::numeric,                FALSE, FALSE,  1, 'offense'),
        ('Field Goals',        NULLIF(p_stats->>'field_goals_made','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Red Zone %',         NULLIF(p_stats->>'red_zone_pct','')::numeric,               FALSE, FALSE,  1, 'offense'),
        ('Third Down %',       NULLIF(p_stats->>'third_down_pct','')::numeric,             FALSE, FALSE,  1, 'offense'),
        ('Tackling',           NULLIF(p_stats->>'total_tackles','')::numeric,              TRUE,  TRUE,   1, 'defense'),
        ('Sacks',              NULLIF(p_stats->>'defensive_sacks','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Pass Defense',       NULLIF(p_stats->>'passes_defended','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',      NULLIF(p_stats->>'defensive_interceptions','')::numeric,    TRUE,  TRUE,   1, 'defense'),
        ('Yards Allowed',      NULLIF(p_stats->>'yards_allowed','')::numeric,              TRUE,  FALSE, -1, 'defense'),
        ('Tackles For Loss',   NULLIF(p_stats->>'tackles_for_loss','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Takeaways',          NULLIF(p_stats->>'takeaways','')::numeric,                  FALSE, FALSE,  1, 'defense'),
        ('Red Zone Def %',     NULLIF(p_stats->>'red_zone_def_pct','')::numeric,           FALSE, FALSE, -1, 'defense'),
        ('Third Down Def %',   NULLIF(p_stats->>'third_down_def_pct','')::numeric,         FALSE, FALSE, -1, 'defense'),
        ('First Downs Allowed',NULLIF(p_stats->>'first_downs_allowed','')::numeric,        FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL'

    UNION ALL
    -- FOOTBALL team — offense (attacking+possession) / defense. +SoT Allowed
    -- (composite -z). Margin (goal_difference) DROPPED. Cards/injuries = display.
    SELECT * FROM (VALUES
        ('Goals For',            NULLIF(p_stats->>'goals_for','')::numeric,               TRUE,  TRUE,   1, 'offense'),
        ('Shooting',             NULLIF(p_stats->>'shots_on_target','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Creation',             NULLIF(p_stats->>'key_passes','')::numeric,              TRUE,  TRUE,   1, 'offense'),
        ('Possession Lost',      NULLIF(p_stats->>'possession_lost','')::numeric,         TRUE,  FALSE, -1, 'offense'),
        ('Possession %',         NULLIF(p_stats->>'possession_pct','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Accurate Passes',      NULLIF(p_stats->>'accurate_passes','')::numeric,         FALSE, FALSE,  1, 'offense'),
        ('Big Chances Created',  NULLIF(p_stats->>'big_chances_created','')::numeric,      FALSE, FALSE,  1, 'offense'),
        ('Successful Dribbles',  NULLIF(p_stats->>'successful_dribbles','')::numeric,      FALSE, FALSE,  1, 'offense'),
        ('Tackling',             NULLIF(p_stats->>'tackles','')::numeric,                 TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',        NULLIF(p_stats->>'interceptions','')::numeric,           TRUE,  TRUE,   1, 'defense'),
        ('Clearances',           NULLIF(p_stats->>'clearances','')::numeric,              TRUE,  TRUE,   1, 'defense'),
        ('SoT Allowed',          NULLIF(p_stats->>'shots_on_target_allowed','')::numeric, TRUE,  FALSE, -1, 'defense'),
        ('Blocked Shots',        NULLIF(p_stats->>'blocked_shots','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Ball Recovery',        NULLIF(p_stats->>'ball_recovery','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Shots Allowed',        NULLIF(p_stats->>'shots_allowed','')::numeric,           FALSE, FALSE, -1, 'defense'),
        ('Big Chances Allowed',  NULLIF(p_stats->>'big_chances_allowed','')::numeric,      FALSE, FALSE, -1, 'defense'),
        ('Yellow Cards',         NULLIF(p_stats->>'yellow_cards_total','')::numeric,       FALSE, FALSE, -1, 'discipline'),
        ('Red Cards',            NULLIF(p_stats->>'red_cards_total','')::numeric,          FALSE, FALSE, -1, 'discipline'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                FALSE, FALSE, -1, 'squad')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$$;

-- ---------------------------------------------------------------------------
-- compute_team_rating — flat composite (UNCHANGED math) + facet-aware breakdown
-- + per-category sub-scores (rating_categories). _team_dp now carries facet.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION compute_team_rating(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_categories = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL);

    DROP TABLE IF EXISTS _team_dp;
    CREATE TEMP TABLE _team_dp (
        team_id INTEGER, league_id INTEGER, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    INSERT INTO _team_dp
    SELECT ts.team_id, COALESCE(ts.league_id, 0),
           dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM team_stats ts
    CROSS JOIN LATERAL rating_datapoints_team(p_sport, ts.stats) dp
    WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats <> '{}'::jsonb;

    -- Composite (flat Σz over in_comp) + Specialist (peak in_spec + label). UNCHANGED.
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

    -- ── Per-datapoint breakdown (now facet-aware) ────────────────────────────
    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    scored AS (
        SELECT team_id, league_id, label, in_comp, in_spec, sign, facet, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct
        FROM z
    ),
    peak AS (
        SELECT DISTINCT ON (team_id, league_id) team_id, league_id, label AS spec_label
        FROM z WHERE in_spec
        ORDER BY team_id, league_id, zr DESC
    ),
    agg AS (
        SELECT s.team_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label,
                   'z',     ROUND(s.zr, 4),
                   'pct',   s.pct,
                   'in_comp', s.in_comp,
                   'in_spec', s.in_spec,
                   'sign',  s.sign,
                   'facet', s.facet,
                   'is_specialty', (pk.spec_label IS NOT DISTINCT FROM s.label)
               ) ORDER BY s.facet, s.label) AS breakdown
        FROM scored s
        LEFT JOIN peak pk USING (team_id, league_id)
        GROUP BY s.team_id, s.league_id
    )
    UPDATE team_stats ts SET rating_breakdown = a.breakdown
    FROM agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id
      AND ts.rating_composite IS NOT NULL;

    -- ── Per-category sub-scores (rating_categories) ──────────────────────────
    -- Category z = mean of sign*zr over the IN_COMP terms of each facet (so facets
    -- with different counts are comparable); pct = percent_rank of that within the
    -- (sport, season, facet) population. Display-only facets (discipline/squad)
    -- have no in_comp terms, so only offense/defense produce a category score.
    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.in_comp, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    cat AS (
        SELECT team_id, league_id, facet, AVG(sign * zr) AS cat_z
        FROM z WHERE in_comp GROUP BY team_id, league_id, facet
    ),
    cat_ranked AS (
        SELECT team_id, league_id, facet, cat_z,
               ROUND((percent_rank() OVER (PARTITION BY facet ORDER BY cat_z ASC))::numeric * 100, 1) AS cat_pct
        FROM cat
    ),
    cat_agg AS (
        SELECT team_id, league_id,
               jsonb_object_agg(facet, jsonb_build_object('z', ROUND(cat_z, 4), 'pct', cat_pct)) AS cats
        FROM cat_ranked GROUP BY team_id, league_id
    )
    UPDATE team_stats ts SET rating_categories = a.cats
    FROM cat_agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id
      AND ts.rating_composite IS NOT NULL;

    RETURN v_updated;
END;
$$;

-- ---------------------------------------------------------------------------
-- Backfill: re-rate teams (all sports — new facets/categories/datapoints/margins)
-- and re-rate NBA players (Foul Drawing added). Player composite is byte-identical
-- (fta is in_spec only); team composites move by design (fta / *_allowed / dropped
-- margins). Idempotent.
-- ---------------------------------------------------------------------------
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM team_stats ORDER BY sport, season LOOP
        PERFORM compute_team_rating(r.sport, r.season);
    END LOOP;
    FOR r IN SELECT DISTINCT season FROM player_stats WHERE sport = 'NBA' ORDER BY season LOOP
        PERFORM compute_rating('NBA', r.season);
    END LOOP;
END $$;

COMMIT;

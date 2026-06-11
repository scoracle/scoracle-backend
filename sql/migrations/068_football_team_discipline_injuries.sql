-- ============================================================================
-- 068 — Football TEAM composite: Yellow Cards + Injuries earn their place.
--
-- They were display-only (orphaned since the Discipline/Squad card was removed) and
-- pass the gate vs goal difference (n=192 team-seasons):
--      Yellow Cards   value -0.35   reliability 0.71   → strong, clear add
--      Injuries       value -0.24   reliability 0.36   → real, add
--      Red Cards      value -0.24   reliability 0.15   → DROPPED (too rare/noisy — Scott)
-- Promote Yellow Cards (→ 'discipline' facet) and Injuries (→ 'offense' facet) to in_comp.
--
-- Also fixes a latent gap: compute_team_rating never rebuilt rating_breakdown (the
-- player bundle does; team breakdowns were only refreshed by finalize_fixture, so a
-- datapoint change applied via a migration left the stored breakdown stale). 068 makes
-- compute_team_rating rebuild rating_breakdown every run — so the new datapoints (and
-- any future team-datapoint change) actually surface on the pizza.
--
-- FOOTBALL team branch only; NBA/NFL datapoints untouched. Recompute football teams.
-- No API restart.  Apply with: ./sql/migrate.sh
-- ============================================================================

BEGIN;

-- ── 1. rating_datapoints_team — Yellow Cards/Injuries → composite (FOOTBALL only) ──
CREATE OR REPLACE FUNCTION public.rating_datapoints_team(p_sport text, p_stats jsonb)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
 LANGUAGE sql
 IMMUTABLE PARALLEL SAFE
AS $function$
    -- NBA team (unchanged).
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
    -- NFL team (unchanged).
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
        ('First Downs Allowed',NULLIF(p_stats->>'first_downs_allowed','')::numeric,        FALSE, FALSE, -1, 'defense'),
        ('Penalty Yards For',     NULLIF(p_stats->>'penalty_yards_drawn','')::numeric,     TRUE,  FALSE,  1, 'discipline'),
        ('Penalty Yards Against', NULLIF(p_stats->>'penalty_yards','')::numeric,           TRUE,  FALSE, -1, 'discipline')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL'

    UNION ALL
    -- FOOTBALL team. Yellow Cards → composite (discipline); Injuries → composite
    -- (offense). Red Cards stay display (too rare/noisy, rel 0.15).
    SELECT * FROM (VALUES
        ('Goals For',            NULLIF(p_stats->>'goals_for','')::numeric,               TRUE,  TRUE,   1, 'offense'),
        ('Shooting',             NULLIF(p_stats->>'shots_on_target','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Creation',             NULLIF(p_stats->>'key_passes','')::numeric,              TRUE,  TRUE,   1, 'offense'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                TRUE,  FALSE, -1, 'offense'),
        ('Penalties Won',        NULLIF(p_stats->>'penalties_won','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Fouls Won',            NULLIF(p_stats->>'fouls_drawn','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Possession Lost',      NULLIF(p_stats->>'possession_lost','')::numeric,         TRUE,  FALSE, -1, 'offense'),
        ('Possession %',         NULLIF(p_stats->>'possession_pct','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Accurate Passes',      NULLIF(p_stats->>'accurate_passes','')::numeric,         FALSE, FALSE,  1, 'offense'),
        ('Big Chances Created',  NULLIF(p_stats->>'big_chances_created','')::numeric,      FALSE, FALSE,  1, 'offense'),
        ('Successful Dribbles',  NULLIF(p_stats->>'successful_dribbles','')::numeric,      FALSE, FALSE,  1, 'offense'),
        ('Tackling',             round(NULLIF(p_stats->>'tackles','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),                 TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',        round(NULLIF(p_stats->>'interceptions','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),           TRUE,  TRUE,   1, 'defense'),
        ('Clearances',           NULLIF(p_stats->>'clearances','')::numeric,              FALSE, FALSE,   1, 'defense'),
        ('SoT Allowed',          NULLIF(p_stats->>'shots_on_target_allowed','')::numeric, TRUE,  FALSE, -1, 'defense'),
        ('Penalties Conceded',   NULLIF(p_stats->>'penalties_committed','')::numeric,      TRUE,  FALSE, -1, 'defense'),
        ('Blocked Shots',        NULLIF(p_stats->>'blocked_shots','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Ball Recovery',        NULLIF(p_stats->>'ball_recovery','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Shots Allowed',        NULLIF(p_stats->>'shots_allowed','')::numeric,           TRUE, FALSE, -1, 'defense'),
        ('Big Chances Allowed',  NULLIF(p_stats->>'big_chances_allowed','')::numeric,      TRUE, FALSE, -1, 'defense'),
        ('Goals Against', NULLIF(p_stats->>'goals_against','')::numeric, TRUE, FALSE, -1, 'defense'),
        ('Fouls Committed',      NULLIF(p_stats->>'fouls_committed','')::numeric,      FALSE, FALSE, -1, 'defense'),
        ('Yellow Cards',         NULLIF(p_stats->>'yellow_cards_total','')::numeric,       TRUE,  FALSE, -1, 'discipline'),
        ('Red Cards',            NULLIF(p_stats->>'red_cards_total','')::numeric,          FALSE, FALSE, -1, 'discipline')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;

-- ── 2. compute_team_rating — now ALSO rebuilds rating_breakdown each run ──────
CREATE OR REPLACE FUNCTION public.compute_team_rating(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_composite_score = NULL, rating_specialist_score = NULL, rating_scoped_scores = NULL,
           rating_categories = NULL, rating_scoped_ranks = NULL, rating_breakdown = NULL
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

    -- Rebuild rating_breakdown (the gap this migration closes).
    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    scored AS (
        SELECT team_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct
        FROM z
    ),
    peak AS (
        SELECT DISTINCT ON (team_id, league_id) team_id, league_id, label AS spec_label
        FROM z WHERE in_spec ORDER BY team_id, league_id, zr DESC
    ),
    agg AS (
        SELECT s.team_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet,
                   'is_specialty', (pk.spec_label IS NOT DISTINCT FROM s.label)
               ) ORDER BY s.facet, s.label) AS breakdown
        FROM scored s LEFT JOIN peak pk USING (team_id, league_id)
        GROUP BY s.team_id, s.league_id
    )
    UPDATE team_stats ts SET rating_breakdown = a.breakdown
    FROM agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id AND ts.rating_composite IS NOT NULL;

    WITH r AS (
        SELECT team_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS crank,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS srank,
               public.rating_score(rating_composite,  AVG(rating_composite)  OVER(), STDDEV_POP(rating_composite)  OVER()) AS cscore,
               public.rating_score(rating_specialist, AVG(rating_specialist) OVER(), STDDEV_POP(rating_specialist) OVER()) AS sscore
        FROM team_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE team_stats ts SET rating_composite_rank = r.crank, rating_specialist_rank = r.srank,
                             rating_composite_score = r.cscore, rating_specialist_score = r.sscore
    FROM r
    WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = r.league_id;

    RETURN v_updated;
END;
$function$;

-- ── 3. Recompute football teams (composite + breakdown + magnitude score) ────
ALTER TABLE team_stats DISABLE TRIGGER trg_percentile_changed_team_stats;
DO $$
DECLARE s INTEGER;
BEGIN
    FOR s IN SELECT DISTINCT season FROM team_stats WHERE sport='FOOTBALL' AND rating_composite IS NOT NULL ORDER BY 1 LOOP
        PERFORM compute_team_rating('FOOTBALL', s);
    END LOOP;
END $$;
ALTER TABLE team_stats ENABLE TRIGGER trg_percentile_changed_team_stats;

-- ── 4. Gate: Yellow Cards + Injuries are composite AND in the rebuilt breakdown ─
DO $$
DECLARE v_fn INTEGER; v_bd INTEGER;
BEGIN
    SELECT count(*) INTO v_fn
    FROM rating_datapoints_team('FOOTBALL',
            (SELECT stats FROM team_stats WHERE sport='FOOTBALL' AND season=2025 AND rating_composite IS NOT NULL LIMIT 1)) d
    WHERE d.label IN ('Yellow Cards','Injuries') AND d.in_comp;

    SELECT count(DISTINCT el->>'label') INTO v_bd
    FROM team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) el
    WHERE ts.sport='FOOTBALL' AND ts.season=2025 AND ts.rating_breakdown IS NOT NULL
      AND el->>'label' IN ('Yellow Cards','Injuries') AND (el->>'in_comp')::bool;

    IF v_fn <> 2 THEN RAISE EXCEPTION '068 FAIL: function emits % composite cards/injuries (want 2)', v_fn; END IF;
    IF v_bd <> 2 THEN RAISE EXCEPTION '068 FAIL: rebuilt breakdown has % composite cards/injuries (want 2)', v_bd; END IF;
    RAISE NOTICE '068 OK: Yellow Cards (discipline) + Injuries (offense) in composite + breakdown rebuilt';
END $$;

COMMIT;

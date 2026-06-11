-- ============================================================================
-- 069 — Fold the team 'discipline' facet into offense/defense (display regroup).
--
-- A lone-wedge Discipline pizza (football: just Yellow Cards after Red Cards stayed
-- display) reads thin. Fold discipline datapoints into offense/defense (Scott):
--   FOOTBALL  Yellow Cards  → defense   (a defensive-discipline cost)
--   NFL       Penalty Yards For     → offense   (drawn penalties help the offense)
--   NFL       Penalty Yards Against → defense   (committed penalties hurt)
-- Red Cards (display) also moves to defense so nothing uses 'discipline' anymore.
--
-- The team composite is a FLAT Σ(sign·z) over in_comp, independent of facet, so this
-- does NOT change any rating — only which pizza a wedge renders in (the stored
-- rating_breakdown's facet tag). Recompute football + NFL teams to refresh the
-- breakdown facets; gate asserts the composite is byte-identical (parity). NBA
-- untouched. Frontend drops 'discipline' from PIZZA_FACETS.
--
-- Apply with: ./sql/migrate.sh
-- ============================================================================

BEGIN;

-- Parity snapshot — ratings must be unchanged (facet is display-only).
CREATE TEMP TABLE _069_pre AS
    SELECT team_id, sport, season, COALESCE(league_id,0) lid, rating_composite, rating_composite_rank
    FROM team_stats WHERE sport IN ('FOOTBALL','NFL') AND rating_composite IS NOT NULL;

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
    -- NFL team — penalty yards folded into offense (for) / defense (against).
    SELECT * FROM (VALUES
        ('Total Yards',        NULLIF(p_stats->>'total_yards','')::numeric,                TRUE,  TRUE,   1, 'offense'),
        ('Giveaways',          NULLIF(p_stats->>'turnovers','')::numeric,                  TRUE,  FALSE, -1, 'offense'),
        ('Touchdowns',         COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                             + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0),      FALSE, FALSE,  1, 'offense'),
        ('First Downs',        NULLIF(p_stats->>'first_downs','')::numeric,                FALSE, FALSE,  1, 'offense'),
        ('Field Goals',        NULLIF(p_stats->>'field_goals_made','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Red Zone %',         NULLIF(p_stats->>'red_zone_pct','')::numeric,               FALSE, FALSE,  1, 'offense'),
        ('Third Down %',       NULLIF(p_stats->>'third_down_pct','')::numeric,             FALSE, FALSE,  1, 'offense'),
        ('Penalty Yards For',  NULLIF(p_stats->>'penalty_yards_drawn','')::numeric,        TRUE,  FALSE,  1, 'offense'),
        ('Tackling',           NULLIF(p_stats->>'total_tackles','')::numeric,              TRUE,  TRUE,   1, 'defense'),
        ('Sacks',              NULLIF(p_stats->>'defensive_sacks','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Pass Defense',       NULLIF(p_stats->>'passes_defended','')::numeric,            TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',      NULLIF(p_stats->>'defensive_interceptions','')::numeric,    TRUE,  TRUE,   1, 'defense'),
        ('Yards Allowed',      NULLIF(p_stats->>'yards_allowed','')::numeric,              TRUE,  FALSE, -1, 'defense'),
        ('Penalty Yards Against', NULLIF(p_stats->>'penalty_yards','')::numeric,           TRUE,  FALSE, -1, 'defense'),
        ('Tackles For Loss',   NULLIF(p_stats->>'tackles_for_loss','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Takeaways',          NULLIF(p_stats->>'takeaways','')::numeric,                  FALSE, FALSE,  1, 'defense'),
        ('Red Zone Def %',     NULLIF(p_stats->>'red_zone_def_pct','')::numeric,           FALSE, FALSE, -1, 'defense'),
        ('Third Down Def %',   NULLIF(p_stats->>'third_down_def_pct','')::numeric,         FALSE, FALSE, -1, 'defense'),
        ('First Downs Allowed',NULLIF(p_stats->>'first_downs_allowed','')::numeric,        FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL'

    UNION ALL
    -- FOOTBALL team — Yellow Cards folded into defense; Red Cards display (defense).
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
        ('Yellow Cards',         NULLIF(p_stats->>'yellow_cards_total','')::numeric,       TRUE,  FALSE, -1, 'defense'),
        ('Red Cards',            NULLIF(p_stats->>'red_cards_total','')::numeric,          FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;

-- Recompute football + NFL teams (refreshes breakdown facets; composite unchanged).
ALTER TABLE team_stats DISABLE TRIGGER trg_percentile_changed_team_stats;
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM team_stats WHERE sport IN ('FOOTBALL','NFL') AND rating_composite IS NOT NULL ORDER BY sport, season LOOP
        PERFORM compute_team_rating(r.sport, r.season);
    END LOOP;
END $$;
ALTER TABLE team_stats ENABLE TRIGGER trg_percentile_changed_team_stats;

-- Gate: ratings byte-identical (parity) + no in_comp 'discipline' wedge remains.
DO $$
DECLARE v_drift INTEGER; v_disc INTEGER;
BEGIN
    SELECT count(*) INTO v_drift
    FROM _069_pre p JOIN team_stats ts
      ON ts.team_id=p.team_id AND ts.sport=p.sport AND ts.season=p.season AND COALESCE(ts.league_id,0)=p.lid
    WHERE ts.rating_composite IS DISTINCT FROM p.rating_composite
       OR ts.rating_composite_rank IS DISTINCT FROM p.rating_composite_rank;

    SELECT count(*) INTO v_disc
    FROM team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) el
    WHERE ts.sport IN ('FOOTBALL','NFL') AND ts.rating_breakdown IS NOT NULL
      AND el->>'facet' = 'discipline';

    IF v_drift > 0 THEN RAISE EXCEPTION '069 FAIL: % team ratings drifted (facet move must be display-only)', v_drift; END IF;
    IF v_disc > 0 THEN RAISE EXCEPTION '069 FAIL: % discipline-facet wedges remain', v_disc; END IF;
    RAISE NOTICE '069 OK: discipline folded into offense/defense; ratings byte-identical';
END $$;

COMMIT;

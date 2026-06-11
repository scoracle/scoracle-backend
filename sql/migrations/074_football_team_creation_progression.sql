-- ============================================================================
-- 074 — Football team OFFENSE: Creation = Big Chances Created; add Progression.
--
--  * Creation: key_passes → big_chances_created. Big chances are a STRICT quality-subset
--    of key passes (0 teams exceed; ~25% of key passes; 0.81 collinear) — the same
--    relationship as shots-on-target ⊆ shots — and higher value (0.90 vs 0.83). So use
--    the quality measure, don't blend (blending re-buries it). 'Big Chances Created'
--    display wedge folds into Creation.
--  * NEW Progression = passes_final_third + successful_dribbles — the ball-progression /
--    "possession + intent" dimension the model was missing. The two components are only
--    0.40 collinear (distinct: territorial passing + carrying), combined value 0.58,
--    reliability 0.85, and 0.69 collinear with Creation (additive, under the gate). The
--    'Successful Dribbles' display wedge folds into it.
--  * Injuries STAYS — measured real, not noise (value -0.24/-0.28, reliability 0.36;
--    a distinct squad-availability signal).
-- Result: offense = Goals For · Creation · Shooting · Progression · Possession Lost ·
-- Injuries (6), symmetric with defense's 6. FOOTBALL team only; NBA/NFL untouched.
-- Apply with: ./sql/migrate.sh
-- ============================================================================

BEGIN;

CREATE OR REPLACE FUNCTION public.rating_datapoints_team(p_sport text, p_stats jsonb)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
 LANGUAGE sql
 IMMUTABLE PARALLEL SAFE
AS $function$
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
    SELECT * FROM (VALUES
        ('Goals For',            NULLIF(p_stats->>'goals_for','')::numeric,               TRUE,  TRUE,   1, 'offense'),
        ('Shooting',             NULLIF(p_stats->>'shots_on_target','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Creation',             NULLIF(p_stats->>'big_chances_created','')::numeric,      TRUE,  TRUE,   1, 'offense'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                TRUE,  FALSE, -1, 'offense'),
        ('Penalties Won',        NULLIF(p_stats->>'penalties_won','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Fouls Won',            NULLIF(p_stats->>'fouls_drawn','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Possession Lost',      NULLIF(p_stats->>'possession_lost','')::numeric,         TRUE,  FALSE, -1, 'offense'),
        ('Possession %',         NULLIF(p_stats->>'possession_pct','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Accurate Passes',      NULLIF(p_stats->>'accurate_passes','')::numeric,         FALSE, FALSE,  1, 'offense'),
        ('Progression',          COALESCE(NULLIF(p_stats->>'passes_final_third','')::numeric,0)
                               + COALESCE(NULLIF(p_stats->>'successful_dribbles','')::numeric,0),  TRUE, FALSE,  1, 'offense'),
        ('Tackling',             round(NULLIF(p_stats->>'tackles','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),                 TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',        round(NULLIF(p_stats->>'interceptions','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),           TRUE,  TRUE,   1, 'defense'),
        ('Clearances',           NULLIF(p_stats->>'clearances','')::numeric,              FALSE, FALSE,   1, 'defense'),
        ('SoT Allowed',          NULLIF(p_stats->>'shots_on_target_allowed','')::numeric, TRUE, FALSE, -1, 'defense'),
        ('Blocked Shots',        NULLIF(p_stats->>'blocked_shots','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Big Chances Allowed',  NULLIF(p_stats->>'big_chances_allowed','')::numeric,      TRUE, FALSE, -1, 'defense'),
        ('Goals Against', NULLIF(p_stats->>'goals_against','')::numeric, TRUE, FALSE, -1, 'defense'),
        ('Fouls Committed',      NULLIF(p_stats->>'fouls_committed','')::numeric,      FALSE, FALSE, -1, 'defense'),
        ('Cards',                COALESCE(NULLIF(p_stats->>'yellow_cards_total','')::numeric,0)
                               + COALESCE(NULLIF(p_stats->>'red_cards_total','')::numeric,0),   TRUE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;;

ALTER TABLE team_stats DISABLE TRIGGER trg_percentile_changed_team_stats;
DO $$
DECLARE s INTEGER;
BEGIN
    FOR s IN SELECT DISTINCT season FROM team_stats WHERE sport='FOOTBALL' AND rating_composite IS NOT NULL ORDER BY 1 LOOP
        PERFORM compute_team_rating('FOOTBALL', s);
    END LOOP;
END $$;
ALTER TABLE team_stats ENABLE TRIGGER trg_percentile_changed_team_stats;

DO $$
DECLARE v_cre INTEGER; v_prog INTEGER; v_off INTEGER;
BEGIN
    SELECT count(*) INTO v_cre FROM team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) el
      WHERE ts.sport='FOOTBALL' AND ts.season=2025 AND el->>'label'='Creation'
        AND (el->>'value')::numeric = (ts.stats->>'big_chances_created')::numeric AND (ts.stats->>'big_chances_created')::numeric>0;
    SELECT count(*) INTO v_prog FROM team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) el
      WHERE ts.sport='FOOTBALL' AND ts.season=2025 AND el->>'label'='Progression' AND (el->>'in_comp')::bool;
    SELECT count(DISTINCT el->>'label') INTO v_off FROM team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) el
      WHERE ts.sport='FOOTBALL' AND ts.season=2025 AND ts.team_id=18 AND (el->>'in_comp')::bool AND el->>'facet'='offense';
    IF v_cre=0 THEN RAISE EXCEPTION '074 FAIL: Creation not big_chances_created'; END IF;
    IF v_prog=0 THEN RAISE EXCEPTION '074 FAIL: Progression missing'; END IF;
    RAISE NOTICE '074 OK: Creation=Big Chances, +Progression; Chelsea offense has % composite wedges', v_off;
END $$;

COMMIT;

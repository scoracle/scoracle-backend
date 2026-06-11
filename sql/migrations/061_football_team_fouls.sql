-- ============================================================================
-- 061 — Football team: penalties_won -> display; add Fouls Won / Fouls Committed
--
-- Penalties Won was the noisiest composite signal (year-over-year repeatability 0.149
-- vs ~0.70 for shots-on-target / key-passes / big-chances), it triple-counted a won
-- penalty (already in Goals For + Shooting), and its swing tracked coaching/approach
-- more than quality (Chelsea's 11-penalty 2023 propped them ~8 ranks; Villa's fluky
-- 0-penalty 2025 deflated them ~5). No clean swap target exists in our data:
-- fouls_drawn has ~0 value (-0.10 vs goals; it's a style fingerprint, repeatability
-- 0.656), and shots_insidebox is a 0.89 duplicate of Shooting. So Penalties Won moves
-- to the DISPLAY tier (in_comp=FALSE) — still shown, out of the composite math.
--
-- And, for team-side parity with the player model (player 'Drawing Fouls' is a
-- composite datapoint at 0.44 vs goals+assists — it earns its spot per-player but
-- washes out to -0.10 at team level), add Fouls Won (offense) + Fouls Committed
-- (defense) as DISPLAY datapoints. They're replicable, distinct, characterful stats
-- but not quality signals, so they live with Possession % / Accurate Passes / Big
-- Chances Created in the display tier — visible on the profile, not in the rating.
-- (Fouls Committed sign -1 = fewer-is-cleaner for the displayed percentile; trivially
-- flippable — display-only.)
--
-- FOOTBALL team branch only. No API restart, no frontend change.
-- ============================================================================

BEGIN;

CREATE OR REPLACE FUNCTION public.rating_datapoints_team(p_sport text, p_stats jsonb)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
 LANGUAGE sql
 IMMUTABLE PARALLEL SAFE
AS $function$
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
        ('First Downs Allowed',NULLIF(p_stats->>'first_downs_allowed','')::numeric,        FALSE, FALSE, -1, 'defense'),
        ('Penalty Yards For',     NULLIF(p_stats->>'penalty_yards_drawn','')::numeric,     TRUE,  FALSE,  1, 'discipline'),
        ('Penalty Yards Against', NULLIF(p_stats->>'penalty_yards','')::numeric,           TRUE,  FALSE, -1, 'discipline')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NFL'

    UNION ALL
    -- FOOTBALL team — offense (attacking+possession) / defense. +SoT Allowed
    -- (composite -z). Margin (goal_difference) DROPPED. Cards/injuries = display.
    SELECT * FROM (VALUES
        ('Goals For',            NULLIF(p_stats->>'goals_for','')::numeric,               TRUE,  TRUE,   1, 'offense'),
        ('Shooting',             NULLIF(p_stats->>'shots_on_target','')::numeric,         TRUE,  TRUE,   1, 'offense'),
        ('Creation',             NULLIF(p_stats->>'key_passes','')::numeric,              TRUE,  TRUE,   1, 'offense'),
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
        ('Yellow Cards',         NULLIF(p_stats->>'yellow_cards_total','')::numeric,       FALSE, FALSE, -1, 'discipline'),
        ('Red Cards',            NULLIF(p_stats->>'red_cards_total','')::numeric,          FALSE, FALSE, -1, 'discipline'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                FALSE, FALSE, -1, 'squad')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;


DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT season FROM team_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM compute_team_rating('FOOTBALL', r.season);
    END LOOP;
END $$;

DO $$
DECLARE v_pw BIGINT; v_fw BIGINT; v_fc BIGINT;
BEGIN
    SELECT count(*) INTO v_pw FROM team_stats ts, jsonb_array_elements(ts.rating_breakdown) e
        WHERE ts.sport='FOOTBALL' AND e->>'label'='Penalties Won' AND (e->>'in_comp')::boolean;
    IF v_pw > 0 THEN RAISE EXCEPTION '061 gate FAIL: Penalties Won still in composite (% rows)', v_pw; END IF;
    SELECT count(*) INTO v_fw FROM team_stats ts, jsonb_array_elements(ts.rating_breakdown) e
        WHERE ts.sport='FOOTBALL' AND e->>'label'='Fouls Won';
    SELECT count(*) INTO v_fc FROM team_stats ts, jsonb_array_elements(ts.rating_breakdown) e
        WHERE ts.sport='FOOTBALL' AND e->>'label'='Fouls Committed';
    IF v_fw = 0 OR v_fc = 0 THEN RAISE EXCEPTION '061 gate FAIL: Fouls datapoints missing (won=%, committed=%)', v_fw, v_fc; END IF;
    RAISE NOTICE '061 OK: Penalties Won display-only; Fouls Won/Committed present (% / % rows)', v_fw, v_fc;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('061_football_team_fouls')
ON CONFLICT (version) DO NOTHING;

COMMIT;

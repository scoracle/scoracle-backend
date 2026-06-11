-- ============================================================================
-- 060 — Football team defense: outcome metrics + possession-adjusted volumes
--
-- The football team DEFENSE facet was measuring defensive *workload*, not defensive
-- *quality*. Two data-grounded problems, both fixed in rating_datapoints_team:
--
--   1. Raw defensive VOLUMES (tackles, interceptions, clearances) were z-scored into
--      the composite — but they correlate POSITIVELY with goals conceded (raw tackles
--      +0.18, interceptions +0.24, clearances +0.37 vs goals_against): a team racks
--      them up *because* it defends constantly, so they mildly REWARD bad defenses.
--      Fix — possession-adjust (PAdj): divide by opponent possession (= how much you
--      had to defend), the per-90 idea for defenders. opponent possession is measured
--      data and the league average is structurally 50% (possession is zero-sum), so
--      this is fully data-driven; the x50 constant washes out of the z-score entirely.
--      PAdj flips the outcome correlation to its correct sign (tackles -0.36,
--      interceptions -0.18). Applied INLINE (no materialization; recomputes each rating
--      run). Tackling + Interceptions -> PAdj.
--      Clearances PAdj only reaches ~0 vs outcome (neutral noise, not a quality signal),
--      so it is DROPPED from the composite/specialist (kept as a display datapoint).
--
--   2. The defensive OUTCOME metrics — Shots Allowed, Big Chances Allowed — were
--      display-only (in_comp=FALSE), and there was NO Goals Against datapoint at all
--      (asymmetric with Goals For on offense). So the composite rewarded interception
--      activity while ignoring what a team actually concedes. Fix — promote Shots
--      Allowed + Big Chances Allowed to the composite and ADD Goals Against (in_comp,
--      sign -1, defense). These are the no-estimation outcome truth.
--
-- Net effect (validated locally): suffocating sides rise (PSG, Arsenal, Man City),
-- ball-dominant-but-leaky sides fall (Chelsea 2025 90.5->86.3), and low-block teams
-- that genuinely concede a lot crater (Hellas Verona 2025 57.9->17.9). Only the
-- FOOTBALL team branch changes; NBA/NFL untouched. compute_team_rating updates rating
-- columns (not percentiles), so the notify trigger is not involved -> no API restart.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/060_football_team_defense.sql
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
        ('Penalties Won',        NULLIF(p_stats->>'penalties_won','')::numeric,           TRUE,  FALSE,  1, 'offense'),
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
        ('Yellow Cards',         NULLIF(p_stats->>'yellow_cards_total','')::numeric,       FALSE, FALSE, -1, 'discipline'),
        ('Red Cards',            NULLIF(p_stats->>'red_cards_total','')::numeric,          FALSE, FALSE, -1, 'discipline'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                FALSE, FALSE, -1, 'squad')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;


-- Recompute every football (team) season with the new defense datapoints.
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT season FROM team_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM compute_team_rating('FOOTBALL', r.season);
    END LOOP;
END $$;

-- Gates
DO $$
DECLARE v_ga BIGINT; v_clr BIGINT; v_padj BIGINT;
BEGIN
    -- Goals Against is now a composite defense datapoint
    SELECT count(*) INTO v_ga FROM team_stats ts, jsonb_array_elements(ts.rating_breakdown) e
        WHERE ts.sport='FOOTBALL' AND e->>'label'='Goals Against' AND (e->>'in_comp')::boolean;
    IF v_ga = 0 THEN RAISE EXCEPTION '060 gate FAIL: Goals Against not in composite'; END IF;

    -- Clearances is dropped from the composite (display-only now)
    SELECT count(*) INTO v_clr FROM team_stats ts, jsonb_array_elements(ts.rating_breakdown) e
        WHERE ts.sport='FOOTBALL' AND e->>'label'='Clearances' AND (e->>'in_comp')::boolean;
    IF v_clr > 0 THEN RAISE EXCEPTION '060 gate FAIL: Clearances still in composite (% rows)', v_clr; END IF;

    -- PAdj applied: for a high-possession team (opp<45), the Tackling datapoint value
    -- exceeds its raw tackle count (scaled up for low defensive load).
    SELECT count(*) INTO v_padj FROM team_stats ts, jsonb_array_elements(ts.rating_breakdown) e
        WHERE ts.sport='FOOTBALL' AND e->>'label'='Tackling'
          AND (ts.stats->>'opp_possession_pct')::numeric < 45
          AND (e->>'value')::numeric > (ts.stats->>'tackles')::numeric;
    IF v_padj = 0 THEN RAISE EXCEPTION '060 gate FAIL: PAdj not applied to Tackling'; END IF;

    RAISE NOTICE '060 OK: Goals Against in composite, Clearances display-only, PAdj applied to % high-possession Tackling rows', v_padj;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('060_football_team_defense')
ON CONFLICT (version) DO NOTHING;

COMMIT;

-- 249: Progression emits absence, not zero.
--
-- The team card's z-pizza showed "Progression 0" on FPL-era clubs: the label's
-- COALESCE(passes_final_third,0) + COALESCE(successful_dribbles,0) manufactures
-- a dense 0 when BOTH vendor keys are era-absent, so the dead-label filter
-- (pop mean IS NULL, mig 247) never fires and every club ties at a zero wedge.
-- Presence-guarded now: no source key, no datapoint. Vendor rows carry the
-- keys, so 2019-2025 recomputes are unchanged.
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
        ('Points Allowed',     NULLIF(p_stats->>'pts_allowed','')::numeric, TRUE,  FALSE, -1, 'defense'),
        ('Defensive Rebounds', NULLIF(p_stats->>'dreb','')::numeric,        FALSE, FALSE,  1, 'defense'),
        ('Opp FG%',            NULLIF(p_stats->>'def_fg_pct','')::numeric,  FALSE, FALSE, -1, 'defense'),
        ('Opp 3PT%',           NULLIF(p_stats->>'def_fg3_pct','')::numeric, FALSE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'NBA'
    UNION ALL
    SELECT * FROM (VALUES
        ('Points Scored',      NULLIF(p_stats->>'points_for','')::numeric,                 TRUE,  TRUE,   1, 'offense'),
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
        ('Points Allowed',     NULLIF(p_stats->>'points_against','')::numeric,             TRUE,  FALSE, -1, 'defense'),
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
        -- Creation: big chances in the vendor era; the FPL era sums squad assists
        -- (zero-suppressed, so an FPL-shaped row 0-fills; a vendor row without
        -- big chances stays NULL for the dead-label filter).
        ('Creation',             COALESCE(NULLIF(p_stats->>'big_chances_created','')::numeric,
                                          CASE WHEN p_stats ? 'expected_goals_for'
                                               THEN COALESCE(NULLIF(p_stats->>'assists','')::numeric, 0) END),
                                                                                           TRUE,  TRUE,   1, 'offense'),
        ('xG For',               NULLIF(p_stats->>'expected_goals_for','')::numeric,       TRUE,  TRUE,   1, 'offense'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                TRUE,  FALSE, -1, 'offense'),
        ('Penalties Won',        NULLIF(p_stats->>'penalties_won','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Fouls Won',            NULLIF(p_stats->>'fouls_drawn','')::numeric,              FALSE, FALSE,  1, 'offense'),
        ('Possession Lost',      NULLIF(p_stats->>'possession_lost','')::numeric,         TRUE,  FALSE, -1, 'offense'),
        ('Possession %',         NULLIF(p_stats->>'possession_pct','')::numeric,          FALSE, FALSE,  1, 'offense'),
        ('Accurate Passes',      NULLIF(p_stats->>'accurate_passes','')::numeric,         FALSE, FALSE,  1, 'offense'),
        -- Progression: vendor-only sources — no key, no datapoint (a dense 0
        -- here survives the dead-label filter and draws a zero wedge).
        ('Progression',          CASE WHEN p_stats ? 'passes_final_third' OR p_stats ? 'successful_dribbles'
                                      THEN COALESCE(NULLIF(p_stats->>'passes_final_third','')::numeric,0)
                                         + COALESCE(NULLIF(p_stats->>'successful_dribbles','')::numeric,0)
                                 END,                                                      TRUE, FALSE,  1, 'offense'),
        ('Tackling',             round(NULLIF(p_stats->>'tackles','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),                 TRUE,  TRUE,   1, 'defense'),
        ('Interceptions',        round(NULLIF(p_stats->>'interceptions','')::numeric * 50.0 / GREATEST(NULLIF(p_stats->>'opp_possession_pct','')::numeric, 30)),           TRUE,  TRUE,   1, 'defense'),
        ('Clearances',           NULLIF(p_stats->>'clearances','')::numeric,              FALSE, FALSE,   1, 'defense'),
        ('SoT Allowed',          NULLIF(p_stats->>'shots_on_target_allowed','')::numeric, TRUE, FALSE, -1, 'defense'),
        ('Blocked Shots',        NULLIF(p_stats->>'blocked_shots','')::numeric,           FALSE, FALSE,  1, 'defense'),
        ('Big Chances Allowed',  NULLIF(p_stats->>'big_chances_allowed','')::numeric,      TRUE, FALSE, -1, 'defense'),
        ('Goals Against',        NULLIF(p_stats->>'goals_against','')::numeric,           TRUE, FALSE, -1, 'defense'),
        ('xG Against',           NULLIF(p_stats->>'expected_goals_against','')::numeric,   TRUE, FALSE, -1, 'defense'),
        ('Clean Sheets',         CASE WHEN p_stats ? 'expected_goals_for'
                                      THEN COALESCE(NULLIF(p_stats->>'clean_sheets','')::numeric, 0) END,
                                                                                           TRUE, TRUE,   1, 'defense'),
        ('Fouls Committed',      NULLIF(p_stats->>'fouls_committed','')::numeric,          FALSE, FALSE, -1, 'defense'),
        -- Cards: zero-suppressed in the FPL era, so absence on an FPL-shaped
        -- row means a genuinely spotless side — 0-fill it; only a row from an
        -- era that never captured cards stays NULL for the dead-label filter.
        ('Cards',                CASE WHEN p_stats ? 'expected_goals_for' OR p_stats ? 'yellow_cards_total' OR p_stats ? 'red_cards_total'
                                      THEN COALESCE(NULLIF(p_stats->>'yellow_cards_total','')::numeric,0)
                                         + COALESCE(NULLIF(p_stats->>'red_cards_total','')::numeric,0)
                                 END,                                                      TRUE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;

-- self-recording (the file manages its own transaction; the runner's INSERT
-- is the idempotent backstop)
INSERT INTO schema_migrations (version) VALUES ('249_progression_absence') ON CONFLICT DO NOTHING;

COMMIT;

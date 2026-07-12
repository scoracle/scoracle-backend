-- 145_nba_team_points_allowed_zscore.sql
--
-- Add Points Allowed to the NBA team z-score defense facet.
--
--   * The NBA team composite measured defense only through Rim Protection (blk),
--     Steals (stl) and Rebounding (reb) — process proxies, not the outcome that
--     actually defines a good defense: how many points it gives up. NFL (125) and
--     FOOTBALL already fold their scoring-allowed metric into the composite; NBA was
--     the odd one out, so 'pts_allowed' was populated on team_stats and shown on the
--     defense pizza (056) yet never contributed to the rating.
--   * Points Allowed feeds defense with an inverse sign (fewer points = better).
--     It is composite-only (in_spec FALSE) because team specialist selection ranks
--     raw z, and an inverse metric would otherwise reward allowing MORE points — the
--     same treatment NFL Points Allowed / Yards Allowed get.
--
-- Derived from the current prod definition (125). NFL and FOOTBALL blocks are
-- reproduced verbatim; only the NBA block gains the 'Points Allowed' row.

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
        ('Creation',             NULLIF(p_stats->>'big_chances_created','')::numeric,      TRUE,  TRUE,   1, 'offense'),
        ('Injuries',             NULLIF(p_stats->>'injuries','')::numeric,                TRUE,  FALSE, -1, 'offense'),
        ('Penalties Won',        NULLIF(p_stats->>'penalties_won','')::numeric,           FALSE, FALSE,  1, 'offense'),
        ('Fouls Won',            NULLIF(p_stats->>'fouls_drawn','')::numeric,              FALSE, FALSE,  1, 'offense'),
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
        ('Goals Against',        NULLIF(p_stats->>'goals_against','')::numeric,           TRUE, FALSE, -1, 'defense'),
        ('Fouls Committed',      NULLIF(p_stats->>'fouls_committed','')::numeric,          FALSE, FALSE, -1, 'defense'),
        ('Cards',                COALESCE(NULLIF(p_stats->>'yellow_cards_total','')::numeric,0)
                               + COALESCE(NULLIF(p_stats->>'red_cards_total','')::numeric,0),   TRUE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;

-- Recompute NBA team ratings so composites/breakdowns/category sub-scores pick up the
-- new defense term. Guard the percentile-change trigger the same way 125 did for NFL.
ALTER TABLE team_stats DISABLE TRIGGER trg_percentile_changed_team_stats;
DO $$
DECLARE s INTEGER;
BEGIN
    FOR s IN SELECT DISTINCT season FROM team_stats WHERE sport='NBA' AND rating_composite IS NOT NULL ORDER BY 1 LOOP
        PERFORM compute_team_rating('NBA', s);
    END LOOP;
END $$;
ALTER TABLE team_stats ENABLE TRIGGER trg_percentile_changed_team_stats;

-- Parity gate: the NBA Points Allowed datapoint must be present, composite-only, and
-- inverse-signed.
DO $$
DECLARE
    v_pts_allowed INTEGER;
BEGIN
    SELECT count(*) INTO v_pts_allowed
    FROM rating_datapoints_team('NBA', '{"pts_allowed": 108}'::jsonb)
    WHERE label='Points Allowed' AND value=108 AND in_comp AND NOT in_spec AND sign=-1 AND facet='defense';

    IF v_pts_allowed <> 1 THEN
        RAISE EXCEPTION '145 FAIL: NBA Points Allowed datapoint missing or malformed';
    END IF;
    RAISE NOTICE '145 OK: NBA team Points Allowed added to defense z-score datapoints';
END $$;

INSERT INTO public.schema_migrations(version)
VALUES ('145_nba_team_points_allowed_zscore')
ON CONFLICT (version) DO NOTHING;

COMMIT;

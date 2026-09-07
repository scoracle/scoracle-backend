-- 247: the z-score model learns the FPL-era vocabulary.
--
-- Scott, 2026-09-07: "likely change up our z-score model to work with the new
-- format." The FOOTBALL arm of rating_datapoints spoke pure vendor keys —
-- shots_on_target, passes_accurate, key_passes, dribbles_success — almost none
-- of which the FPL feed provides, so a 2026 composite would have rated players
-- on goals, assists, tackles and a wall of neutral zeros.
--
-- Design rules:
--   * Dual-era by COALESCE(vendor, fpl). A season's rows are era-homogeneous,
--     so every row in a cohort resolves the same branch and the per-label
--     z-distribution stays internally consistent. 2025 recomputes byte-alike;
--     2026 rates on xG/xA/DC.
--   * Vendor-only labels stay (neutral zeros in the FPL era, alive on any 2025
--     recompute); FPL-only labels are neutral in the vendor era. Absence z=0.
--   * Discipline joins the composite — cards were captured in both eras but
--     never rated.
--   * The FPL feed zero-suppresses counting stats (a 0-goal defender has no
--     goals key), so counting inputs are 0-filled here at the rating layer —
--     absence would otherwise z-score as "at the mean" and rank non-scorers
--     mid-table in Goalscoring. Season rows stay sparse; only the z-model
--     densifies.
--   * Era-dead labels (no value anywhere in the cohort) are dropped in the z
--     step (pop mean IS NULL) — an all-tie label percent_ranks everyone to 0
--     and reads as a fabricated liability on the card.
--   * The appearance gate self-scales: LEAST(configured, ceil(half the cohort
--     max)). Three games in, 2 appearances ranks you; by matchweek 20 the gate
--     has tightened to the configured 10. No seasonal tending.
BEGIN;

-- Rate-mode support for the new composite inputs (the derive machinery reads
-- is_derived definitions; absence = the key never gets a per_90/per_game twin).
INSERT INTO stat_definitions
    (sport, key_name, display_name, entity_type, category, is_inverse, is_derived,
     is_percentile_eligible, sort_order, unit, comparable)
VALUES
    ('FOOTBALL', 'expected_assists_per_90',        'xA Per 90',                  'player', 'passing',   false, true, false, 210, 'per_game_avg', true),
    ('FOOTBALL', 'expected_assists_per_game',      'xA Per Game',                'player', 'passing',   false, true, true,  310, '',             false),
    ('FOOTBALL', 'defensive_contribution_per_90',  'Defensive Contrib. Per 90',  'player', 'defensive', false, true, false, 211, 'per_game_avg', true),
    ('FOOTBALL', 'defensive_contribution_per_game','Defensive Contrib. Per Game','player', 'defensive', false, true, true,  311, '',             false),
    ('FOOTBALL', 'cbi_per_90',                     'CBI Per 90',                 'player', 'defensive', false, true, false, 212, 'per_game_avg', true),
    ('FOOTBALL', 'cbi_per_game',                   'CBI Per Game',               'player', 'defensive', false, true, true,  312, '',             false)
ON CONFLICT DO NOTHING;

CREATE OR REPLACE FUNCTION public.rating_datapoints(p_sport text, p_stats jsonb, p_rate_mode text DEFAULT 'total'::text, p_position text DEFAULT NULL::text)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
 LANGUAGE sql
 STABLE PARALLEL SAFE
AS $function$
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'NBA' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all', 'pts'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all', 'reb'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all', 'ast'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all', 'stl'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all', 'blk'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all', 'fg3m'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all', NULL),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all', 'tov'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all', 'pf'),
        ('Foul Drawing',    NULLIF(p_stats->>'fta','')::numeric,        FALSE, TRUE,  1, 'all', 'fta')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NBA'

    UNION ALL
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'FOOTBALL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Goalscoring',     COALESCE(NULLIF(p_stats->>'goals','')::numeric, 0), TRUE, TRUE, 1, 'all', 'goals',           'out'),
        ('Creation',        COALESCE(NULLIF(p_stats->>'assists','')::numeric, 0), TRUE, TRUE, 1, 'all', 'assists',       'out'),
        -- Shooting: shots-on-target in the vendor era, xG in the FPL era
        -- (rate-aware inline; the suffix is '' in total mode so the raw keys
        -- resolve — rate_base NULL keeps the outer CASE off).
        ('Shooting',        COALESCE(
                                NULLIF(p_stats->>('shots_on_target' || COALESCE(rs.suffix,'')),'')::numeric,
                                NULLIF(p_stats->>('expected_goals'  || COALESCE(rs.suffix,'')),'')::numeric,
                                NULLIF(p_stats->>'shots_on_target','')::numeric,
                                NULLIF(p_stats->>'expected_goals','')::numeric, 0),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate', 'out'),
        -- Chance Creation: key passes in the vendor era, xA in the FPL era.
        ('Chance Creation', COALESCE(
                                NULLIF(p_stats->>('key_passes'       || COALESCE(rs.suffix,'')),'')::numeric,
                                NULLIF(p_stats->>('expected_assists' || COALESCE(rs.suffix,'')),'')::numeric,
                                NULLIF(p_stats->>'key_passes','')::numeric,
                                NULLIF(p_stats->>'expected_assists','')::numeric, 0),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success','out'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        FALSE, FALSE, 1, 'all', 'duels_won',       'out'),
        ('Tackling',        round(COALESCE(NULLIF(p_stats->>('tackles' || COALESCE(rs.suffix,'')),'')::numeric,
                                           NULLIF(p_stats->>'tackles','')::numeric, 0)
                                  * 50.0 / GREATEST(NULLIF(p_stats->>'team_opp_possession','')::numeric, 30), 2),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Interceptions',   round(COALESCE(NULLIF(p_stats->>('interceptions' || COALESCE(rs.suffix,'')),'')::numeric,
                                           NULLIF(p_stats->>'interceptions','')::numeric)
                                  * 50.0 / GREATEST(NULLIF(p_stats->>'team_opp_possession','')::numeric, 30), 2),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        -- Defensive Work: FPL's own tackles+CBI+recoveries threshold composite.
        -- FPL-era-only key, but zero-suppressed within that era, so it 0-fills
        -- only when the row is demonstrably FPL-shaped (carries appearances +
        -- an FPL-only sibling key) — a bare NULL would kill the dead-label
        -- filter for the vendor era.
        ('Defensive Work',  CASE WHEN p_stats ? 'expected_goals_conceded' OR p_stats ? 'defensive_contribution'
                                 THEN COALESCE(NULLIF(p_stats->>'defensive_contribution','')::numeric, 0) END,
                                                                              TRUE, TRUE,   1, 'all', 'defensive_contribution', 'out'),
        ('CBI',             CASE WHEN p_stats ? 'expected_goals_conceded' OR p_stats ? 'cbi'
                                 THEN COALESCE(NULLIF(p_stats->>'cbi','')::numeric, 0) END,
                                                                              FALSE, TRUE,  1, 'all', 'cbi',             'out'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       FALSE, FALSE, 1, 'all', 'clearances',      'out'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           FALSE, FALSE, 1, 'all', 'blocks',          'out'),
        ('Ball Recovery',   COALESCE(NULLIF(p_stats->>'ball_recovery','')::numeric, 0), FALSE, FALSE, 1, 'all', 'ball_recovery', 'out'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      FALSE, FALSE, 1, 'all', 'fouls_drawn',     'out'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all', NULL,              'out'),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all', 'possession_lost','out'),
        -- Cards were captured in both eras but never rated; zero-suppressed
        -- keys mean absence IS zero here, so COALESCE(,0) is the truth.
        ('Discipline',      COALESCE(NULLIF(p_stats->>'yellow_cards','')::numeric, 0)
                          + 3 * COALESCE(NULLIF(p_stats->>'red_cards','')::numeric, 0),
                                                                              TRUE, FALSE, -1, 'all', NULL,              'out'),
        ('Shot-Stopping',   COALESCE(NULLIF(p_stats->>'saves','')::numeric, 0), FALSE, FALSE, 1, 'all', 'saves',         'gk'),
        -- Goals Prevented: xGC minus conceded when the FPL feed carries xGC
        -- (goals_conceded is zero-suppressed, hence COALESCE inside); the
        -- vendor league-average-save-pct formula otherwise.
        ('Goals Prevented', CASE WHEN p_stats ? 'expected_goals_conceded'
                                 THEN NULLIF(p_stats->>'expected_goals_conceded','')::numeric
                                      - COALESCE(NULLIF(p_stats->>'goals_conceded','')::numeric, 0)
                                 ELSE NULLIF(p_stats->>'saves','')::numeric
                                      - (COALESCE(NULLIF(p_stats->>'saves','')::numeric,0) + COALESCE(NULLIF(p_stats->>'goals_conceded','')::numeric,0))
                                        * NULLIF(p_stats->>'league_avg_save_pct','')::numeric / 100.0
                            END,
                                                                              TRUE, TRUE, 1, 'all', NULL, 'gk'),
        ('Clean Sheets',       CASE WHEN p_stats ? 'expected_goals_conceded' OR p_stats ? 'clean_sheets'
                                    THEN COALESCE(NULLIF(p_stats->>'clean_sheets','')::numeric, 0) END,
                                                                                   TRUE, TRUE, 1, 'all', NULL, 'gk'),
        ('Penalty Saves',      CASE WHEN p_stats ? 'expected_goals_conceded' OR p_stats ? 'penalties_saved'
                                    THEN COALESCE(NULLIF(p_stats->>'penalties_saved','')::numeric, 0) END,
                                                                                   FALSE, TRUE, 1, 'all', NULL, 'gk'),
        ('Distribution',       NULLIF(p_stats->>'pass_accuracy','')::numeric,      TRUE, TRUE, 1, 'all', NULL, 'gk'),
        ('Long-Ball Accuracy', NULLIF(p_stats->>'long_ball_accuracy','')::numeric, TRUE, TRUE, 1, 'all', NULL, 'gk')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base, pos_class)
    WHERE p_sport = 'FOOTBALL'
      AND (CASE WHEN p_position = 'Goalkeeper' THEN v.pos_class = 'gk'
                ELSE v.pos_class = 'out' END)

    UNION ALL
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'NFL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Air Yards Responsible',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_yards')::numeric,0)
                + COALESCE((p_stats->>'interception_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_yards' || rs.suffix))::numeric,(p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>('receiving_yards' || rs.suffix))::numeric,(p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>('kick_return_yards' || rs.suffix))::numeric,(p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_returner_return_yards' || rs.suffix))::numeric,(p_stats->>'punt_returner_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_yards' || rs.suffix))::numeric,(p_stats->>'punt_yards')::numeric,0)
                + COALESCE((p_stats->>('interception_yards' || rs.suffix))::numeric,(p_stats->>'interception_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Ground Yards Responsible',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'rushing_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('rushing_yards' || rs.suffix))::numeric,(p_stats->>'rushing_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', 'rushing_yards'),
        ('Points Responsible For',
            CASE WHEN p_rate_mode = 'total' THEN
                  6 * (
                      COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'interception_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>'fumbles_touchdowns')::numeric,0)
                  )
                + 3 * COALESCE((p_stats->>'field_goals_made')::numeric,0)
                + COALESCE((p_stats->>'extra_points_made')::numeric,0)
            ELSE
                  6 * (
                      COALESCE((p_stats->>('passing_touchdowns' || rs.suffix))::numeric,(p_stats->>'passing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('rushing_touchdowns' || rs.suffix))::numeric,(p_stats->>'rushing_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('receiving_touchdowns' || rs.suffix))::numeric,(p_stats->>'receiving_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('kick_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'kick_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('punt_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'punt_return_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('interception_touchdowns' || rs.suffix))::numeric,(p_stats->>'interception_touchdowns')::numeric,0)
                    + COALESCE((p_stats->>('fumbles_touchdowns' || rs.suffix))::numeric,(p_stats->>'fumbles_touchdowns')::numeric,0)
                  )
                + 3 * COALESCE((p_stats->>('field_goals_made' || rs.suffix))::numeric,(p_stats->>'field_goals_made')::numeric,0)
                + COALESCE((p_stats->>('extra_points_made' || rs.suffix))::numeric,(p_stats->>'extra_points_made')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_interceptions' || rs.suffix))::numeric,(p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>('fumbles_lost' || rs.suffix))::numeric,(p_stats->>'fumbles_lost')::numeric,0)
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,       TRUE, TRUE,   1, 'defense', 'total_tackles'),
        ('Tackles For Loss',
            CASE WHEN p_rate_mode = 'total' THEN
                  GREATEST(
                      COALESCE((p_stats->>'tackles_for_loss')::numeric,0),
                      COALESCE((p_stats->>'defensive_sacks')::numeric,0)
                  )
            ELSE
                  GREATEST(
                      COALESCE((p_stats->>('tackles_for_loss' || rs.suffix))::numeric,(p_stats->>'tackles_for_loss')::numeric,0),
                      COALESCE((p_stats->>('defensive_sacks' || rs.suffix))::numeric,(p_stats->>'defensive_sacks')::numeric,0)
                  )
            END,                                                                  TRUE, TRUE,   1, 'defense', NULL),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense', 'defensive_interceptions')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NFL';
$function$;

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
        ('Progression',          COALESCE(NULLIF(p_stats->>'passes_final_third','')::numeric,0)
                               + COALESCE(NULLIF(p_stats->>'successful_dribbles','')::numeric,0),  TRUE, FALSE,  1, 'offense'),
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
        ('Cards',                COALESCE(NULLIF(p_stats->>'yellow_cards_total','')::numeric,0)
                               + COALESCE(NULLIF(p_stats->>'red_cards_total','')::numeric,0),   TRUE, FALSE, -1, 'defense')
    ) v(label, value, in_comp, in_spec, sign, facet) WHERE p_sport = 'FOOTBALL';
$function$;

CREATE OR REPLACE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, composite_score numeric, breakdown jsonb, scoped_ranks jsonb, scoped_scores jsonb)
 LANGUAGE sql
 STABLE
AS $function$
    WITH lasp AS (
        SELECT CASE WHEN p_sport='FOOTBALL'
                    THEN round(avg(NULLIF(stats->>'save_pct','')::numeric), 4) END AS asp
        FROM player_stats
        WHERE sport='FOOTBALL' AND season=p_season AND position='Goalkeeper'
          AND (stats->>'appearances')::numeric >= 15
    ),
    -- The gate self-scales with the season's progress: half the cohort max,
    -- never below 1, capped at the configured floor. Early season rates on
    -- thin evidence rather than not at all; the configured value takes over
    -- once the cohort has 2x that many appearances.
    eff AS (
        SELECT rt.stat_key,
               LEAST(rt.min_value, GREATEST(1, ceil(0.5 * COALESCE((
                   SELECT MAX(NULLIF(ps2.stats->>rt.stat_key,'')::numeric)
                   FROM player_stats ps2
                   WHERE ps2.sport = p_sport AND ps2.season = p_season), 0)))) AS min_value
        FROM public.rating_thresholds rt
        WHERE rt.sport = p_sport
    ),
    dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
               tm.conference, tm.division,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet,
               -- Phase 2: TAG eligibility instead of filtering it out. Sub-gate players
               -- still produce datapoints (for their breakdown); pop/ranks/scoped below
               -- use `WHERE is_ranked` so the rated cohort is unchanged.
               COALESCE((
                   SELECT bool_and(COALESCE((ps.stats->>e.stat_key)::numeric, 0) >= e.min_value)
                   FROM eff e
                 ), FALSE) AS is_ranked
        FROM player_stats ps
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        LEFT JOIN LATERAL (
            SELECT tts.stats->>'opp_possession_pct' AS opp
            FROM team_stats tts
            WHERE tts.team_id = ps.team_id AND tts.sport = p_sport AND tts.season = p_season
            LIMIT 1
        ) topp ON p_sport = 'FOOTBALL'
        CROSS JOIN lasp
        CROSS JOIN LATERAL rating_datapoints(
            p_sport,
            CASE WHEN p_sport = 'FOOTBALL'
                 THEN ps.stats || jsonb_strip_nulls(jsonb_build_object(
                          'team_opp_possession', topp.opp,
                          'league_avg_save_pct', lasp.asp))
                 ELSE ps.stats END,
            p_rate_mode, ps.position) d
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp WHERE is_ranked GROUP BY label
    ),
    z AS (
        -- p.mean IS NULL = no ranked row carries the label this season (an
        -- era-dead key). Dropped: an all-tie label percent_ranks everyone to
        -- 0 and reads as a fabricated liability. Dropped rows contributed
        -- zr=0, so composites are unchanged.
        SELECT d.player_id, d.league_id, d.position, d.conference, d.division,
               d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value, d.is_ranked,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM dp d JOIN pop p USING (label)
        WHERE p.mean IS NOT NULL
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
    comp AS (
        SELECT player_id, league_id, composite FROM comp_flat
    ),
    rk AS (
        SELECT DISTINCT player_id, league_id, is_ranked FROM z
    ),
    scored AS (
        -- Rated cohort: percent_rank over the rated set only → byte-identical to before.
        SELECT player_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_position,
               CASE WHEN p_sport IN ('NFL','NBA') AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, conference ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_conference,
               CASE WHEN p_sport='NFL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, division ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_division,
               CASE WHEN p_sport='FOOTBALL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, league_id ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_league
        FROM z WHERE is_ranked
        UNION ALL
        -- Sub-gate players: per-stat percentile = standing within the RATED cohort for
        -- that stat (count-based, so it doesn't perturb the cohort's own percent_rank).
        -- Scope cuts omitted (they are unranked).
        SELECT u.player_id, u.league_id, u.label, u.in_comp, u.in_spec, u.sign, u.facet, u.value, u.zr,
               -- Sub-gate fill = the datapoint's standardized magnitude vs the rated
               -- cohort (50 + 10*z in the good direction, clamped 1-99) — the same scale
               -- as rating_score. Fast scalar; a true percentile-vs-cohort is O(n^2).
               ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (u.sign * u.zr)))::numeric, 1) AS pct,
               NULL::numeric AS pct_position, NULL::numeric AS pct_conference,
               NULL::numeric AS pct_division, NULL::numeric AS pct_league
        FROM z u WHERE NOT u.is_ranked
    ),
    bd AS (
        -- (mig 221) the specialty flag leaves the datapoint with the concept. A client
        -- that ever wants a hero row can take max(z); the Scout names standouts in prose.
        -- (Spelling the retired key out here would trip this migration's own proof gate.)
        SELECT s.player_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet,
                   'scoped_pct', jsonb_strip_nulls(jsonb_build_object(
                       'position', s.pct_position, 'conference', s.pct_conference,
                       'division', s.pct_division, 'league', s.pct_league))
               ) ORDER BY s.label) AS breakdown
        FROM scored s
        GROUP BY s.player_id, s.league_id
    ),
    base AS (
        -- (mig 221) the specialist CTE was an INNER join here; without it an entity is
        -- rated on its composite alone, which is what the rating always was.
        SELECT c.player_id, c.league_id,
               ROUND(c.composite, 4) AS composite,
               bd.breakdown, rk.is_ranked
        FROM comp c
        JOIN bd USING (player_id, league_id)
        JOIN rk USING (player_id, league_id)
    ),
    ranks AS (
        SELECT player_id, league_id, is_ranked,
               CASE WHEN is_ranked THEN ROUND((percent_rank() OVER (PARTITION BY is_ranked ORDER BY composite ASC))::numeric * 100, 1) END AS composite_rank,
               CASE WHEN is_ranked THEN public.rating_score(composite, AVG(composite) OVER(PARTITION BY is_ranked), STDDEV_POP(composite) OVER(PARTITION BY is_ranked)) END AS composite_score
        FROM base
    ),
    scoped AS (
        SELECT b.player_id, b.league_id,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position ORDER BY b.composite ASC))::numeric*100,1) END AS pos_pct,
               CASE WHEN p_sport IN ('NFL','NBA') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.conference ORDER BY b.composite ASC))::numeric*100,1) END AS conf_pct,
               CASE WHEN p_sport='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.division ORDER BY b.composite ASC))::numeric*100,1) END AS div_pct,
               CASE WHEN p_sport='FOOTBALL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, ps.league_id ORDER BY b.composite ASC))::numeric*100,1) END AS league_pct,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position)) END AS pos_score,
               CASE WHEN p_sport IN ('NFL','NBA') THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.conference), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.conference)) END AS conf_score,
               CASE WHEN p_sport='NFL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.division), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.division)) END AS div_score,
               CASE WHEN p_sport='FOOTBALL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, ps.league_id), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, ps.league_id)) END AS league_score
        FROM base b
        JOIN player_stats ps
          ON ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
         AND COALESCE(ps.league_id, 0) = b.league_id
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        WHERE ps.position IS NOT NULL AND b.is_ranked
    )
    SELECT b.player_id, b.league_id,
           CASE WHEN b.is_ranked THEN b.composite END AS composite,
           r.composite_rank, r.composite_score,
           b.breakdown,
           NULLIF(jsonb_strip_nulls(jsonb_build_object(
               'position', sc.pos_pct, 'conference', sc.conf_pct,
               'division', sc.div_pct, 'league', sc.league_pct)), '{}'::jsonb) AS scoped_ranks,
           NULLIF(jsonb_strip_nulls(jsonb_build_object(
               'position', sc.pos_score, 'conference', sc.conf_score,
               'division', sc.div_score, 'league', sc.league_score)), '{}'::jsonb) AS scoped_scores
    FROM base b
    JOIN ranks r USING (player_id, league_id)
    LEFT JOIN scoped sc USING (player_id, league_id);
$function$;

CREATE OR REPLACE FUNCTION public.compute_team_rating(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating = NULL, rating_rank = NULL, rating_score = NULL,
           rating_scoped_scores = NULL,
           rating_categories = NULL, rating_scoped_ranks = NULL, rating_breakdown = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating IS NOT NULL OR rating_rank IS NOT NULL);

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
        -- mean IS NULL = era-dead label; see _compute_rating_bundle.
        SELECT d.team_id, d.league_id, d.in_comp, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
        WHERE p.mean IS NOT NULL
    ),
    composite AS (
        SELECT team_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY team_id, league_id
    )
    UPDATE team_stats ts SET rating = ROUND(c.composite, 4)
    FROM composite c
    WHERE ts.team_id = c.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = c.league_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
        WHERE p.mean IS NOT NULL
    ),
    scored AS (
        SELECT team_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct
        FROM z
    ),
    agg AS (
        SELECT s.team_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet
               ) ORDER BY s.facet, s.label) AS breakdown
        FROM scored s
        GROUP BY s.team_id, s.league_id
    )
    UPDATE team_stats ts SET rating_breakdown = a.breakdown
    FROM agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id AND ts.rating IS NOT NULL;

    WITH r AS (
        SELECT team_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating ASC))::numeric * 100, 1) AS crank,
               public.rating_score(rating, AVG(rating) OVER(), STDDEV_POP(rating) OVER()) AS cscore
        FROM team_stats
        WHERE sport = p_sport AND season = p_season AND rating IS NOT NULL
    )
    UPDATE team_stats ts SET rating_rank = r.crank, rating_score = r.cscore
    FROM r
    WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = r.league_id;

    RETURN v_updated;
END;
$function$;

COMMIT;

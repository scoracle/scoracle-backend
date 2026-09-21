-- Name the actual source measurement behind every rated NBA/NFL label.
-- Migration 252 fixed FOOTBALL's derived formulas but the NBA and NFL player
-- branches (and the NBA team branch) still returned the display label itself as
-- `measure`. The bundle persisted `Rim Protection` as its own measurement, the
-- Scout adapter suppressed the duplicate label/measure text, and the card
-- received only the category name — inviting invented mechanics for a quantity
-- that is really blocks (Rim Protection), assists (Playmaking), turnovers
-- (Ball Security) and plus-minus (On-Court Impact).
--
-- This migration names each underlying quantity or formula explicitly, then
-- rebuilds every NBA/NFL player and team bundle inside this transaction. It
-- proves numerical parity in-database: rating, rank, score, scoped ranks and
-- scores, and every breakdown member EXCEPT the `measure` field must be
-- identical before and after. Percentile partitions are unchanged by
-- construction — every label still maps to exactly one measure.
BEGIN;
SET LOCAL statement_timeout = '30min';

-- ---------------------------------------------------------------------------
-- Parity capture: break down every affected stored bundle into
-- everything-but-measure so the rebuild can be compared member by member.
-- ---------------------------------------------------------------------------
CREATE TEMP TABLE _player_parity AS
SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
       rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
       (SELECT jsonb_agg(d - 'measure' ORDER BY ord)
          FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_measure,
       (SELECT jsonb_object_agg(m.key, jsonb_build_object(
                    'rating', m.value->'rating',
                    'rating_rank', m.value->'rating_rank',
                    'rating_score', m.value->'rating_score',
                    'breakdown', (SELECT jsonb_agg(d - 'measure' ORDER BY ord)
                                    FROM jsonb_array_elements(m.value->'breakdown')
                                      WITH ORDINALITY b(d, ord)),
                    'scoped_ranks', m.value->'scoped_ranks',
                    'scoped_scores', m.value->'scoped_scores'))
          FROM jsonb_each(rating_modes) m) AS modes_no_measure
FROM player_stats
WHERE sport IN ('NBA', 'NFL')
  AND (rating IS NOT NULL OR rating_breakdown IS NOT NULL OR rating_modes IS NOT NULL);

CREATE TEMP TABLE _team_parity AS
SELECT sport, season, team_id, COALESCE(league_id, 0) AS league_id,
       rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
       (SELECT jsonb_agg(d - 'measure' ORDER BY ord)
          FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_measure
FROM team_stats
WHERE sport IN ('NBA', 'NFL')
  AND (rating IS NOT NULL OR rating_breakdown IS NOT NULL);

-- ---------------------------------------------------------------------------
-- Same function, with source measurement identity on the NBA and NFL branches.
-- Values, polarity, comparable/specialty flags, facets, rate logic and the
-- FOOTBALL branches are byte-identical to migration 252.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.rating_measurements(p_sport text, p_stats jsonb, p_rate_mode text DEFAULT 'total'::text, p_position text DEFAULT NULL::text)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text, measure text)
 LANGUAGE sql
 STABLE PARALLEL SAFE
AS $function$
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric END,
           v.in_comp, v.in_spec, v.sign, v.facet,
           CASE v.label
             WHEN 'Scoring' THEN 'points'
             WHEN 'Rebounding' THEN 'total rebounds'
             WHEN 'Playmaking' THEN 'assists'
             WHEN 'Steals' THEN 'steals'
             WHEN 'Rim Protection' THEN 'blocks'
             WHEN '3PT Shooting' THEN 'three-point field goals made'
             WHEN 'On-Court Impact' THEN 'plus-minus'
             WHEN 'Ball Security' THEN 'turnovers'
             WHEN 'Discipline' THEN 'personal fouls'
             WHEN 'Foul Drawing' THEN 'field goals attempted'
             ELSE v.label END
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
      AND (p_rate_mode = 'total' OR (v.rate_base IS NOT NULL AND
           (p_stats->>(SELECT denom_key FROM public.rate_modes WHERE sport=p_sport AND mode=p_rate_mode))::numeric > 0))

    UNION ALL
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric END,
           v.in_comp, v.in_spec, v.sign, v.facet,
           CASE v.label
             WHEN 'Goalscoring' THEN 'goals'
             WHEN 'Creation' THEN 'assists'
             WHEN 'Discipline' THEN 'yellow cards + 3 x red cards'
             WHEN 'Passing' THEN 'accurate passes'
             WHEN 'Dribbling' THEN 'successful dribbles'
             WHEN 'Shooting' THEN CASE
               WHEN NULLIF(p_stats->>('shots_on_target' || COALESCE(rs.suffix,'')), '') IS NOT NULL THEN 'shots on target'
               WHEN NULLIF(p_stats->>('expected_goals' || COALESCE(rs.suffix,'')), '') IS NOT NULL THEN 'expected goals'
               ELSE NULL END
             WHEN 'Chance Creation' THEN CASE
               WHEN NULLIF(p_stats->>('key_passes' || COALESCE(rs.suffix,'')), '') IS NOT NULL THEN 'key passes'
               WHEN NULLIF(p_stats->>('expected_assists' || COALESCE(rs.suffix,'')), '') IS NOT NULL THEN 'expected assists'
               ELSE NULL END
             WHEN 'Goals Prevented' THEN CASE WHEN p_stats ? 'expected_goals_conceded'
               THEN 'expected goals conceded minus goals conceded' ELSE 'saves relative to league save percentage' END
             WHEN 'CBI' THEN 'clearances + blocks + interceptions (combined)'
             WHEN 'Defensive Work' THEN 'defensive contributions'
             WHEN 'Tackling' THEN 'possession-adjusted tackles'
             WHEN 'Interceptions' THEN 'possession-adjusted interceptions'
             ELSE v.label END
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
                                NULLIF(p_stats->>('expected_goals'  || COALESCE(rs.suffix,'')),'')::numeric),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate', 'out'),
        -- Chance Creation: key passes in the vendor era, xA in the FPL era.
        ('Chance Creation', COALESCE(
                                NULLIF(p_stats->>('key_passes'       || COALESCE(rs.suffix,'')),'')::numeric,
                                NULLIF(p_stats->>('expected_assists' || COALESCE(rs.suffix,'')),'')::numeric),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success','out'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        FALSE, FALSE, 1, 'all', 'duels_won',       'out'),
        ('Tackling',        round(NULLIF(p_stats->>('tackles' || COALESCE(rs.suffix,'')),'')::numeric
                                  * 50.0 / GREATEST(NULLIF(p_stats->>'team_opp_possession','')::numeric, 30), 2),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Interceptions',   round(NULLIF(p_stats->>('interceptions' || COALESCE(rs.suffix,'')),'')::numeric
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
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,   TRUE, FALSE, -1, 'all', 'possession_lost','out'),
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
      AND (p_rate_mode = 'total' OR ((v.rate_base IS NOT NULL OR v.label IN ('Shooting','Chance Creation','Tackling','Interceptions')) AND
           (p_stats->>(SELECT denom_key FROM public.rate_modes WHERE sport=p_sport AND mode=p_rate_mode))::numeric > 0))
      AND (CASE WHEN p_position = 'Goalkeeper' THEN v.pos_class = 'gk'
                ELSE v.pos_class = 'out' END)

    UNION ALL
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric END,
           v.in_comp, v.in_spec, v.sign, v.facet,
           CASE v.label
             WHEN 'Air Yards Responsible'
                  THEN 'sum of passing, receiving, kick-return, punt-return, punt and interception yards'
             WHEN 'Ground Yards Responsible' THEN 'rushing yards'
             WHEN 'Points Responsible For'
                  THEN '6 x touchdowns (passing, rushing, receiving, kick return, punt return, interception, fumble) + 3 x field goals made + extra points made'
             WHEN 'Giveaways' THEN 'passing interceptions + fumbles lost'
             WHEN 'Tackling' THEN 'total tackles'
             WHEN 'Tackles For Loss' THEN 'max of tackles for loss and defensive sacks'
             WHEN 'Interceptions' THEN 'defensive interceptions'
             ELSE v.label END
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
                  CASE WHEN NULLIF(p_stats->>'passing_yards','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('passing_yards' || rs.suffix),'')::numeric END
                + CASE WHEN NULLIF(p_stats->>'receiving_yards','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('receiving_yards' || rs.suffix),'')::numeric END
                + CASE WHEN NULLIF(p_stats->>'kick_return_yards','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('kick_return_yards' || rs.suffix),'')::numeric END
                + CASE WHEN NULLIF(p_stats->>'punt_returner_return_yards','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('punt_returner_return_yards' || rs.suffix),'')::numeric END
                + CASE WHEN NULLIF(p_stats->>'punt_yards','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('punt_yards' || rs.suffix),'')::numeric END
                + CASE WHEN NULLIF(p_stats->>'interception_yards','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('interception_yards' || rs.suffix),'')::numeric END
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Ground Yards Responsible',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'rushing_yards')::numeric,0)
            ELSE
                  CASE WHEN NULLIF(p_stats->>'rushing_yards','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('rushing_yards' || rs.suffix),'')::numeric END
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
                      CASE WHEN NULLIF(p_stats->>'passing_touchdowns','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('passing_touchdowns' || rs.suffix),'')::numeric END
                    + CASE WHEN NULLIF(p_stats->>'rushing_touchdowns','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('rushing_touchdowns' || rs.suffix),'')::numeric END
                    + CASE WHEN NULLIF(p_stats->>'receiving_touchdowns','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('receiving_touchdowns' || rs.suffix),'')::numeric END
                    + CASE WHEN NULLIF(p_stats->>'kick_return_touchdowns','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('kick_return_touchdowns' || rs.suffix),'')::numeric END
                    + CASE WHEN NULLIF(p_stats->>'punt_return_touchdowns','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('punt_return_touchdowns' || rs.suffix),'')::numeric END
                    + CASE WHEN NULLIF(p_stats->>'interception_touchdowns','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('interception_touchdowns' || rs.suffix),'')::numeric END
                    + CASE WHEN NULLIF(p_stats->>'fumbles_touchdowns','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('fumbles_touchdowns' || rs.suffix),'')::numeric END
                  )
                + 3 * CASE WHEN NULLIF(p_stats->>'field_goals_made','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('field_goals_made' || rs.suffix),'')::numeric END
                + CASE WHEN NULLIF(p_stats->>'extra_points_made','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('extra_points_made' || rs.suffix),'')::numeric END
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost')::numeric,0)
            ELSE
                  CASE WHEN NULLIF(p_stats->>'passing_interceptions','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('passing_interceptions' || rs.suffix),'')::numeric END
                + CASE WHEN NULLIF(p_stats->>'fumbles_lost','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('fumbles_lost' || rs.suffix),'')::numeric END
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,       TRUE, TRUE,   1, 'defense', 'total_tackles'),
        ('Tackles For Loss',
            CASE WHEN p_rate_mode = 'total' THEN
                  GREATEST(
                      COALESCE((p_stats->>'tackles_for_loss')::numeric,0),
                      COALESCE((p_stats->>'defensive_sacks')::numeric,0)
                  )
            ELSE
                CASE WHEN ((p_stats->>'tackles_for_loss') IS NOT NULL AND (p_stats->>('tackles_for_loss' || rs.suffix)) IS NULL)
                       OR ((p_stats->>'defensive_sacks') IS NOT NULL AND (p_stats->>('defensive_sacks' || rs.suffix)) IS NULL)
                     THEN NULL ELSE GREATEST(
                      CASE WHEN NULLIF(p_stats->>'tackles_for_loss','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('tackles_for_loss' || rs.suffix),'')::numeric END,
                      CASE WHEN NULLIF(p_stats->>'defensive_sacks','') IS NULL THEN 0 ELSE NULLIF(p_stats->>('defensive_sacks' || rs.suffix),'')::numeric END
                  ) END
            END,                                                                  TRUE, TRUE,   1, 'defense', NULL),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense', 'defensive_interceptions')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NFL'
      AND (p_rate_mode = 'total' OR
           (p_stats->>(SELECT denom_key FROM public.rate_modes WHERE sport=p_sport AND mode=p_rate_mode))::numeric > 0);
$function$;

CREATE OR REPLACE FUNCTION public.rating_datapoints(p_sport text, p_stats jsonb, p_rate_mode text DEFAULT 'total'::text, p_position text DEFAULT NULL::text)
RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
LANGUAGE sql STABLE PARALLEL SAFE AS $$
    SELECT label,value,in_comp,in_spec,sign,facet
    FROM public.rating_measurements(p_sport,p_stats,p_rate_mode,p_position);
$$;

CREATE OR REPLACE FUNCTION public.rating_measurements_team(p_sport text, p_stats jsonb)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text, measure text)
 LANGUAGE sql
 IMMUTABLE PARALLEL SAFE
AS $function$
    SELECT v.*, CASE v.label
        WHEN 'Scoring' THEN 'points'
        WHEN 'Playmaking' THEN 'assists'
        WHEN '3PT Shooting' THEN 'three-point field goals made'
        WHEN 'Foul Drawing' THEN 'field goals attempted'
        WHEN 'Ball Security' THEN 'turnovers'
        WHEN 'Offensive Rebounds' THEN 'offensive rebounds'
        WHEN 'Rim Protection' THEN 'blocks'
        WHEN 'Steals' THEN 'steals'
        WHEN 'Rebounding' THEN 'total rebounds'
        WHEN 'Points Allowed' THEN 'points allowed'
        WHEN 'Defensive Rebounds' THEN 'defensive rebounds'
        WHEN 'Opp FG%' THEN 'opponent field goal percentage'
        WHEN 'Opp 3PT%' THEN 'opponent three-point percentage'
        ELSE v.label END FROM (VALUES
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
    SELECT v.*, CASE v.label
        WHEN 'Giveaways' THEN 'turnovers'
        WHEN 'Touchdowns' THEN 'passing + rushing touchdowns'
        WHEN 'Field Goals' THEN 'field goals made'
        WHEN 'Penalty Yards For' THEN 'penalty yards drawn'
        WHEN 'Tackling' THEN 'total tackles'
        WHEN 'Sacks' THEN 'defensive sacks'
        WHEN 'Pass Defense' THEN 'passes defended'
        WHEN 'Interceptions' THEN 'defensive interceptions'
        ELSE v.label END FROM (VALUES
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
    SELECT v.*, CASE v.label
        WHEN 'Creation' THEN CASE WHEN NULLIF(p_stats->>'big_chances_created','') IS NOT NULL
            THEN 'big chances created' WHEN p_stats ? 'expected_goals_for' THEN 'assists' END
        WHEN 'Shooting' THEN 'shots on target'
        WHEN 'Tackling' THEN 'possession-adjusted tackles'
        WHEN 'Interceptions' THEN 'possession-adjusted interceptions'
        ELSE v.label END FROM (VALUES
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

CREATE OR REPLACE FUNCTION public.rating_datapoints_team(p_sport text, p_stats jsonb)
RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT label,value,in_comp,in_spec,sign,facet FROM public.rating_measurements_team(p_sport,p_stats);
$$;

-- ---------------------------------------------------------------------------
-- Rebuild every NBA/NFL bundle from authoritative statistics. The FOOTBALL
-- branches are byte-identical to 252, so football bundles are left untouched.
-- ---------------------------------------------------------------------------
DO $rebuild$
DECLARE r record;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM public.player_stats WHERE sport IN ('NBA','NFL') ORDER BY sport, season LOOP
        PERFORM public.compute_rating(r.sport, r.season);
    END LOOP;
    FOR r IN SELECT DISTINCT sport, season FROM public.team_stats WHERE sport IN ('NBA','NFL') ORDER BY sport, season LOOP
        PERFORM public.compute_team_rating(r.sport, r.season);
    END LOOP;
END $rebuild$;

-- ---------------------------------------------------------------------------
-- Parity proof: ratings, ranks, scores and scoped ranks/scores are unchanged,
-- and every breakdown member except `measure` is identical. A failure raises
-- and rolls the whole transaction back, leaving applied history untouched.
-- ---------------------------------------------------------------------------
DO $parity$
DECLARE mismatch bigint;
BEGIN
    WITH rebuilt AS (
        SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
               rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
               (SELECT jsonb_agg(d - 'measure' ORDER BY ord)
                  FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_measure,
               (SELECT jsonb_object_agg(m.key, jsonb_build_object(
                            'rating', m.value->'rating',
                            'rating_rank', m.value->'rating_rank',
                            'rating_score', m.value->'rating_score',
                            'breakdown', (SELECT jsonb_agg(d - 'measure' ORDER BY ord)
                                            FROM jsonb_array_elements(m.value->'breakdown')
                                              WITH ORDINALITY b(d, ord)),
                            'scoped_ranks', m.value->'scoped_ranks',
                            'scoped_scores', m.value->'scoped_scores'))
                  FROM jsonb_each(rating_modes) m) AS modes_no_measure
        FROM player_stats
        WHERE sport IN ('NBA','NFL')
          AND (rating IS NOT NULL OR rating_breakdown IS NOT NULL OR rating_modes IS NOT NULL)
    )
    SELECT count(*) INTO mismatch
    FROM _player_parity b
    LEFT JOIN rebuilt r USING (sport, season, player_id, league_id)
    WHERE r.sport IS NULL
       OR r.rating IS DISTINCT FROM b.rating
       OR r.rating_rank IS DISTINCT FROM b.rating_rank
       OR r.rating_score IS DISTINCT FROM b.rating_score
       OR r.rating_scoped_ranks IS DISTINCT FROM b.rating_scoped_ranks
       OR r.rating_scoped_scores IS DISTINCT FROM b.rating_scoped_scores
       OR r.breakdown_no_measure IS DISTINCT FROM b.breakdown_no_measure
       OR r.modes_no_measure IS DISTINCT FROM b.modes_no_measure;
    IF mismatch > 0 THEN
        RAISE EXCEPTION 'Player rating rebuild broke numerical parity for % rows', mismatch;
    END IF;

    WITH rebuilt AS (
        SELECT sport, season, team_id, COALESCE(league_id, 0) AS league_id,
               rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
               (SELECT jsonb_agg(d - 'measure' ORDER BY ord)
                  FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_measure
        FROM team_stats
        WHERE sport IN ('NBA','NFL')
          AND (rating IS NOT NULL OR rating_breakdown IS NOT NULL)
    )
    SELECT count(*) INTO mismatch
    FROM _team_parity b
    LEFT JOIN rebuilt r USING (sport, season, team_id, league_id)
    WHERE r.sport IS NULL
       OR r.rating IS DISTINCT FROM b.rating
       OR r.rating_rank IS DISTINCT FROM b.rating_rank
       OR r.rating_score IS DISTINCT FROM b.rating_score
       OR r.rating_scoped_ranks IS DISTINCT FROM b.rating_scoped_ranks
       OR r.rating_scoped_scores IS DISTINCT FROM b.rating_scoped_scores
       OR r.breakdown_no_measure IS DISTINCT FROM b.breakdown_no_measure;
    IF mismatch > 0 THEN
        RAISE EXCEPTION 'Team rating rebuild broke numerical parity for % rows', mismatch;
    END IF;
END $parity$;

-- ---------------------------------------------------------------------------
-- Contract: every NBA/NFL player measure is a source identity distinct from the
-- display label; the named producer examples survive; stored breakdowns carry it.
-- ---------------------------------------------------------------------------
DO $contract$
BEGIN
    IF (SELECT measure FROM public.rating_measurements('NBA','{"blk":2.1}','total') WHERE label='Rim Protection')
        IS DISTINCT FROM 'blocks' THEN RAISE EXCEPTION 'Rim Protection lost source identity (blk)'; END IF;
    IF (SELECT measure FROM public.rating_measurements('NBA','{"ast":5.2}','total') WHERE label='Playmaking')
        IS DISTINCT FROM 'assists' THEN RAISE EXCEPTION 'Playmaking lost source identity (ast)'; END IF;
    IF (SELECT measure FROM public.rating_measurements('NBA','{"turnover":2.6}','total') WHERE label='Ball Security')
        IS DISTINCT FROM 'turnovers' THEN RAISE EXCEPTION 'Ball Security lost source identity (turnover)'; END IF;
    IF (SELECT measure FROM public.rating_measurements('NBA','{"plus_minus":1.4}','total') WHERE label='On-Court Impact')
        IS DISTINCT FROM 'plus-minus' THEN RAISE EXCEPTION 'On-Court Impact lost source identity (plus_minus)'; END IF;
    IF (SELECT measure FROM public.rating_measurements('NFL','{"passing_yards":3200,"receiving_yards":0}','total','QB') WHERE label='Air Yards Responsible')
        IS DISTINCT FROM 'sum of passing, receiving, kick-return, punt-return, punt and interception yards'
        THEN RAISE EXCEPTION 'Air Yards Responsible lost formula identity'; END IF;
    IF (SELECT measure FROM public.rating_measurements('NFL','{"tackles_for_loss":4,"defensive_sacks":9}','total','LB') WHERE label='Tackles For Loss')
        IS DISTINCT FROM 'max of tackles for loss and defensive sacks'
        THEN RAISE EXCEPTION 'Tackles For Loss lost combination identity'; END IF;
    IF (SELECT measure FROM public.rating_measurements_team('NBA','{"blk":4.1}') WHERE label='Rim Protection')
        IS DISTINCT FROM 'blocks' THEN RAISE EXCEPTION 'Team Rim Protection lost source identity'; END IF;
    IF EXISTS (SELECT 1 FROM public.rating_measurements('NBA',
                    '{"pts":1,"reb":1,"ast":1,"stl":1,"blk":1,"fg3m":1,"plus_minus":1,"turnover":1,"pf":1,"fta":1}','total')
                WHERE measure = label)
        THEN RAISE EXCEPTION 'NBA measure still echoes the display label'; END IF;
    IF EXISTS (SELECT 1 FROM public.rating_measurements('NFL',
                    '{"passing_yards":1,"rushing_yards":1,"total_tackles":1,"defensive_interceptions":1,"fumbles_lost":1}','total','LB')
                WHERE measure = label)
        THEN RAISE EXCEPTION 'NFL measure still echoes the display label'; END IF;
    IF EXISTS (SELECT 1 FROM public.rating_measurements_team('NBA','{"pts":1,"ast":1,"blk":1,"reb":1,"stl":1,"fg3m":1,"fta":1,"turnover":1,"oreb":1,"dreb":1,"pts_allowed":1,"def_fg_pct":1,"def_fg3_pct":1}')
                WHERE measure = label)
        THEN RAISE EXCEPTION 'NBA team measure still echoes the display label'; END IF;
    IF EXISTS (SELECT 1 FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
                WHERE ps.sport IN ('NBA','NFL') AND d->>'measure' = d->>'label')
        THEN RAISE EXCEPTION 'Stored NBA/NFL breakdown still carries the display label as measure'; END IF;
    IF EXISTS (SELECT 1 FROM player_stats ps CROSS JOIN LATERAL jsonb_each(ps.rating_modes) m
                CROSS JOIN LATERAL jsonb_array_elements(m.value->'breakdown') d
                WHERE ps.sport IN ('NBA','NFL') AND d->>'measure' = d->>'label')
        THEN RAISE EXCEPTION 'Stored NBA/NFL rate mode still carries the display label as measure'; END IF;
    IF EXISTS (SELECT 1 FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
                WHERE ps.sport IN ('NBA','NFL') AND COALESCE(d->>'measure','') = '')
        THEN RAISE EXCEPTION 'Stored NBA/NFL breakdown lost measurement identity'; END IF;
END $contract$;

INSERT INTO public.schema_migrations(version) VALUES ('264_rating_source_identity') ON CONFLICT DO NOTHING;
COMMIT;

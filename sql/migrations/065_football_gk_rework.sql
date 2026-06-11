-- ============================================================================
-- 065 — Football goalkeepers: bimodal value — shot-stopping + distribution.
--
-- The 063 split gave keepers their own cohort; this rebuilds what's IN it. The old GK
-- composite (Shot-Stopping=saves, Penalty Saves, Punching, High Claims) was 3/4 noise.
-- Reliability (YoY, rated keepers n=72) + value vs team conceding (n=105):
--      pass accuracy        rel 0.72   value −0.41   (accurate keepers concede less)
--      long-ball accuracy   rel 0.66   value −0.25
--      High Claims          rel 0.45   value +0.13
--      saves (volume)       rel 0.14   (shot-stopping value provided in context)
--      Penalty Saves        rel 0.14   noise
--      Punching             rel 0.21   noise
--      save %               rel 0.08   noise (textbook skill metric is a season coin
--                                       flip; no xG in-feed for a goals-prevented one)
--
-- A keeper's value is BIMODAL (Scott's eye test): a keeper on a bad team provides value
-- by stopping the barrage of shots; a keeper on a good team provides value with
-- distribution. Saves (volume) and distribution are anti-correlated across keepers, so
-- a composite of BOTH credits each keeper for their actual mode — and a keeper who
-- never faces shots simply earns ~0 on shot-stopping (he can't add value where he does
-- nothing) and earns on distribution instead. Saves' low YoY reliability is CONTEXT
-- changing (shots faced ← team), not luck like penalties_won — the saves are real value
-- provided this season, which is what a season z-rating measures.
--
-- We do NOT use goals-conceded/90: it would PENALISE the bad-team barrage-stopper (more
-- conceded → worse) — the opposite of crediting his shot-stopping value. And save% /
-- raw save-skill can't be isolated reliably from box-score data, so we don't pretend to.
--
-- GK composite = Shot-Stopping (saves) + Distribution (pass accuracy) + Long-Ball
-- Accuracy + High Claims. Pass↔long-ball collinearity 0.62 (< the 0.7 de-dup gate).
-- Drop Penalty Saves + Punching. FOOTBALL GK rows only — outfield (064) and NBA/NFL
-- untouched. No API restart. Recompute FOOTBALL. (Apply AFTER 064.)
--
-- Apply with: ./sql/migrate.sh  (or psql -f)
-- ============================================================================

BEGIN;

CREATE OR REPLACE FUNCTION public.rating_datapoints(p_sport text, p_stats jsonb, p_rate_mode text DEFAULT 'total'::text, p_position text DEFAULT NULL)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
 LANGUAGE sql
 STABLE PARALLEL SAFE
AS $function$
    -- NBA (unchanged).
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
    -- FOOTBALL. Outfield: PAdj Tackling/Interceptions + Duels/Ball Recovery/Drawing
    -- Fouls display-only (064). GK: shot-stopping (saves) + distribution + command (065).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'FOOTBALL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all', 'goals',           'out'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all', 'assists',         'out'),
        ('Shooting',        NULLIF(p_stats->>'shots_total','')::numeric,      TRUE, TRUE,   1, 'all', 'shots',           'out'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate', 'out'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all', 'key_passes',      'out'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success','out'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        FALSE, FALSE, 1, 'all', 'duels_won',       'out'),
        ('Tackling',        round(COALESCE(NULLIF(p_stats->>('tackles' || COALESCE(rs.suffix,'')),'')::numeric,
                                           NULLIF(p_stats->>'tackles','')::numeric)
                                  * 50.0 / GREATEST(NULLIF(p_stats->>'team_opp_possession','')::numeric, 30), 2),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Interceptions',   round(COALESCE(NULLIF(p_stats->>('interceptions' || COALESCE(rs.suffix,'')),'')::numeric,
                                           NULLIF(p_stats->>'interceptions','')::numeric)
                                  * 50.0 / GREATEST(NULLIF(p_stats->>'team_opp_possession','')::numeric, 30), 2),
                                                                              TRUE, TRUE,   1, 'all', NULL,              'out'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       FALSE, FALSE, 1, 'all', 'clearances',      'out'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           FALSE, FALSE, 1, 'all', 'blocks',          'out'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    FALSE, FALSE, 1, 'all', 'ball_recovery',   'out'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      FALSE, FALSE, 1, 'all', 'fouls_drawn',     'out'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all', NULL,              'out'),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all', 'possession_lost','out'),
        -- GK: shot-stopping (credits the barrage-stopper) + distribution + command.
        ('Shot-Stopping',      NULLIF(p_stats->>'saves','')::numeric,              TRUE, TRUE, 1, 'all', 'saves', 'gk'),
        ('Distribution',       NULLIF(p_stats->>'pass_accuracy','')::numeric,      TRUE, TRUE, 1, 'all', NULL,    'gk'),
        ('Long-Ball Accuracy', NULLIF(p_stats->>'long_ball_accuracy','')::numeric, TRUE, TRUE, 1, 'all', NULL,    'gk'),
        ('High Claims',        NULLIF(p_stats->>'good_high_claim','')::numeric,     TRUE, TRUE, 1, 'all', NULL,    'gk')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base, pos_class)
    WHERE p_sport = 'FOOTBALL'
      AND (CASE WHEN p_position = 'Goalkeeper' THEN v.pos_class = 'gk'
                ELSE v.pos_class = 'out' END)

    UNION ALL
    -- NFL (unchanged).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'NFL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Total Yards',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>'rushing_yards')::numeric,0)
                + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_yards' || rs.suffix))::numeric,(p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>('rushing_yards' || rs.suffix))::numeric,(p_stats->>'rushing_yards')::numeric,0)
                + COALESCE((p_stats->>('receiving_yards' || rs.suffix))::numeric,(p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>('kick_return_yards' || rs.suffix))::numeric,(p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_returner_return_yards' || rs.suffix))::numeric,(p_stats->>'punt_returner_return_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Touchdowns',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_touchdowns' || rs.suffix))::numeric,(p_stats->>'passing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('rushing_touchdowns' || rs.suffix))::numeric,(p_stats->>'rushing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('receiving_touchdowns' || rs.suffix))::numeric,(p_stats->>'receiving_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('kick_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'kick_return_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('punt_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'punt_return_touchdowns')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Receiving',        NULLIF(p_stats->>'receptions','')::numeric,          TRUE, TRUE,   1, 'offense', 'receptions'),
        ('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_interceptions' || rs.suffix))::numeric,(p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>('fumbles_lost' || rs.suffix))::numeric,(p_stats->>'fumbles_lost')::numeric,0)
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,       TRUE, TRUE,   1, 'defense', 'total_tackles'),
        ('Tackles For Loss', NULLIF(p_stats->>'tackles_for_loss','')::numeric,    TRUE, TRUE,   1, 'defense', 'tackles_for_loss'),
        ('Sacks',            NULLIF(p_stats->>'defensive_sacks','')::numeric,     TRUE, TRUE,   1, 'defense', 'defensive_sacks'),
        ('Pass Defense',     NULLIF(p_stats->>'passes_defended','')::numeric,     TRUE, TRUE,   1, 'defense', 'passes_defended'),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense', 'defensive_interceptions'),
        ('Fumble Recovery',  NULLIF(p_stats->>'fumbles_recovered','')::numeric,   TRUE, TRUE,   1, 'defense', 'fumbles_recovered'),
        ('Field Goals',      NULLIF(p_stats->>'field_goals_made','')::numeric,    TRUE, TRUE,   1, 'special', 'field_goals_made'),
        ('Punting',          NULLIF(p_stats->>'punts_inside_20','')::numeric,     TRUE, TRUE,   1, 'special', 'punts_inside_20')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NFL';
$function$;

-- Recompute FOOTBALL (NBA/NFL untouched). Notify trigger fires only on `percentiles`.
ALTER TABLE player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;
DO $$
DECLARE s INTEGER;
BEGIN
    FOR s IN SELECT DISTINCT season FROM player_stats WHERE sport = 'FOOTBALL' ORDER BY 1 LOOP
        PERFORM compute_rating('FOOTBALL', s);
        PERFORM compute_event_starline('FOOTBALL', s);
    END LOOP;
END $$;
ALTER TABLE player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;

-- Gate: keepers carry exactly the four new GK labels, none of the dropped shot-stopping
-- noise (Penalty Saves / Punching); outfielders carry no GK labels.
DO $$
DECLARE v_old INTEGER; v_new_ok INTEGER; v_out_gk INTEGER;
BEGIN
    SELECT count(*) INTO v_old
    FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport='FOOTBALL' AND ps.rating_breakdown IS NOT NULL
      AND el->>'label' IN ('Penalty Saves','Punching');
    SELECT count(*) INTO v_out_gk
    FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport='FOOTBALL' AND ps.position IS DISTINCT FROM 'Goalkeeper'
      AND el->>'label' IN ('Distribution','Long-Ball Accuracy');
    SELECT count(*) INTO v_new_ok
    FROM player_stats ps
    WHERE ps.sport='FOOTBALL' AND ps.season=2025 AND ps.position='Goalkeeper'
      AND ps.rating_breakdown IS NOT NULL
      AND (SELECT count(*) FROM jsonb_array_elements(ps.rating_breakdown) el
           WHERE el->>'label' IN ('Shot-Stopping','Distribution','Long-Ball Accuracy','High Claims')) = 4;

    IF v_old > 0 OR v_out_gk > 0 THEN
        RAISE EXCEPTION '065 FAIL: stale Penalty Saves/Punching rows=%, GK labels on outfielders=%', v_old, v_out_gk;
    END IF;
    RAISE NOTICE '065 OK: keepers rated on Shot-Stopping + Distribution + Long-Ball + High Claims (% keepers, 2025)', v_new_ok;
END $$;

COMMIT;

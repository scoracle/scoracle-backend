-- ============================================================================
-- 064 — Football outfield composite: PAdj Tackling/Interceptions; drop Duels,
--       Ball Recovery, Drawing Fouls to display.
--
-- Incremental-validity audit (leave-one-out vs team goal-difference + team-altitude
-- corr of a metric's team total with goals_for/against, n=192 team-seasons):
--
--   * Tackling / Interceptions — RAW volume is mildly perverse (a team tackling/
--     intercepting more concedes more: t_gd −.09 / −.17 — it rewards being camped in
--     your own half). Possession-adjustment fixes it: PAdj = raw × 50 / opponent-
--     possession (per-90-for-defenders; ×50 washes out of the z-score; 50% is the
--     structural league average since possession is zero-sum). Modeled both levels;
--     it FLIPS the outcome sign decisively:
--        player:  Tackling vs goal-diff  −.074 → +.151 ;  Interceptions −.068 → +.106
--        team:    Tackling vs goal-diff  −.241 → +.528 ;  Interceptions −.333 → +.526
--                 Tackling vs conceded   +.190 → −.337 (matches the 060 team result)
--     A SWAP, not a drop. Applied to the rate-resolved base so every rate mode stays
--     consistent (total/per_90/per_game all possession-adjusted), 2-decimal so per-90
--     keeps its resolution.
--
--     OPPONENT POSSESSION SOURCE: rating_datapoints reads `team_opp_possession` from
--     the player's stats — but that key isn't produced by the committed pipeline (it
--     only ever existed in an ad-hoc clone experiment; prod has 0). The team-level
--     `team_stats.opp_possession_pct` IS populated on prod every season, and equals the
--     per-player value exactly. So _compute_rating_bundle now INJECTS each FOOTBALL
--     player's team opp-possession from team_stats at rating time (one extra LEFT JOIN)
--     — no new derived field, no backfill, auto-covers future data. (The per-game
--     compute_event_starline has no per-event opp-possession, so its Tackling stays on
--     the 30-floor = uniform scaling = z unchanged ≈ raw; the season composite is what
--     matters and is fully adjusted.)
--
--   * Duels — HIGHEST-influence wedge (corr .82 with the rating) yet NO outcome signal
--     (team t_gd .06; leave-one-out −.032 — it drags). Pure activity. → display.
--   * Ball Recovery — looks valuable at team level (t_gd +.44) but that value is already
--     carried by the passing/creation cluster (collinear); on its margin it drags
--     (−.020). Redundant. → display.
--   * Drawing Fouls — weak value (t_gd +.11) and drags the composite (−.024). → display.
--
-- All three → in_comp=FALSE, in_spec=FALSE (display tier, like Blocks/Clearances). GK
-- datapoints untouched here (their rework is 065). NBA/NFL untouched. Recompute FOOTBALL.
-- No API restart (rating_datapoints isn't a prepared statement).
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
    -- FOOTBALL. pos_class gates GK vs outfield (063). Tackling/Interceptions are
    -- possession-adjusted (raw × 50 / opp-possession, floored at 30), on the rate-
    -- resolved base; team_opp_possession is injected by _compute_rating_bundle.
    -- Duels / Ball Recovery / Drawing Fouls are display-only (FALSE, FALSE).
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
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all', 'saves',           'gk'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all', NULL,              'gk'),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all', NULL,              'gk'),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all', NULL,              'gk')
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

-- _compute_rating_bundle — inject FOOTBALL players' team opp-possession (from
-- team_stats.opp_possession_pct, which IS on prod) into the stats passed to
-- rating_datapoints, so PAdj resolves with real opponent possession. Body is the live
-- (063) definition; the only changes are the LEFT JOIN topp + the injected stats arg.
CREATE OR REPLACE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, specialist numeric, specialist_rank numeric, specialty text, breakdown jsonb, scoped_ranks jsonb)
 LANGUAGE sql STABLE
AS $function$
    WITH dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
               tm.conference, tm.division,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet
        FROM player_stats ps
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        LEFT JOIN LATERAL (
            SELECT tts.stats->>'opp_possession_pct' AS opp
            FROM team_stats tts
            WHERE tts.team_id = ps.team_id AND tts.sport = p_sport AND tts.season = p_season
            LIMIT 1
        ) topp ON p_sport = 'FOOTBALL'
        CROSS JOIN LATERAL rating_datapoints(
            p_sport,
            CASE WHEN topp.opp IS NOT NULL
                 THEN ps.stats || jsonb_build_object('team_opp_possession', topp.opp)
                 ELSE ps.stats END,
            p_rate_mode, ps.position) d
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND COALESCE((
                SELECT bool_and(COALESCE((ps.stats->>rt.stat_key)::numeric, 0) >= rt.min_value)
                FROM public.rating_thresholds rt WHERE rt.sport = p_sport
              ), FALSE)
    ),
    pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.position, d.conference, d.division,
               d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM dp d JOIN pop p USING (label)
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
        SELECT player_id, league_id, composite FROM comp_flat  WHERE p_sport <> 'NFL'
        UNION ALL
        SELECT player_id, league_id, composite FROM comp_facet WHERE p_sport =  'NFL'
    ),
    sp AS (
        SELECT DISTINCT ON (player_id, league_id)
               player_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY player_id, league_id, zr DESC, label
    ),
    scored AS (
        SELECT player_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct,
               CASE WHEN p_sport='NFL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_position,
               CASE WHEN p_sport IN ('NFL','NBA') AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, conference ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_conference,
               CASE WHEN p_sport='NFL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, division ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_division,
               CASE WHEN p_sport='FOOTBALL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, league_id ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_league
        FROM z
    ),
    bd AS (
        SELECT s.player_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet,
                   'is_specialty', (sp.specialty IS NOT DISTINCT FROM s.label),
                   'scoped_pct', jsonb_strip_nulls(jsonb_build_object(
                       'position', s.pct_position, 'conference', s.pct_conference,
                       'division', s.pct_division, 'league', s.pct_league))
               ) ORDER BY s.label) AS breakdown
        FROM scored s
        LEFT JOIN sp USING (player_id, league_id)
        GROUP BY s.player_id, s.league_id
    ),
    base AS (
        SELECT c.player_id, c.league_id,
               ROUND(c.composite, 4)  AS composite,
               ROUND(sp.specialist, 4) AS specialist,
               sp.specialty, bd.breakdown
        FROM comp c
        JOIN sp USING (player_id, league_id)
        JOIN bd USING (player_id, league_id)
    ),
    ranks AS (
        SELECT player_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY composite  ASC))::numeric * 100, 1) AS composite_rank,
               ROUND((percent_rank() OVER (ORDER BY specialist ASC))::numeric * 100, 1) AS specialist_rank
        FROM base
    ),
    scoped AS (
        SELECT b.player_id, b.league_id,
               CASE WHEN p_sport='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position ORDER BY b.composite ASC))::numeric*100,1) END AS pos_pct,
               CASE WHEN p_sport IN ('NFL','NBA') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.conference ORDER BY b.composite ASC))::numeric*100,1) END AS conf_pct,
               CASE WHEN p_sport='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.division ORDER BY b.composite ASC))::numeric*100,1) END AS div_pct,
               CASE WHEN p_sport='FOOTBALL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, ps.league_id ORDER BY b.composite ASC))::numeric*100,1) END AS league_pct
        FROM base b
        JOIN player_stats ps
          ON ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
         AND COALESCE(ps.league_id, 0) = b.league_id
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        WHERE ps.position IS NOT NULL
    )
    SELECT b.player_id, b.league_id,
           b.composite, r.composite_rank,
           b.specialist, r.specialist_rank, b.specialty,
           b.breakdown,
           NULLIF(jsonb_strip_nulls(jsonb_build_object(
               'position', sc.pos_pct, 'conference', sc.conf_pct,
               'division', sc.div_pct, 'league', sc.league_pct)), '{}'::jsonb) AS scoped_ranks
    FROM base b
    JOIN ranks r USING (player_id, league_id)
    LEFT JOIN scoped sc USING (player_id, league_id);
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

-- Gate: (a) Duels/Ball Recovery/Drawing Fouls out of the composite; (b) PAdj used the
-- REAL team opponent-possession (breakdown Tackling = round(tackles × 50/opp, 2) for
-- non-50%-possession teams — fails if injection broke and the 30-floor was used).
DO $$
DECLARE v_comp INTEGER; v_padj INTEGER;
BEGIN
    SELECT count(*) INTO v_comp
    FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport='FOOTBALL' AND ps.rating_breakdown IS NOT NULL
      AND el->>'label' IN ('Duels','Ball Recovery','Drawing Fouls')
      AND (el->>'in_comp')::bool;
    IF v_comp > 0 THEN
        RAISE EXCEPTION '064 FAIL: % Duels/Ball Recovery/Drawing Fouls rows still in composite', v_comp;
    END IF;

    SELECT count(*) INTO v_padj
    FROM player_stats ps
    JOIN team_stats ts ON ts.team_id=ps.team_id AND ts.sport='FOOTBALL' AND ts.season=ps.season
    CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport='FOOTBALL' AND ps.season=2025 AND ps.position<>'Goalkeeper'
      AND el->>'label'='Tackling'
      AND (ps.stats->>'tackles')::numeric > 0
      AND (ts.stats->>'opp_possession_pct')::numeric NOT BETWEEN 49.5 AND 50.5
      AND (el->>'value')::numeric = round((ps.stats->>'tackles')::numeric * 50.0
                                          / GREATEST((ts.stats->>'opp_possession_pct')::numeric, 30), 2);
    IF v_padj = 0 THEN
        RAISE EXCEPTION '064 FAIL: PAdj did not use real opponent-possession (injection broken?)';
    END IF;
    RAISE NOTICE '064 OK: Tackling/Interceptions PAdj with real opp-possession (% verified rows); Duels/Ball Recovery/Drawing Fouls display-only', v_padj;
END $$;

COMMIT;

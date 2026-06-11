-- ============================================================================
-- 066 — Football goalkeepers: Goals Prevented core. The keeper's three jobs.
--
-- 065's bimodal GK composite (Shot-Stopping=raw saves + Distribution + Long-Ball +
-- High Claims) failed the eye test: top-10 keepers were High-Claims merchants from
-- mid-table clubs; the marquee keepers sat mid-pack. Two fixes, both modelled:
--
--   * High Claims is the keeper's Blocks/Clearances — weakest metric (rel .45, value
--     +.13 perverse) but FAT-TAILED (max z 4.07 vs ~2.3 for the others), so cross-
--     claiming keepers on weak teams dominate. Even possession-adjusted it stays
--     fat-tailed. DROPPED.
--   * Raw saves is VOLUME (workload), not skill — and PAdj-saves only de-confounds it,
--     it can't make volume into quality. The right shot-stopping metric is
--     GOALS PREVENTED = saves − (saves + goals_conceded) × league-avg save%  —  i.e.
--     save% expressed as goals stopped above an average keeper, weighted by shots
--     actually faced. It credits quality AND volume (so it also honours the bad-team
--     barrage-stopper). Modelled top-10 is finally credible — Maignan (+10.5),
--     Provedel (+10.2), Donnarumma, Neuer, Oblak, Joan García, Sommer; the claims-
--     merchants are gone. league-avg save% is a per-season structural constant,
--     injected into _compute_rating_bundle exactly like opponent-possession (064).
--
-- RELIABILITY EXCEPTION (documented on purpose): Goals Prevented YoY autocorrelation is
-- ~0.156 — better than save% (0.079) and raw saves (0.141), but still LOW. Keeper shot-
-- stopping genuinely regresses season-to-season (needs ~3 seasons to stabilise; we have
-- no post-shot xG in-feed to isolate skill from shot difficulty). We include it anyway,
-- on principle: shot-stopping is the keeper's DEFINING job — measuring it imperfectly
-- beats omitting the dimension. This is a deliberate exception to the value×reliability
-- gate, justified by essentiality.
--
-- Note: Goals Prevented correctly rates some reputation-elite keepers LOWER (Alisson
-- save% 64.8, Raya 69.8 in 2025) — their actual shot-stopping season was mediocre; the
-- "elite" read is reputation, not this season's data. The engine is doing its job.
--
-- GK composite = Goals Prevented + Distribution (pass accuracy) + Long-Ball Accuracy.
-- FOOTBALL GK rows only — outfield (064) and NBA/NFL untouched. Recompute FOOTBALL.
-- No API restart. (Apply AFTER 064/065.)
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
    -- Fouls display-only (064). GK: Goals Prevented + distribution + long-ball (066).
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
        -- GK: Goals Prevented (saves above a league-avg keeper, given shots faced)
        -- + distribution. league_avg_save_pct injected by _compute_rating_bundle.
        ('Goals Prevented',    NULLIF(p_stats->>'saves','')::numeric
                               - (COALESCE(NULLIF(p_stats->>'saves','')::numeric,0) + COALESCE(NULLIF(p_stats->>'goals_conceded','')::numeric,0))
                                 * NULLIF(p_stats->>'league_avg_save_pct','')::numeric / 100.0,
                                                                              TRUE, TRUE, 1, 'all', NULL, 'gk'),
        ('Distribution',       NULLIF(p_stats->>'pass_accuracy','')::numeric,      TRUE, TRUE, 1, 'all', NULL, 'gk'),
        ('Long-Ball Accuracy', NULLIF(p_stats->>'long_ball_accuracy','')::numeric, TRUE, TRUE, 1, 'all', NULL, 'gk')
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

-- _compute_rating_bundle — inject BOTH the team opp-possession (064, for PAdj) and the
-- per-season league-average GK save% (066, for Goals Prevented) into the football
-- player's stats. Both are per-(sport,season) structural constants joined/computed at
-- rating time; no new derived fields, no backfill.
CREATE OR REPLACE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, specialist numeric, specialist_rank numeric, specialty text, breakdown jsonb, scoped_ranks jsonb)
 LANGUAGE sql STABLE
AS $function$
    WITH lasp AS (
        SELECT CASE WHEN p_sport='FOOTBALL'
                    THEN round(avg(NULLIF(stats->>'save_pct','')::numeric), 4) END AS asp
        FROM player_stats
        WHERE sport='FOOTBALL' AND season=p_season AND position='Goalkeeper'
          AND (stats->>'appearances')::numeric >= 15
    ),
    dp AS (
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

-- Gate: keepers carry exactly {Goals Prevented, Distribution, Long-Ball Accuracy} and
-- none of the dropped GK rows; Goals Prevented actually computed (varies, non-degenerate);
-- outfielders carry no GK-only labels.
DO $$
DECLARE v_old INTEGER; v_new_ok INTEGER; v_out_gk INTEGER; v_gp_spread NUMERIC;
BEGIN
    SELECT count(*) INTO v_old
    FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport='FOOTBALL' AND ps.rating_breakdown IS NOT NULL
      AND el->>'label' IN ('Shot-Stopping','Penalty Saves','Punching','High Claims');
    SELECT count(*) INTO v_out_gk
    FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport='FOOTBALL' AND ps.position IS DISTINCT FROM 'Goalkeeper'
      AND el->>'label' IN ('Goals Prevented','Distribution','Long-Ball Accuracy');
    SELECT count(*) INTO v_new_ok
    FROM player_stats ps
    WHERE ps.sport='FOOTBALL' AND ps.season=2025 AND ps.position='Goalkeeper'
      AND ps.rating_breakdown IS NOT NULL
      AND (SELECT count(*) FROM jsonb_array_elements(ps.rating_breakdown) el
           WHERE el->>'label' IN ('Goals Prevented','Distribution','Long-Ball Accuracy')) = 3;
    SELECT round(stddev_pop((el->>'value')::numeric),2) INTO v_gp_spread
    FROM player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport='FOOTBALL' AND ps.season=2025 AND ps.position='Goalkeeper' AND el->>'label'='Goals Prevented';

    IF v_old > 0 OR v_out_gk > 0 THEN
        RAISE EXCEPTION '066 FAIL: stale GK rows=%, GK labels on outfielders=%', v_old, v_out_gk;
    END IF;
    IF COALESCE(v_gp_spread,0) = 0 THEN
        RAISE EXCEPTION '066 FAIL: Goals Prevented is degenerate (no spread) — save%% injection broken?';
    END IF;
    RAISE NOTICE '066 OK: keepers on Goals Prevented + Distribution + Long-Ball (% keepers; GP spread %)', v_new_ok, v_gp_spread;
END $$;

COMMIT;

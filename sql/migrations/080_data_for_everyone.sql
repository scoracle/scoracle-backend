-- ============================================================================
-- 080_data_for_everyone.sql  (Phase 2 — engine)
-- "Gated composite + data-for-everyone": players below the rating gate still get a
-- full breakdown (z + per-stat percentile vs the RATED cohort), but are excluded
-- from the ranked composite — rating_composite / rank / score = NULL for them, so
-- they never touch the leaderboard or the cohort distribution.
--
-- Parity: the rated (gated) cohort is BYTE-IDENTICAL to before. `pop` (mean/sd) and
-- every rank/score window stay restricted to `WHERE is_ranked`, which is exactly the
-- old gated set — so gated players' composite/rank/score/breakdown do not move
-- (gate-checked at recompute below). The only additions are sub-gate players' rows.
--
-- Reads (handled in db.go, not here): the leaderboard already filters
-- rating_composite IS NOT NULL → unranked players stay off it automatically; the
-- profile/sparkline switch to surfacing a season on rating_breakdown presence.
--
-- Apply: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/080_data_for_everyone.sql
-- ============================================================================

BEGIN;

CREATE OR REPLACE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, composite_score numeric, specialist numeric, specialist_rank numeric, specialist_score numeric, specialty text, breakdown jsonb, scoped_ranks jsonb, scoped_scores jsonb)
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
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet,
               -- Phase 2: TAG eligibility instead of filtering it out. Sub-gate players
               -- still produce datapoints (for their breakdown); pop/ranks/scoped below
               -- use `WHERE is_ranked` so the rated cohort is unchanged.
               COALESCE((
                   SELECT bool_and(COALESCE((ps.stats->>rt.stat_key)::numeric, 0) >= rt.min_value)
                   FROM public.rating_thresholds rt WHERE rt.sport = p_sport
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
        SELECT d.player_id, d.league_id, d.position, d.conference, d.division,
               d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value, d.is_ranked,
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
               sp.specialty, bd.breakdown, rk.is_ranked
        FROM comp c
        JOIN sp USING (player_id, league_id)
        JOIN bd USING (player_id, league_id)
        JOIN rk USING (player_id, league_id)
    ),
    ranks AS (
        SELECT player_id, league_id, is_ranked,
               CASE WHEN is_ranked THEN ROUND((percent_rank() OVER (PARTITION BY is_ranked ORDER BY composite  ASC))::numeric * 100, 1) END AS composite_rank,
               CASE WHEN is_ranked THEN ROUND((percent_rank() OVER (PARTITION BY is_ranked ORDER BY specialist ASC))::numeric * 100, 1) END AS specialist_rank,
               CASE WHEN is_ranked THEN public.rating_score(composite,  AVG(composite)  OVER(PARTITION BY is_ranked), STDDEV_POP(composite)  OVER(PARTITION BY is_ranked)) END AS composite_score,
               CASE WHEN is_ranked THEN public.rating_score(specialist, AVG(specialist) OVER(PARTITION BY is_ranked), STDDEV_POP(specialist) OVER(PARTITION BY is_ranked)) END AS specialist_score
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
           CASE WHEN b.is_ranked THEN b.specialist END AS specialist, r.specialist_rank, r.specialist_score,
           CASE WHEN b.is_ranked THEN b.specialty END AS specialty,
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

-- Recompute every FOOTBALL season so sub-gate players gain their breakdown.
CREATE TEMP TABLE _080_pre AS
    SELECT player_id, season, COALESCE(league_id,0) AS lid,
           rating_composite, rating_composite_rank, rating_composite_score,
           rating_specialist, rating_specialist_rank, rating_specialty, rating_breakdown
    FROM player_stats WHERE sport='FOOTBALL' AND rating_composite IS NOT NULL;

SET session_replication_role = replica;
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT season FROM player_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM compute_rating('FOOTBALL', r.season);
    END LOOP;
END $$;
SET session_replication_role = DEFAULT;

-- Parity gate: the RANKED cohort must be byte-identical (incl. breakdown) — only
-- sub-gate (unranked) breakdowns are new.
DO $$
DECLARE v_drift BIGINT;
BEGIN
    SELECT count(*) INTO v_drift
    FROM _080_pre pre JOIN player_stats ps
      ON ps.player_id=pre.player_id AND ps.sport='FOOTBALL' AND ps.season=pre.season
     AND COALESCE(ps.league_id,0)=pre.lid
    WHERE pre.rating_composite       IS DISTINCT FROM ps.rating_composite
       OR pre.rating_composite_rank  IS DISTINCT FROM ps.rating_composite_rank
       OR pre.rating_composite_score IS DISTINCT FROM ps.rating_composite_score
       OR pre.rating_specialist      IS DISTINCT FROM ps.rating_specialist
       OR pre.rating_specialist_rank IS DISTINCT FROM ps.rating_specialist_rank
       OR pre.rating_specialty       IS DISTINCT FROM ps.rating_specialty
       OR pre.rating_breakdown       IS DISTINCT FROM ps.rating_breakdown;
    IF v_drift > 0 THEN
        RAISE EXCEPTION '080 PARITY FAIL: % ranked-cohort rows changed (gated ratings must not move)', v_drift;
    END IF;
    RAISE NOTICE '080 parity OK: ranked cohort byte-identical across % rows', (SELECT count(*) FROM _080_pre);
END $$;

-- Smoke: sub-gate players now carry a breakdown but no rank.
DO $$
DECLARE v_unranked_bd BIGINT; v_romeo text;
BEGIN
    SELECT count(*) INTO v_unranked_bd FROM player_stats
     WHERE sport='FOOTBALL' AND rating_composite_rank IS NULL AND rating_breakdown IS NOT NULL;
    RAISE NOTICE '080: % sub-gate football player-seasons now carry an (unranked) breakdown', v_unranked_bd;
    IF v_unranked_bd = 0 THEN RAISE EXCEPTION '080 FAIL: no sub-gate breakdowns produced'; END IF;
END $$;

COMMIT;

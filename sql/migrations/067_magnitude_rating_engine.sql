-- ============================================================================
-- 067 — Magnitude rating: replace percentile as the headline rating with a score.
--
-- ★ THE REVELATION ★  The leaderboard/profile "RATING" has always been the PERCENTILE
-- rank (percent_rank × 100). Percentile is rank-based, so the top 1% of ANY population
-- is — by definition — ≥99, no matter what you rank by (1 metric or 14; proven: ranking
-- 1,785 PL-era players by goals alone, by passes alone, or by a 14-metric sum each yields
-- ~15-19 players at ≥99). That's a wall of 99s with no differentiating power: Yamal
-- (composite 25.1) and the #12 player (16.2) both read ~99.x despite a chasm between them.
--
-- The fix is a MAGNITUDE score — a transform of the composite itself, which preserves the
-- gaps percentile destroys:
--       score = 50 + 10 × (composite − cohort_mean) / cohort_sd ,  clamped [1, 99]
-- A standard T-score: average player = 50, SD = 10. The composite z-sum already lives on
-- this scale (mean ~0). Result on football 2025: 99-club 19 → 4, and the field spreads by
-- real value (Mbappé 94, Kane 89, Pedri 85). The ×10 slope is a single tunable constant.
--
-- This is the rating model's display backbone — once proven on football it becomes the
-- formula for NBA / NFL / future sports. We ADD the score columns (non-destructive); the
-- percentile columns stay (cohort "top X%" context still has a use). Computed wherever the
-- rank is computed: player composite + specialist, per rate-mode (rating_modes), and per
-- cohort (scoped); team composite + specialist + scoped. Recompute ALL sports.
--
-- No API restart needed for the columns themselves, but 068 (the API/frontend switch)
-- will add the score to the payloads. Recompute touches rating_* columns only (not
-- `percentiles`), so the FCM notify trigger won't fire; disabled anyway, house style.
--
-- Apply with: ./sql/migrate.sh  (or psql -f)
-- ============================================================================

BEGIN;

-- ── 1. Score columns (non-destructive; percentile columns retained) ──────────
ALTER TABLE player_stats
    ADD COLUMN IF NOT EXISTS rating_composite_score  numeric,
    ADD COLUMN IF NOT EXISTS rating_specialist_score numeric,
    ADD COLUMN IF NOT EXISTS rating_scoped_scores    jsonb;
ALTER TABLE team_stats
    ADD COLUMN IF NOT EXISTS rating_composite_score  numeric,
    ADD COLUMN IF NOT EXISTS rating_specialist_score numeric,
    ADD COLUMN IF NOT EXISTS rating_scoped_scores    jsonb;

-- ── 2. Player bundle: emit composite_score / specialist_score / scoped_scores ─
--    Body is the live (066) definition; additions are the *_score columns in the
--    ranks + scoped CTEs and the RETURNS TABLE signature. The magnitude transform is
--    factored into a small SQL helper so player + team share one definition.
CREATE OR REPLACE FUNCTION public.rating_score(p_value numeric, p_mean numeric, p_sd numeric)
 RETURNS numeric LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN p_value IS NULL OR p_sd IS NULL OR p_sd = 0 THEN NULL
                ELSE ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (p_value - p_mean) / p_sd))::numeric, 1) END;
$$;

DROP FUNCTION IF EXISTS public._compute_rating_bundle(text, integer, text);
CREATE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric,
               composite_score numeric, specialist numeric, specialist_rank numeric, specialist_score numeric,
               specialty text, breakdown jsonb, scoped_ranks jsonb, scoped_scores jsonb)
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
               ROUND((percent_rank() OVER (ORDER BY specialist ASC))::numeric * 100, 1) AS specialist_rank,
               public.rating_score(composite,  AVG(composite)  OVER(), STDDEV_POP(composite)  OVER()) AS composite_score,
               public.rating_score(specialist, AVG(specialist) OVER(), STDDEV_POP(specialist) OVER()) AS specialist_score
        FROM base
    ),
    scoped AS (
        SELECT b.player_id, b.league_id,
               CASE WHEN p_sport='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position ORDER BY b.composite ASC))::numeric*100,1) END AS pos_pct,
               CASE WHEN p_sport IN ('NFL','NBA') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.conference ORDER BY b.composite ASC))::numeric*100,1) END AS conf_pct,
               CASE WHEN p_sport='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.division ORDER BY b.composite ASC))::numeric*100,1) END AS div_pct,
               CASE WHEN p_sport='FOOTBALL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, ps.league_id ORDER BY b.composite ASC))::numeric*100,1) END AS league_pct,
               CASE WHEN p_sport='NFL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position)) END AS pos_score,
               CASE WHEN p_sport IN ('NFL','NBA') THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.conference), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.conference)) END AS conf_score,
               CASE WHEN p_sport='NFL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.division), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.division)) END AS div_score,
               CASE WHEN p_sport='FOOTBALL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, ps.league_id), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, ps.league_id)) END AS league_score
        FROM base b
        JOIN player_stats ps
          ON ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
         AND COALESCE(ps.league_id, 0) = b.league_id
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        WHERE ps.position IS NOT NULL
    )
    SELECT b.player_id, b.league_id,
           b.composite, r.composite_rank, r.composite_score,
           b.specialist, r.specialist_rank, r.specialist_score, b.specialty,
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

-- ── 3. compute_rating: store the new score columns + per-mode scores ─────────
CREATE OR REPLACE FUNCTION public.compute_rating(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
    v_mode    TEXT;
    v_modes   TEXT[] := ARRAY['total'] || COALESCE(
        (SELECT array_agg(mode ORDER BY mode) FROM public.rate_modes WHERE sport = p_sport),
        ARRAY[]::TEXT[]);
BEGIN
    UPDATE player_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_composite_score = NULL, rating_specialist_score = NULL, rating_scoped_scores = NULL,
           rating_breakdown = NULL, rating_scoped_ranks = NULL, rating_modes = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL OR rating_modes IS NOT NULL);

    FOREACH v_mode IN ARRAY v_modes LOOP
        IF v_mode = 'total' THEN
            WITH b AS MATERIALIZED (
                SELECT * FROM _compute_rating_bundle(p_sport, p_season, 'total')
            )
            UPDATE player_stats ps SET
                rating_composite       = b.composite,
                rating_specialist      = b.specialist,
                rating_specialty       = b.specialty,
                rating_composite_rank  = b.composite_rank,
                rating_specialist_rank = b.specialist_rank,
                rating_composite_score = b.composite_score,
                rating_specialist_score= b.specialist_score,
                rating_breakdown       = b.breakdown,
                rating_scoped_ranks    = b.scoped_ranks,
                rating_scoped_scores   = b.scoped_scores
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
            GET DIAGNOSTICS v_updated = ROW_COUNT;
        ELSE
            WITH b AS MATERIALIZED (
                SELECT * FROM _compute_rating_bundle(p_sport, p_season, v_mode)
            )
            UPDATE player_stats ps SET
                rating_modes = COALESCE(ps.rating_modes, '{}'::jsonb) || jsonb_build_object(
                    v_mode, jsonb_build_object(
                        'composite',        b.composite,
                        'composite_rank',   b.composite_rank,
                        'composite_score',  b.composite_score,
                        'specialist',       b.specialist,
                        'specialist_rank',  b.specialist_rank,
                        'specialist_score', b.specialist_score,
                        'specialty',        b.specialty,
                        'breakdown',        b.breakdown,
                        'scoped_ranks',     b.scoped_ranks,
                        'scoped_scores',    b.scoped_scores
                    ))
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
        END IF;
    END LOOP;

    RETURN v_updated;
END;
$function$;

-- ── 4. compute_team_rating: composite/specialist score + scoped scores ──────
CREATE OR REPLACE FUNCTION public.compute_team_rating(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_composite_score = NULL, rating_specialist_score = NULL, rating_scoped_scores = NULL,
           rating_categories = NULL, rating_scoped_ranks = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL);

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
        SELECT d.team_id, d.league_id, d.in_comp, d.in_spec, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    composite AS (
        SELECT team_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY team_id, league_id
    ),
    spec AS (
        SELECT DISTINCT ON (team_id, league_id)
               team_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY team_id, league_id, zr DESC
    )
    UPDATE team_stats ts SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (team_id, league_id)
    WHERE ts.team_id = c.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = c.league_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    WITH r AS (
        SELECT team_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS crank,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS srank,
               public.rating_score(rating_composite,  AVG(rating_composite)  OVER(), STDDEV_POP(rating_composite)  OVER()) AS cscore,
               public.rating_score(rating_specialist, AVG(rating_specialist) OVER(), STDDEV_POP(rating_specialist) OVER()) AS sscore
        FROM team_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE team_stats ts SET rating_composite_rank = r.crank, rating_specialist_rank = r.srank,
                             rating_composite_score = r.cscore, rating_specialist_score = r.sscore
    FROM r
    WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = r.league_id;

    RETURN v_updated;
END;
$function$;

-- ── 5. Recompute ALL sports/seasons (players + teams) ───────────────────────
ALTER TABLE player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;
ALTER TABLE team_stats   DISABLE TRIGGER trg_percentile_changed_team_stats;
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats WHERE rating_composite IS NOT NULL ORDER BY sport, season LOOP
        PERFORM compute_rating(r.sport, r.season);
    END LOOP;
    FOR r IN SELECT DISTINCT sport, season FROM team_stats WHERE rating_composite IS NOT NULL ORDER BY sport, season LOOP
        PERFORM compute_team_rating(r.sport, r.season);
    END LOOP;
END $$;
ALTER TABLE player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;
ALTER TABLE team_stats   ENABLE TRIGGER trg_percentile_changed_team_stats;

-- ── 6. Gate: scores populated, on the [1,99] scale, average ≈ 50, far fewer 99s ─
DO $$
DECLARE v_n INT; v_avg NUMERIC; v_ge99 INT; v_rank99 INT; v_teamn INT;
BEGIN
    SELECT count(*), round(avg(rating_composite_score),1),
           count(*) FILTER (WHERE rating_composite_score >= 99),
           count(*) FILTER (WHERE rating_composite_rank >= 99)
      INTO v_n, v_avg, v_ge99, v_rank99
    FROM player_stats WHERE sport='FOOTBALL' AND season=2025 AND rating_composite_score IS NOT NULL;
    SELECT count(*) INTO v_teamn FROM team_stats WHERE rating_composite_score IS NOT NULL;

    IF v_n = 0 OR v_teamn = 0 THEN RAISE EXCEPTION '067 FAIL: scores not populated (players=%, teams=%)', v_n, v_teamn; END IF;
    IF v_avg NOT BETWEEN 45 AND 55 THEN RAISE EXCEPTION '067 FAIL: avg score % off 50-center', v_avg; END IF;
    IF v_ge99 >= v_rank99 THEN RAISE EXCEPTION '067 FAIL: score 99-club (%) not smaller than percentile 99-club (%)', v_ge99, v_rank99; END IF;
    RAISE NOTICE '067 OK: magnitude scores live (% players, avg %); 99-club percentile=% -> score=% ; % teams scored',
        v_n, v_avg, v_rank99, v_ge99, v_teamn;
END $$;

COMMIT;

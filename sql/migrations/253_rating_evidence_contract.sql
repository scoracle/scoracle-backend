-- Rating evidence: a percentile is a rank, not a fallback standardized score.
-- Based on production definitions retrieved 2026-09-14. Requires migration 252.
-- Changes: persist measurement identity; normalize/rank only like measures;
-- retain unranked observations with pct NULL; omit missing values, not measured zeros.
-- All existing derived season/rate bundles are rebuilt in the same transaction.
BEGIN;
SET LOCAL statement_timeout = '10min';

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
    eff AS MATERIALIZED (
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
               d.label, d.measure, d.value, d.in_comp, d.in_spec, d.sign, d.facet,
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
        CROSS JOIN LATERAL public.rating_measurements(
            p_sport,
            CASE WHEN p_sport = 'FOOTBALL'
                 THEN ps.stats || jsonb_strip_nulls(jsonb_build_object(
                          'team_opp_possession', topp.opp,
                          'league_avg_save_pct', lasp.asp))
                 ELSE ps.stats END,
            p_rate_mode, ps.position) d
        WHERE ps.sport = p_sport AND ps.season = p_season AND ps.stats <> '{}'::jsonb
    ),
    pop AS (
        SELECT label, measure, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp WHERE is_ranked GROUP BY label, measure
    ),
    z AS (
        -- Keep raw measurements even without a comparable ranked population.
        SELECT d.player_id, d.league_id, d.position, d.conference, d.division,
               d.label, d.measure, d.in_comp, d.in_spec, d.sign, d.facet, d.value, d.is_ranked,
               CASE WHEN p.mean IS NOT NULL THEN COALESCE((d.value - p.mean) / p.sd, 0) END AS zr
        FROM dp d LEFT JOIN pop p USING (label, measure)
        WHERE d.value IS NOT NULL
    ),
    comp_flat AS (
        SELECT player_id, league_id, SUM(sign * zr) FILTER (WHERE in_comp) AS composite
        FROM z GROUP BY player_id, league_id
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
        SELECT DISTINCT player_id, league_id, is_ranked FROM dp
    ),
    scored AS (
        -- True percentiles, within the eligible population of the same measurement.
        SELECT player_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,
               CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure) > min(sign*zr) OVER (PARTITION BY label, measure) THEN ROUND((percent_rank() OVER (PARTITION BY label, measure ORDER BY sign * zr ASC))::numeric * 100, 1) END AS pct,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') AND position IS NOT NULL
                    THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position) > min(sign*zr) OVER (PARTITION BY label, measure, position) THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position ORDER BY sign*zr ASC))::numeric*100,1) END END AS pct_position,
               CASE WHEN p_sport IN ('NFL','NBA') AND position IS NOT NULL
                    THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position, conference) > min(sign*zr) OVER (PARTITION BY label, measure, position, conference) THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position, conference ORDER BY sign*zr ASC))::numeric*100,1) END END AS pct_conference,
               CASE WHEN p_sport='NFL' AND position IS NOT NULL
                    THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position, division) > min(sign*zr) OVER (PARTITION BY label, measure, position, division) THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position, division ORDER BY sign*zr ASC))::numeric*100,1) END END AS pct_division,
               CASE WHEN p_sport='FOOTBALL' AND position IS NOT NULL
                    THEN CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure, position, league_id) > min(sign*zr) OVER (PARTITION BY label, measure, position, league_id) THEN ROUND((percent_rank() OVER (PARTITION BY label, measure, position, league_id ORDER BY sign*zr ASC))::numeric*100,1) END END AS pct_league
        FROM z WHERE is_ranked AND zr IS NOT NULL
        UNION ALL
        -- Ineligible samples retain measurements, never fabricated percentile ranks.
        SELECT u.player_id, u.league_id, u.label, u.measure, u.in_comp, u.in_spec, u.sign, u.facet, u.value, u.zr,
               NULL::numeric AS pct,
               NULL::numeric AS pct_position, NULL::numeric AS pct_conference,
               NULL::numeric AS pct_division, NULL::numeric AS pct_league
        FROM z u WHERE NOT u.is_ranked OR u.zr IS NULL
    ),
    bd AS (
        -- (mig 221) the specialty flag leaves the datapoint with the concept. A client
        -- that ever wants a hero row can take max(z); the Scout names standouts in prose.
        -- (Spelling the retired key out here would trip this migration's own proof gate.)
        SELECT s.player_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'measure', s.measure, 'eligible', r.is_ranked,
                   'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet,
                   'scoped_pct', jsonb_strip_nulls(jsonb_build_object(
                       'position', s.pct_position, 'conference', s.pct_conference,
                       'division', s.pct_division, 'league', s.pct_league))
               ) ORDER BY s.label) AS breakdown
        FROM scored s JOIN rk r USING (player_id, league_id)
        GROUP BY s.player_id, s.league_id
    ),
    base AS (
        -- (mig 221) the specialist CTE was an INNER join here; without it an entity is
        -- rated on its composite alone, which is what the rating always was.
        SELECT c.player_id, c.league_id,
               ROUND(c.composite, 4) AS composite,
               bd.breakdown, (rk.is_ranked AND c.composite IS NOT NULL) AS is_ranked
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
$function$
;

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
       SET rating = NULL, rating_rank = NULL, rating_score = NULL,
           rating_scoped_scores = NULL,
           rating_breakdown = NULL, rating_scoped_ranks = NULL, rating_modes = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating IS NOT NULL OR rating_rank IS NOT NULL OR rating_modes IS NOT NULL OR rating_breakdown IS NOT NULL);

    FOREACH v_mode IN ARRAY v_modes LOOP
        IF v_mode = 'total' THEN
            WITH b AS MATERIALIZED (
                SELECT * FROM _compute_rating_bundle(p_sport, p_season, 'total')
            )
            UPDATE player_stats ps SET
                rating               = b.composite,
                rating_rank          = b.composite_rank,
                rating_score         = b.composite_score,
                rating_breakdown     = b.breakdown,
                rating_scoped_ranks  = b.scoped_ranks,
                rating_scoped_scores = b.scoped_scores
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
                    v_mode,
                    jsonb_build_object(
                        'rating',        b.composite,
                        'rating_rank',   b.composite_rank,
                        'rating_score',  b.composite_score,
                        'breakdown',     b.breakdown,
                        'scoped_ranks',  b.scoped_ranks,
                        'scoped_scores', b.scoped_scores
                    ))
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
        END IF;
    END LOOP;

    RETURN v_updated;
END;
$function$
;

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
       AND (rating IS NOT NULL OR rating_rank IS NOT NULL OR rating_breakdown IS NOT NULL);

    DROP TABLE IF EXISTS _team_dp;
    CREATE TEMP TABLE _team_dp (
        team_id INTEGER, league_id INTEGER, label TEXT, measure TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    INSERT INTO _team_dp
    SELECT ts.team_id, COALESCE(ts.league_id, 0),
           dp.label, dp.measure, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM team_stats ts
    CROSS JOIN LATERAL public.rating_measurements_team(p_sport, ts.stats) dp
    WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats <> '{}'::jsonb AND dp.value IS NOT NULL;

    WITH pop AS (
        SELECT label, measure, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label, measure
    ),
    z AS (
        -- mean IS NULL = era-dead label; see _compute_rating_bundle.
        SELECT d.team_id, d.league_id, d.in_comp, d.sign, d.label, d.measure,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label, measure)
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
        SELECT label, measure, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label, measure
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.label, d.measure, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label, measure)
        WHERE p.mean IS NOT NULL
    ),
    scored AS (
        SELECT team_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,
               CASE WHEN max(sign*zr) OVER (PARTITION BY label, measure) > min(sign*zr) OVER (PARTITION BY label, measure) THEN ROUND((percent_rank() OVER (PARTITION BY label, measure ORDER BY sign * zr ASC))::numeric * 100, 1) END AS pct
        FROM z
    ),
    agg AS (
        SELECT s.team_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'measure', s.measure, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
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
$function$
;

-- Re-derive all existing seasons from authoritative statistics. Do not infer the
-- measurement behind historical breakdowns using a new formula. This also removes
-- legacy fallback percentiles from every total and rate-mode bundle atomically.
DO $rebuild$
DECLARE r record;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM public.player_stats ORDER BY sport, season LOOP
        PERFORM public.compute_rating(r.sport, r.season);
    END LOOP;
    FOR r IN SELECT DISTINCT sport, season FROM public.team_stats ORDER BY sport, season LOOP
        PERFORM public.compute_team_rating(r.sport, r.season);
    END LOOP;
END $rebuild$;

DO $contract$
BEGIN
 IF EXISTS (SELECT 1 FROM public.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
            WHERE d->>'eligible'='false' AND d->>'pct' IS NOT NULL)
 THEN RAISE EXCEPTION 'Unranked player has a percentile'; END IF;
 IF EXISTS (SELECT 1 FROM public.player_stats ps CROSS JOIN LATERAL jsonb_each(ps.rating_modes) m
            CROSS JOIN LATERAL jsonb_array_elements(m.value->'breakdown') d
            WHERE d->>'eligible'='false' AND d->>'pct' IS NOT NULL)
 THEN RAISE EXCEPTION 'Unranked rate mode has a percentile'; END IF;
 IF EXISTS (SELECT 1 FROM public.player_stats ps CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
            WHERE d->>'value' IS NULL OR COALESCE(d->>'measure','')='')
 THEN RAISE EXCEPTION 'Player measure missing identity or value'; END IF;
 IF EXISTS (SELECT 1 FROM public.team_stats ts CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
            WHERE d->>'value' IS NULL OR COALESCE(d->>'measure','')='')
 THEN RAISE EXCEPTION 'Team measure missing identity or value'; END IF;
END $contract$;

INSERT INTO public.schema_migrations(version) VALUES ('253_rating_evidence_contract') ON CONFLICT DO NOTHING;
COMMIT;

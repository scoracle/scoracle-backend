-- 142_peak_scouting_reframe.sql
-- Wave 5 / F1+F11: PEAK is scouting-report context from the full metric spread,
-- not a pre-labeled specialist-credit axis. Physical rating_specialist* columns
-- remain because the read layer aliases them as rating_peak*; the JSON surfaces
-- stop carrying old specialist labels.

CREATE OR REPLACE FUNCTION public.rating_breakdown_without_specialty(p_breakdown jsonb)
RETURNS jsonb
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE
        WHEN p_breakdown IS NULL THEN NULL
        ELSE COALESCE((
            SELECT jsonb_agg(e - 'is_specialty' ORDER BY ord)
            FROM jsonb_array_elements(p_breakdown) WITH ORDINALITY AS x(e, ord)
        ), '[]'::jsonb)
    END;
$$;

CREATE OR REPLACE FUNCTION public.rating_mode_peak_payload(p_mode jsonb)
RETURNS jsonb
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE
        WHEN p_mode IS NULL THEN NULL
        ELSE (p_mode - 'specialist' - 'specialist_rank' - 'specialist_score' - 'specialty' - 'breakdown')
             || jsonb_strip_nulls(jsonb_build_object(
                    'peak',       COALESCE(p_mode->'peak',       p_mode->'specialist'),
                    'peak_rank',  COALESCE(p_mode->'peak_rank',  p_mode->'specialist_rank'),
                    'peak_score', COALESCE(p_mode->'peak_score', p_mode->'specialist_score'),
                    'peak_label', COALESCE(p_mode->'peak_label', p_mode->'specialty'),
                    'breakdown',  public.rating_breakdown_without_specialty(p_mode->'breakdown')
                ))
    END;
$$;

UPDATE public.player_stats
   SET rating_breakdown = public.rating_breakdown_without_specialty(rating_breakdown)
 WHERE rating_breakdown IS NOT NULL
   AND EXISTS (
       SELECT 1 FROM jsonb_array_elements(rating_breakdown) e
       WHERE e ? 'is_specialty'
   );

UPDATE public.team_stats
   SET rating_breakdown = public.rating_breakdown_without_specialty(rating_breakdown)
 WHERE rating_breakdown IS NOT NULL
   AND EXISTS (
       SELECT 1 FROM jsonb_array_elements(rating_breakdown) e
       WHERE e ? 'is_specialty'
   );

UPDATE public.player_stats ps
   SET rating_modes = (
       SELECT jsonb_object_agg(k, public.rating_mode_peak_payload(v))
       FROM jsonb_each(ps.rating_modes) AS m(k, v)
   )
 WHERE ps.rating_modes IS NOT NULL;

CREATE OR REPLACE FUNCTION public.compute_rating(p_sport text, p_season integer) RETURNS integer
    LANGUAGE plpgsql
    AS $$
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
                rating_breakdown       = public.rating_breakdown_without_specialty(b.breakdown),
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
                    v_mode,
                    public.rating_mode_peak_payload(jsonb_build_object(
                        'composite',        b.composite,
                        'composite_rank',   b.composite_rank,
                        'composite_score',  b.composite_score,
                        'peak',             b.specialist,
                        'peak_rank',        b.specialist_rank,
                        'peak_score',       b.specialist_score,
                        'peak_label',       b.specialty,
                        'breakdown',        b.breakdown,
                        'scoped_ranks',     b.scoped_ranks,
                        'scoped_scores',    b.scoped_scores
                    )))
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
        END IF;
    END LOOP;

    RETURN v_updated;
END;
$$;

CREATE OR REPLACE FUNCTION public.compute_team_rating(p_sport text, p_season integer) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_composite_score = NULL, rating_specialist_score = NULL, rating_scoped_scores = NULL,
           rating_categories = NULL, rating_scoped_ranks = NULL, rating_breakdown = NULL
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

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
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
      AND COALESCE(ts.league_id, 0) = a.league_id AND ts.rating_composite IS NOT NULL;

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
$$;

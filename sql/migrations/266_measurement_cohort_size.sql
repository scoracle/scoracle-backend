-- Carry the comparison population with each measurement.
-- A percentile without its cohort size invites magnitude guessing: "96th percentile"
-- of 8 eligible profiles and of 800 are different claims, and the model had no way to
-- know which. The population was already computed (pop: mean, spread over ranked rows
-- per label/measure); this migration persists its size alongside each stored datapoint.
--
-- Parity: purely additive. Every existing breakdown field, rating, rank, score, scoped
-- value and z is unchanged; the only breakdown delta is the new `cohort` key.
-- Per-measure source coverage (missing vs provider-suppressed zero) stays out of scope:
-- that is the separately-unresolved provider-contract item.
BEGIN;
SET LOCAL statement_timeout = '30min';

-- Parity capture: everything except the new `cohort` key.
CREATE TEMP TABLE _player_cohort_parity AS
SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
       rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
       (SELECT jsonb_agg(d - 'cohort' ORDER BY ord)
          FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_cohort,
       (SELECT jsonb_object_agg(m.key, jsonb_build_object(
                    'rating', m.value->'rating',
                    'rating_rank', m.value->'rating_rank',
                    'rating_score', m.value->'rating_score',
                    'breakdown', (SELECT jsonb_agg(d - 'cohort' ORDER BY ord)
                                    FROM jsonb_array_elements(m.value->'breakdown')
                                      WITH ORDINALITY b(d, ord)),
                    'scoped_ranks', m.value->'scoped_ranks',
                    'scoped_scores', m.value->'scoped_scores'))
          FROM jsonb_each(rating_modes) m) AS modes_no_cohort
FROM player_stats
WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL OR rating_modes IS NOT NULL;

CREATE TEMP TABLE _team_cohort_parity AS
SELECT sport, season, team_id, COALESCE(league_id, 0) AS league_id,
       rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
       (SELECT jsonb_agg(d - 'cohort' ORDER BY ord)
          FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_cohort
FROM team_stats
WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Player bundle: pop carries its own size; z, scored and the stored breakdown carry it.
-- ---------------------------------------------------------------------------
DO $patch$
DECLARE body text;
BEGIN
    body := pg_get_functiondef('public._compute_rating_bundle(text,integer,text)'::regprocedure);

    IF body NOT LIKE '%NULLIF(STDDEV_POP(value), 0) AS sd%FROM dp WHERE is_ranked GROUP BY label, measure%'
    THEN RAISE EXCEPTION 'Player population shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp WHERE is_ranked GROUP BY label, measure',
        'NULLIF(STDDEV_POP(value), 0) AS sd, COUNT(*) AS cohort
        FROM dp WHERE is_ranked GROUP BY label, measure');

    IF body NOT LIKE '%d.value, d.is_ranked,%'
    THEN RAISE EXCEPTION 'Player z shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'd.value, d.is_ranked,
               CASE WHEN p.mean IS NOT NULL AND p.sd IS NOT NULL',
        'd.value, d.is_ranked, p.cohort,
               CASE WHEN p.mean IS NOT NULL AND p.sd IS NOT NULL');

    IF body NOT LIKE '%SELECT player_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,%'
    THEN RAISE EXCEPTION 'Player scored shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'SELECT player_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,',
        'SELECT player_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr, cohort,');
    body := replace(body,
        'SELECT u.player_id, u.league_id, u.label, u.measure, u.in_comp, u.in_spec, u.sign, u.facet, u.value, u.zr,',
        'SELECT u.player_id, u.league_id, u.label, u.measure, u.in_comp, u.in_spec, u.sign, u.facet, u.value, u.zr, u.cohort,');

    IF body NOT LIKE '%ROUND(s.zr, 4), ''pct'', s.pct,%'
    THEN RAISE EXCEPTION 'Player breakdown shape changed; re-derive the patch'; END IF;
    body := replace(body,
        '''value'', s.value, ''z'', ROUND(s.zr, 4), ''pct'', s.pct,',
        '''value'', s.value, ''z'', ROUND(s.zr, 4), ''pct'', s.pct, ''cohort'', s.cohort,');

    EXECUTE body;
END $patch$;

-- ---------------------------------------------------------------------------
-- Team rating: both passes count their population; the breakdown carries it.
-- ---------------------------------------------------------------------------
DO $patch$
DECLARE body text;
BEGIN
    body := pg_get_functiondef('public.compute_team_rating(text,integer)'::regprocedure);

    IF body NOT LIKE '%NULLIF(STDDEV_POP(value), 0) AS sd%FROM _team_dp GROUP BY label, measure%'
    THEN RAISE EXCEPTION 'Team population shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label, measure',
        'NULLIF(STDDEV_POP(value), 0) AS sd, COUNT(*) AS cohort
        FROM _team_dp GROUP BY label, measure');

    IF body NOT LIKE '%d.in_spec, d.sign, d.facet, d.value,%'
    THEN RAISE EXCEPTION 'Team z shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'd.in_spec, d.sign, d.facet, d.value,
               CASE WHEN p.sd IS NOT NULL',
        'd.in_spec, d.sign, d.facet, d.value, p.cohort,
               CASE WHEN p.sd IS NOT NULL');

    IF body NOT LIKE '%SELECT team_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,%'
    THEN RAISE EXCEPTION 'Team scored shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'SELECT team_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr,',
        'SELECT team_id, league_id, label, measure, in_comp, in_spec, sign, facet, value, zr, cohort,');

    IF body NOT LIKE '%ROUND(s.zr, 4), ''pct'', s.pct,%'
    THEN RAISE EXCEPTION 'Team breakdown shape changed; re-derive the patch'; END IF;
    body := replace(body,
        '''value'', s.value, ''z'', ROUND(s.zr, 4), ''pct'', s.pct,',
        '''value'', s.value, ''z'', ROUND(s.zr, 4), ''pct'', s.pct, ''cohort'', s.cohort,');

    EXECUTE body;
END $patch$;

-- ---------------------------------------------------------------------------
-- Rebuild every stored bundle from authoritative statistics.
-- ---------------------------------------------------------------------------
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

-- ---------------------------------------------------------------------------
-- Parity proof: everything except the new `cohort` key is byte-identical.
-- ---------------------------------------------------------------------------
DO $parity$
DECLARE mismatch bigint;
BEGIN
    WITH rebuilt AS (
        SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
               rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
               (SELECT jsonb_agg(d - 'cohort' ORDER BY ord)
                  FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_cohort,
               (SELECT jsonb_object_agg(m.key, jsonb_build_object(
                            'rating', m.value->'rating',
                            'rating_rank', m.value->'rating_rank',
                            'rating_score', m.value->'rating_score',
                            'breakdown', (SELECT jsonb_agg(d - 'cohort' ORDER BY ord)
                                            FROM jsonb_array_elements(m.value->'breakdown')
                                              WITH ORDINALITY b(d, ord)),
                            'scoped_ranks', m.value->'scoped_ranks',
                            'scoped_scores', m.value->'scoped_scores'))
                  FROM jsonb_each(rating_modes) m) AS modes_no_cohort
        FROM player_stats
        WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL OR rating_modes IS NOT NULL
    )
    SELECT count(*) INTO mismatch
    FROM _player_cohort_parity b
    LEFT JOIN rebuilt r USING (sport, season, player_id, league_id)
    WHERE r.sport IS NULL
       OR r.rating IS DISTINCT FROM b.rating
       OR r.rating_rank IS DISTINCT FROM b.rating_rank
       OR r.rating_score IS DISTINCT FROM b.rating_score
       OR r.rating_scoped_ranks IS DISTINCT FROM b.rating_scoped_ranks
       OR r.rating_scoped_scores IS DISTINCT FROM b.rating_scoped_scores
       OR r.breakdown_no_cohort IS DISTINCT FROM b.breakdown_no_cohort
       OR r.modes_no_cohort IS DISTINCT FROM b.modes_no_cohort;
    IF mismatch > 0 THEN
        RAISE EXCEPTION 'Player cohort rebuild broke parity for % rows', mismatch;
    END IF;

    WITH rebuilt AS (
        SELECT sport, season, team_id, COALESCE(league_id, 0) AS league_id,
               rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
               (SELECT jsonb_agg(d - 'cohort' ORDER BY ord)
                  FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_cohort
        FROM team_stats
        WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL
    )
    SELECT count(*) INTO mismatch
    FROM _team_cohort_parity b
    LEFT JOIN rebuilt r USING (sport, season, team_id, league_id)
    WHERE r.sport IS NULL
       OR r.rating IS DISTINCT FROM b.rating
       OR r.rating_rank IS DISTINCT FROM b.rating_rank
       OR r.rating_score IS DISTINCT FROM b.rating_score
       OR r.rating_scoped_ranks IS DISTINCT FROM b.rating_scoped_ranks
       OR r.rating_scoped_scores IS DISTINCT FROM b.rating_scoped_scores
       OR r.breakdown_no_cohort IS DISTINCT FROM b.breakdown_no_cohort;
    IF mismatch > 0 THEN
        RAISE EXCEPTION 'Team cohort rebuild broke parity for % rows', mismatch;
    END IF;
END $parity$;

-- ---------------------------------------------------------------------------
-- Contract: a percentile always names its population size, and the population is
-- a comparison (at least two distinct observations).
-- ---------------------------------------------------------------------------
DO $contract$
BEGIN
    IF EXISTS (
        SELECT 1 FROM player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE d->>'pct' IS NOT NULL AND (d->>'cohort') IS NULL
    ) THEN RAISE EXCEPTION 'Ranked player observation lost its comparison population'; END IF;
    IF EXISTS (
        SELECT 1 FROM player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE d->>'pct' IS NOT NULL AND (d->>'cohort')::numeric < 2
    ) THEN RAISE EXCEPTION 'Percentile stored against a single-observation population'; END IF;
    IF EXISTS (
        SELECT 1 FROM team_stats ts
        CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE d->>'pct' IS NOT NULL AND (d->>'cohort') IS NULL
    ) THEN RAISE EXCEPTION 'Ranked team observation lost its comparison population'; END IF;
END $contract$;

INSERT INTO public.schema_migrations(version) VALUES ('266_measurement_cohort_size') ON CONFLICT DO NOTHING;
COMMIT;

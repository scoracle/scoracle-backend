-- Withhold quality z where the comparison spread is undefined.
-- The 253 standardization coalesced a division by an unavailable spread to z=0: a cohort
-- population of one (STDDEV_POP = 0, NULLIF → NULL) or an all-tied population stored
-- z = 0.00, and the Scout legend tells the model "0 is average" — so a lone eligible entity
-- read as exactly average distance from a spread that was never defined. Percentiles were
-- already withheld for the same populations; this migration withholds z the same way.
-- A degenerate comparison is unknown evidence, not a neutral distance.
--
-- Parity: for populations with a defined spread, every stored z is unchanged. Degenerate
-- rows change only 0 → NULL, and the composite is unchanged because those rows contributed
-- sign * 0 to SUM. The event-starline composite paths are intentionally untouched: their z
-- never reaches a model prompt and their zero-contribution semantics are unchanged.
--
-- The two function bodies are patched surgically from their live definitions so every other
-- byte stays identical to applied history.
BEGIN;
SET LOCAL statement_timeout = '30min';

-- Parity capture: everything except breakdown `z`, which is the only field the fix may change.
CREATE TEMP TABLE _player_z_parity AS
SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
       rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
       (SELECT jsonb_agg(d - 'z' ORDER BY ord)
          FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_z,
       (SELECT jsonb_object_agg(m.key, jsonb_build_object(
                    'rating', m.value->'rating',
                    'rating_rank', m.value->'rating_rank',
                    'rating_score', m.value->'rating_score',
                    'breakdown', (SELECT jsonb_agg(d - 'z' ORDER BY ord)
                                    FROM jsonb_array_elements(m.value->'breakdown')
                                      WITH ORDINALITY b(d, ord)),
                    'scoped_ranks', m.value->'scoped_ranks',
                    'scoped_scores', m.value->'scoped_scores'))
          FROM jsonb_each(rating_modes) m) AS modes_no_z,
       (SELECT jsonb_agg(jsonb_build_object('label', d->>'label', 'measure', d->>'measure',
                                            'value', d->>'value', 'pct', d->>'pct',
                                            'z', d->>'z', 'eligible', d->>'eligible')
                          ORDER BY ord)
          FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS elements_raw
FROM player_stats
WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL OR rating_modes IS NOT NULL;

CREATE TEMP TABLE _team_z_parity AS
SELECT sport, season, team_id, COALESCE(league_id, 0) AS league_id,
       rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
       (SELECT jsonb_agg(d - 'z' ORDER BY ord)
          FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_z
FROM team_stats
WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Player bundle: withhold z when the population has no defined spread. Rows with
-- undefined zr still reach the unranked union branch, so the datapoint (value,
-- pct NULL) survives with z withheld, not dropped.
-- ---------------------------------------------------------------------------
DO $patch$
DECLARE body text;
BEGIN
    body := pg_get_functiondef('public._compute_rating_bundle(text,integer,text)'::regprocedure);
    IF body NOT LIKE '%CASE WHEN p.mean IS NOT NULL THEN COALESCE((d.value - p.mean) / p.sd, 0) END AS zr%'
    THEN RAISE EXCEPTION 'Player z fallback line changed before 265; re-derive the patch'; END IF;
    body := replace(body,
        'CASE WHEN p.mean IS NOT NULL THEN COALESCE((d.value - p.mean) / p.sd, 0) END AS zr',
        'CASE WHEN p.mean IS NOT NULL AND p.sd IS NOT NULL THEN (d.value - p.mean) / p.sd END AS zr');
    -- A withheld z must keep contributing its former sign * 0 to the composite, or an
    -- all-degenerate profile would lose its composite entirely (SUM of empty = NULL).
    IF body NOT LIKE '%SUM(sign * zr) FILTER (WHERE in_comp) AS composite%'
    THEN RAISE EXCEPTION 'Player composite shape changed before 265; re-derive the patch'; END IF;
    body := replace(body,
        'SUM(sign * zr) FILTER (WHERE in_comp) AS composite',
        'SUM(sign * COALESCE(zr, 0)) FILTER (WHERE in_comp) AS composite');
    EXECUTE body;
END $patch$;

-- Team rating: the composite pass keeps its join semantics (a degenerate row contributed
-- sign * 0, so withholding it changes no sum); both z CTEs stop manufacturing 0.
DO $patch$
DECLARE body text;
BEGIN
    body := pg_get_functiondef('public.compute_team_rating(text,integer)'::regprocedure);
    IF body NOT LIKE '%COALESCE((d.value - p.mean) / p.sd, 0) AS zr%'
    THEN RAISE EXCEPTION 'Team z fallback shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'COALESCE((d.value - p.mean) / p.sd, 0) AS zr',
        'CASE WHEN p.sd IS NOT NULL THEN (d.value - p.mean) / p.sd END AS zr');
    IF body NOT LIKE '%SUM(sign * zr) AS composite%'
    THEN RAISE EXCEPTION 'Team composite shape changed; re-derive the patch'; END IF;
    body := replace(body,
        'SUM(sign * zr) AS composite',
        'SUM(sign * COALESCE(zr, 0)) AS composite');
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
-- Parity proof: ratings, ranks, scores and scoped values unchanged; breakdown
-- members identical except z, which may only go 0 → NULL on a degenerate row.
-- ---------------------------------------------------------------------------
DO $parity$
DECLARE mismatch bigint;
BEGIN
    WITH rebuilt AS (
        SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
               rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
               (SELECT jsonb_agg(d - 'z' ORDER BY ord)
                  FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_z,
               (SELECT jsonb_object_agg(m.key, jsonb_build_object(
                            'rating', m.value->'rating',
                            'rating_rank', m.value->'rating_rank',
                            'rating_score', m.value->'rating_score',
                            'breakdown', (SELECT jsonb_agg(d - 'z' ORDER BY ord)
                                            FROM jsonb_array_elements(m.value->'breakdown')
                                              WITH ORDINALITY b(d, ord)),
                            'scoped_ranks', m.value->'scoped_ranks',
                            'scoped_scores', m.value->'scoped_scores'))
                  FROM jsonb_each(rating_modes) m) AS modes_no_z
        FROM player_stats
        WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL OR rating_modes IS NOT NULL
    )
    SELECT count(*) INTO mismatch
    FROM _player_z_parity b
    LEFT JOIN rebuilt r USING (sport, season, player_id, league_id)
    WHERE r.sport IS NULL
       OR r.rating IS DISTINCT FROM b.rating
       OR r.rating_rank IS DISTINCT FROM b.rating_rank
       OR r.rating_score IS DISTINCT FROM b.rating_score
       OR r.rating_scoped_ranks IS DISTINCT FROM b.rating_scoped_ranks
       OR r.rating_scoped_scores IS DISTINCT FROM b.rating_scoped_scores
       OR r.breakdown_no_z IS DISTINCT FROM b.breakdown_no_z
       OR r.modes_no_z IS DISTINCT FROM b.modes_no_z;
    IF mismatch > 0 THEN
        RAISE EXCEPTION 'Player z rebuild broke parity for % rows', mismatch;
    END IF;

    WITH rebuilt AS (
        SELECT sport, season, team_id, COALESCE(league_id, 0) AS league_id,
               rating, rating_rank, rating_score, rating_scoped_ranks, rating_scoped_scores,
               (SELECT jsonb_agg(d - 'z' ORDER BY ord)
                  FROM jsonb_array_elements(rating_breakdown) WITH ORDINALITY t(d, ord)) AS breakdown_no_z
        FROM team_stats
        WHERE rating IS NOT NULL OR rating_breakdown IS NOT NULL
    )
    SELECT count(*) INTO mismatch
    FROM _team_z_parity b
    LEFT JOIN rebuilt r USING (sport, season, team_id, league_id)
    WHERE r.sport IS NULL
       OR r.rating IS DISTINCT FROM b.rating
       OR r.rating_rank IS DISTINCT FROM b.rating_rank
       OR r.rating_score IS DISTINCT FROM b.rating_score
       OR r.rating_scoped_ranks IS DISTINCT FROM b.rating_scoped_ranks
       OR r.rating_scoped_scores IS DISTINCT FROM b.rating_scoped_scores
       OR r.breakdown_no_z IS DISTINCT FROM b.breakdown_no_z;
    IF mismatch > 0 THEN
        RAISE EXCEPTION 'Team z rebuild broke parity for % rows', mismatch;
    END IF;

    -- Every z change must be exactly 0 -> NULL on an unranked observation. Eligibility
    -- is not part of the exception: an ineligible row of a degenerate population
    -- (display-tier zeros over an all-tied cohort) moves the same way, because
    -- degeneracy is a property of the population, not of the row.
    WITH old AS (
        SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
               (SELECT jsonb_object_agg(d->>'label', jsonb_build_object(
                            'z', d->>'z', 'pct', d->>'pct', 'eligible', d->>'eligible'))
                  FROM jsonb_array_elements(elements_raw) e(d)
                 WHERE d->>'label' IS NOT NULL) AS by_label
        FROM _player_z_parity
    ),
    cur AS (
        SELECT sport, season, player_id, COALESCE(league_id, 0) AS league_id,
               (SELECT jsonb_object_agg(d->>'label', jsonb_build_object(
                            'z', d->>'z', 'pct', d->>'pct', 'eligible', d->>'eligible'))
                  FROM jsonb_array_elements(rating_breakdown) d
                 WHERE d->>'label' IS NOT NULL) AS by_label
        FROM player_stats
        WHERE rating_breakdown IS NOT NULL
    ),
    changed AS (
        SELECT oe, ne
        FROM old o
        JOIN cur c USING (sport, season, player_id, league_id)
        CROSS JOIN LATERAL jsonb_object_keys(c.by_label) AS ok(key)
        JOIN LATERAL (SELECT o.by_label->ok.key AS oe, c.by_label->ok.key AS ne) kv ON TRUE
        WHERE o.by_label IS NOT NULL AND c.by_label IS NOT NULL
    )
    SELECT count(*) INTO mismatch
    FROM changed
    WHERE (ne->>'z') IS DISTINCT FROM (oe->>'z')
      AND NOT ((oe->>'z')::numeric = 0
               AND (ne->>'z') IS NULL
               AND (oe->>'pct') IS NULL
               AND (ne->>'pct') IS NULL);
    IF mismatch > 0 THEN
        RAISE EXCEPTION 'Stored z moved outside the degeneracy exception for % elements', mismatch;
    END IF;
END $parity$;

-- ---------------------------------------------------------------------------
-- Contract: a ranked observation always has both pct and z; an eligible
-- observation without a percentile carries no fabricated distance.
-- ---------------------------------------------------------------------------
DO $contract$
BEGIN
    IF EXISTS (
        SELECT 1 FROM player_stats ps
        CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) d
        WHERE (d->>'eligible') = 'true' AND d->>'pct' IS NULL AND d->>'z' IS NOT NULL
    ) THEN RAISE EXCEPTION 'Ranked-eligible observation carries z without a percentile'; END IF;
    IF EXISTS (
        SELECT 1 FROM player_stats ps
        CROSS JOIN LATERAL jsonb_each(ps.rating_modes) m
        CROSS JOIN LATERAL jsonb_array_elements(m.value->'breakdown') d
        WHERE (d->>'eligible') = 'true' AND d->>'pct' IS NULL AND d->>'z' IS NOT NULL
    ) THEN RAISE EXCEPTION 'Ranked-eligible rate-mode observation carries z without a percentile'; END IF;
    IF EXISTS (
        SELECT 1 FROM team_stats ts
        CROSS JOIN LATERAL jsonb_array_elements(ts.rating_breakdown) d
        WHERE COALESCE((d->>'z')::numeric, 1) = 0 AND d->>'pct' IS NULL AND d->>'value' IS NOT NULL
    ) THEN RAISE EXCEPTION 'Team breakdown still stores a fabricated zero z'; END IF;
END $contract$;

INSERT INTO public.schema_migrations(version) VALUES ('265_withhold_degenerate_z') ON CONFLICT DO NOTHING;
COMMIT;

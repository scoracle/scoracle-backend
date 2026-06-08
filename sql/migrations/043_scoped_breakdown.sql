-- ============================================================================
-- 043_scoped_breakdown.sql
-- Position-scoped per-datapoint percentiles in the rating breakdown, so the
-- frontend's "By Position" scope re-ranks the PIZZA SLICES within the position
-- cohort — the same way Per-X re-rates them — not just the headline composite.
--
-- Adds `scoped_pct` to every rating_breakdown element: { "position": <pct> }, the
-- percent_rank of that datapoint's sign*z WITHIN (label, position), parallel to
-- the existing positionless `pct`. Computed for the default AND every rate mode
-- (rating_modes), so per-X × position compose. Players only (position scope);
-- teams (conference/division/league slice scope) are a follow-on.
--
-- STRICTLY ADDITIVE: only `_compute_rating_bundle` changes (adds position to its
-- datapoint set + scoped_pct to the breakdown). compute_rating is unchanged — it
-- already loops modes through the bundle; we just re-run it. The composite /
-- ranks / specialist / z / pct math is untouched, so the parity gate stays green
-- (it compares the breakdown MODULO is_specialty AND the new scoped_pct).
--
-- Served automatically: the sparkline statement already passes rating_breakdown
-- (and rating_modes) through row_to_json, so scoped_pct rides along — no Go change.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/043_scoped_breakdown.sql
-- ============================================================================

BEGIN;

-- Parity snapshot (pre-recompute).
CREATE TEMP TABLE _parity_before_043 ON COMMIT DROP AS
SELECT player_id, sport, season, COALESCE(league_id, 0) AS league_id,
       rating_composite, rating_specialist, rating_specialty,
       rating_composite_rank, rating_specialist_rank,
       rating_breakdown, rating_scoped_ranks, rating_modes
FROM player_stats
WHERE sport IN ('FOOTBALL', 'NBA', 'NFL');

-- ── _compute_rating_bundle — 042's body + position-scoped breakdown pct ──────
CREATE OR REPLACE FUNCTION _compute_rating_bundle(p_sport TEXT, p_season INTEGER, p_rate_mode TEXT)
RETURNS TABLE (
    player_id INTEGER, league_id INTEGER,
    composite NUMERIC, composite_rank NUMERIC,
    specialist NUMERIC, specialist_rank NUMERIC, specialty TEXT,
    breakdown JSONB, scoped_ranks JSONB
)
LANGUAGE sql STABLE AS $$
    WITH dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet
        FROM player_stats ps
        CROSS JOIN LATERAL rating_datapoints(p_sport, ps.stats, p_rate_mode) d
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND CASE p_sport
                WHEN 'NBA'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0) >= 30
                                 AND COALESCE((ps.stats->>'minutes')::numeric, 0)      >= 20
                WHEN 'FOOTBALL' THEN COALESCE((ps.stats->>'appearances')::numeric, 0)  >= 15
                WHEN 'NFL'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0)  >= 8
                ELSE FALSE
              END
    ),
    pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.position, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
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
               -- Position-scoped percentile: same sign*z, ranked within the
               -- player's position cohort. NULL for players with no position.
               CASE WHEN position IS NULL THEN NULL
                    ELSE ROUND((percent_rank() OVER (PARTITION BY label, position ORDER BY sign * zr ASC))::numeric * 100, 1)
               END AS pct_position
        FROM z
    ),
    bd AS (
        SELECT s.player_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label,
                   'value', s.value,
                   'z',     ROUND(s.zr, 4),
                   'pct',   s.pct,
                   'in_comp', s.in_comp,
                   'in_spec', s.in_spec,
                   'sign',  s.sign,
                   'facet', s.facet,
                   'is_specialty', (sp.specialty IS NOT DISTINCT FROM s.label),
                   -- Per-scope percentiles for the slice re-rank (empty {} when
                   -- the player has no position). The frontend picks scoped_pct
                   -- [scope] when a scope is active, else the positionless pct.
                   'scoped_pct', jsonb_strip_nulls(jsonb_build_object('position', s.pct_position))
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
               ROUND((percent_rank() OVER (PARTITION BY ps.position ORDER BY b.composite ASC))::numeric * 100, 1) AS pos_pct
        FROM base b
        JOIN player_stats ps
          ON ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
         AND COALESCE(ps.league_id, 0) = b.league_id
        WHERE ps.position IS NOT NULL
    )
    SELECT b.player_id, b.league_id,
           b.composite, r.composite_rank,
           b.specialist, r.specialist_rank, b.specialty,
           b.breakdown,
           CASE WHEN sc.pos_pct IS NOT NULL
                THEN jsonb_build_object('position', sc.pos_pct) END AS scoped_ranks
    FROM base b
    JOIN ranks r USING (player_id, league_id)
    LEFT JOIN scoped sc USING (player_id, league_id);
$$;

-- ── Recompute every player (sport, season) — refills breakdown with scoped_pct ─
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats
             WHERE sport IN ('FOOTBALL', 'NBA', 'NFL') ORDER BY sport, season LOOP
        PERFORM compute_rating(r.sport, r.season);
    END LOOP;
END $$;

-- ── Parity gate — breakdown compared MODULO is_specialty AND scoped_pct ──────
CREATE OR REPLACE FUNCTION _bd_sans_spec(p JSONB) RETURNS JSONB LANGUAGE sql IMMUTABLE AS $fn$
    SELECT CASE WHEN p IS NULL THEN NULL ELSE
        (SELECT jsonb_agg((e - 'is_specialty' - 'scoped_pct') ORDER BY ord)
         FROM jsonb_array_elements(p) WITH ORDINALITY t(e, ord)) END;
$fn$;

DO $$
DECLARE
    v_drift     BIGINT;
    v_spec_real BIGINT;
    v_scoped    BIGINT;
    v_modes_sc  BIGINT;
BEGIN
    SELECT count(*) INTO v_drift
    FROM _parity_before_043 b
    JOIN player_stats a
      ON a.player_id = b.player_id AND a.sport = b.sport AND a.season = b.season
     AND COALESCE(a.league_id, 0) = b.league_id
    WHERE a.rating_composite       IS DISTINCT FROM b.rating_composite
       OR a.rating_specialist      IS DISTINCT FROM b.rating_specialist
       OR a.rating_composite_rank  IS DISTINCT FROM b.rating_composite_rank
       OR a.rating_specialist_rank IS DISTINCT FROM b.rating_specialist_rank
       OR _bd_sans_spec(a.rating_breakdown) IS DISTINCT FROM _bd_sans_spec(b.rating_breakdown)
       OR a.rating_scoped_ranks    IS DISTINCT FROM b.rating_scoped_ranks;
    IF v_drift > 0 THEN
        RAISE EXCEPTION 'PARITY FAILURE (043): % rows drifted in non-scoped_pct fields — aborting', v_drift;
    END IF;

    -- Specialty label changes must be tie-only (same specialist value).
    SELECT count(*) INTO v_spec_real
    FROM _parity_before_043 b JOIN player_stats a
      ON a.player_id=b.player_id AND a.sport=b.sport AND a.season=b.season AND COALESCE(a.league_id,0)=b.league_id
    WHERE a.rating_specialty IS DISTINCT FROM b.rating_specialty
      AND a.rating_specialist IS DISTINCT FROM b.rating_specialist;
    IF v_spec_real > 0 THEN
        RAISE EXCEPTION 'PARITY FAILURE (043): % rows changed specialty with a different specialist value', v_spec_real;
    END IF;

    -- Smoke: scoped_pct.position actually populated in the default breakdown.
    SELECT count(*) INTO v_scoped FROM player_stats
        WHERE sport IN ('FOOTBALL','NBA','NFL')
          AND rating_breakdown IS NOT NULL
          AND EXISTS (SELECT 1 FROM jsonb_array_elements(rating_breakdown) e
                      WHERE e->'scoped_pct' ? 'position');
    -- And in the alternate rate modes (per-X × position must compose).
    SELECT count(*) INTO v_modes_sc FROM player_stats
        WHERE sport IN ('FOOTBALL','NBA','NFL')
          AND rating_modes IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM jsonb_each(rating_modes) m
              CROSS JOIN LATERAL jsonb_array_elements(m.value->'breakdown') e
              WHERE e->'scoped_pct' ? 'position');
    RAISE NOTICE 'PARITY OK (043): non-scoped fields unchanged. default breakdowns w/ scoped_pct.position: %, mode breakdowns w/ scoped_pct.position: %', v_scoped, v_modes_sc;
END $$;

DROP FUNCTION _bd_sans_spec(JSONB);

COMMIT;

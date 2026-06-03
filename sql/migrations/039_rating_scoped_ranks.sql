-- ============================================================================
-- 039_rating_scoped_ranks.sql
-- SCOPE TOGGLES (re-rank within cohort): precompute the composite percentile
-- within each cohort the entity belongs to, served ready-made.
--   Players : { position }                       (percentile vs same-position)
--   Teams   : { conference, division }  (NBA/NFL) / { league }  (football)
-- The 0-100 positionless rating_composite_rank stays the "All" scope; these are
-- the scoped re-ranks the profile/leaderboard dropdowns switch to.
--
-- STRICTLY ADDITIVE: compute_rating / compute_team_rating copied verbatim from
-- 038 with ONE scoped-ranks pass appended after the existing passes; the initial
-- reset also clears rating_scoped_ranks. Composite/specialist/category math is
-- byte-identical. New column rating_scoped_ranks JSONB on player_stats + team_stats.
-- ============================================================================

BEGIN;

ALTER TABLE player_stats ADD COLUMN IF NOT EXISTS rating_scoped_ranks JSONB;
ALTER TABLE team_stats   ADD COLUMN IF NOT EXISTS rating_scoped_ranks JSONB;

CREATE OR REPLACE FUNCTION compute_rating(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated  INTEGER := 0;
    -- Category-balanced Composite ONLY where players are single-phase (NFL).
    -- NBA + FOOTBALL stay flat-z (multi-phase players). See spec §1.5.
    v_balanced BOOLEAN := (p_sport = 'NFL');
BEGIN
    UPDATE player_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_scoped_ranks = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL);

    DROP TABLE IF EXISTS _rating_dp;
    CREATE TEMP TABLE _rating_dp (
        player_id INTEGER, league_id INTEGER, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    -- Qualified population × the shared datapoint definitions. Floors per §6.
    INSERT INTO _rating_dp
    SELECT ps.player_id, COALESCE(ps.league_id, 0),
           dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM player_stats ps
    CROSS JOIN LATERAL rating_datapoints(p_sport, ps.stats) dp
    WHERE ps.sport = p_sport AND ps.season = p_season
      AND CASE p_sport
            WHEN 'NBA'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0) >= 30
                             AND COALESCE((ps.stats->>'minutes')::numeric, 0)      >= 20
            WHEN 'FOOTBALL' THEN COALESCE((ps.stats->>'appearances')::numeric, 0)  >= 15
            WHEN 'NFL'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0)  >= 8
            ELSE FALSE
          END;

    -- Positionless z, then Composite (flat or facet-balanced) + Specialist (peak + label).
    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _rating_dp GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _rating_dp d JOIN pop p USING (label)
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
    composite AS (
        SELECT player_id, league_id, composite FROM comp_flat  WHERE NOT v_balanced
        UNION ALL
        SELECT player_id, league_id, composite FROM comp_facet WHERE     v_balanced
    ),
    spec AS (
        SELECT DISTINCT ON (player_id, league_id)
               player_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY player_id, league_id, zr DESC
    )
    UPDATE player_stats ps SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (player_id, league_id)
    WHERE ps.player_id = c.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = c.league_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    -- Display ranks: positionless percent_rank (0–100) over the raw z scores —
    -- a friendly 0–100 for the meta card / leaderboard; the raw z stays the engine.
    WITH r AS (
        SELECT player_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating_composite  ASC))::numeric * 100, 1) AS crank,
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS srank
        FROM player_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE player_stats ps SET rating_composite_rank = r.crank, rating_specialist_rank = r.srank
    FROM r
    WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = r.league_id;

    -- ── Per-datapoint breakdown (migration 030) ──────────────────────────────
    -- Additive: reuses _rating_dp, writes ONLY rating_breakdown. Does not touch
    -- any of the composite/specialist/rank writes above.
    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _rating_dp GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _rating_dp d JOIN pop p USING (label)
    ),
    scored AS (
        SELECT player_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct
        FROM z
    ),
    peak AS (
        SELECT DISTINCT ON (player_id, league_id) player_id, league_id, label AS spec_label
        FROM z WHERE in_spec
        ORDER BY player_id, league_id, zr DESC
    ),
    agg AS (
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
                   'is_specialty', (pk.spec_label IS NOT DISTINCT FROM s.label)
               ) ORDER BY s.label) AS breakdown
        FROM scored s
        LEFT JOIN peak pk USING (player_id, league_id)
        GROUP BY s.player_id, s.league_id
    )
    UPDATE player_stats ps SET rating_breakdown = a.breakdown
    FROM agg a
    WHERE ps.player_id = a.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = a.league_id
      AND ps.rating_composite IS NOT NULL;

    -- ── Scoped ranks (re-rank within cohort) ─────────────────────────────────
    -- Players: percentile of composite within the POSITION cohort (sport, season),
    -- pooled across leagues like the positionless rating. "All" stays
    -- rating_composite_rank; this is the by-position re-rank the scope dropdown picks.
    WITH pr AS (
        SELECT player_id, league_id,
               ROUND((percent_rank() OVER (PARTITION BY position ORDER BY rating_composite ASC))::numeric * 100, 1) AS pos_pct
        FROM player_stats
        WHERE sport = p_sport AND season = p_season
          AND rating_composite IS NOT NULL AND position IS NOT NULL
    )
    UPDATE player_stats ps SET rating_scoped_ranks =
        jsonb_strip_nulls(jsonb_build_object('position', pr.pos_pct))
    FROM pr
    WHERE ps.player_id = pr.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = pr.league_id;

    RETURN v_updated;
END;
$$;

CREATE OR REPLACE FUNCTION compute_team_rating(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
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

    -- Composite (flat Σz over in_comp) + Specialist (peak in_spec + label). UNCHANGED.
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
               ROUND((percent_rank() OVER (ORDER BY rating_specialist ASC))::numeric * 100, 1) AS srank
        FROM team_stats
        WHERE sport = p_sport AND season = p_season AND rating_composite IS NOT NULL
    )
    UPDATE team_stats ts SET rating_composite_rank = r.crank, rating_specialist_rank = r.srank
    FROM r
    WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = r.league_id;

    -- ── Per-datapoint breakdown (now facet-aware) ────────────────────────────
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
    peak AS (
        SELECT DISTINCT ON (team_id, league_id) team_id, league_id, label AS spec_label
        FROM z WHERE in_spec
        ORDER BY team_id, league_id, zr DESC
    ),
    agg AS (
        SELECT s.team_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label,
                   'value', s.value,
                   'z',     ROUND(s.zr, 4),
                   'pct',   s.pct,
                   'in_comp', s.in_comp,
                   'in_spec', s.in_spec,
                   'sign',  s.sign,
                   'facet', s.facet,
                   'is_specialty', (pk.spec_label IS NOT DISTINCT FROM s.label)
               ) ORDER BY s.facet, s.label) AS breakdown
        FROM scored s
        LEFT JOIN peak pk USING (team_id, league_id)
        GROUP BY s.team_id, s.league_id
    )
    UPDATE team_stats ts SET rating_breakdown = a.breakdown
    FROM agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id
      AND ts.rating_composite IS NOT NULL;

    -- ── Per-category sub-scores (rating_categories) ──────────────────────────
    -- Category z = mean of sign*zr over the IN_COMP terms of each facet (so facets
    -- with different counts are comparable); pct = percent_rank of that within the
    -- (sport, season, facet) population. Display-only facets (discipline/squad)
    -- have no in_comp terms, so only offense/defense produce a category score.
    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.in_comp, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    cat AS (
        SELECT team_id, league_id, facet, AVG(sign * zr) AS cat_z
        FROM z WHERE in_comp GROUP BY team_id, league_id, facet
    ),
    cat_ranked AS (
        SELECT team_id, league_id, facet, cat_z,
               ROUND((percent_rank() OVER (PARTITION BY facet ORDER BY cat_z ASC))::numeric * 100, 1) AS cat_pct
        FROM cat
    ),
    cat_agg AS (
        SELECT team_id, league_id,
               jsonb_object_agg(facet, jsonb_build_object('z', ROUND(cat_z, 4), 'pct', cat_pct)) AS cats
        FROM cat_ranked GROUP BY team_id, league_id
    )
    UPDATE team_stats ts SET rating_categories = a.cats
    FROM cat_agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id
      AND ts.rating_composite IS NOT NULL;

    -- ── Scoped ranks (re-rank within cohort) ─────────────────────────────────
    -- Teams: percentile of composite within conference / division (NBA, NFL) or
    -- league (football). jsonb_strip_nulls drops absent cohorts → football yields
    -- {league}; NBA/NFL yield {conference, division} (+ a redundant league the UI
    -- ignores, since their league_id is uniform).
    WITH tr AS (
        SELECT ts.team_id, ts.league_id, tm.conference, tm.division,
               ROUND((percent_rank() OVER (PARTITION BY tm.conference ORDER BY ts.rating_composite ASC))::numeric * 100, 1) AS conf_pct,
               ROUND((percent_rank() OVER (PARTITION BY tm.division  ORDER BY ts.rating_composite ASC))::numeric * 100, 1) AS div_pct,
               ROUND((percent_rank() OVER (PARTITION BY ts.league_id ORDER BY ts.rating_composite ASC))::numeric * 100, 1) AS league_pct
        FROM team_stats ts JOIN teams tm ON tm.id = ts.team_id AND tm.sport = ts.sport
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.rating_composite IS NOT NULL
    )
    UPDATE team_stats ts SET rating_scoped_ranks = jsonb_strip_nulls(jsonb_build_object(
        'conference', CASE WHEN tr.conference IS NOT NULL THEN tr.conf_pct END,
        'division',   CASE WHEN tr.division   IS NOT NULL THEN tr.div_pct END,
        'league',     tr.league_pct
    ))
    FROM tr
    WHERE ts.team_id = tr.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = tr.league_id;

    RETURN v_updated;
END;
$$;

-- Backfill: re-rate every (sport, season) so rating_scoped_ranks populates.
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats ORDER BY sport, season LOOP
        PERFORM compute_rating(r.sport, r.season);
    END LOOP;
    FOR r IN SELECT DISTINCT sport, season FROM team_stats ORDER BY sport, season LOOP
        PERFORM compute_team_rating(r.sport, r.season);
    END LOOP;
END $$;

COMMIT;

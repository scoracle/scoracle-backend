-- ============================================================================
-- 077_football_position_scope.sql
-- Restore a "By Position" cohort scope for FOOTBALL players.
--
-- Symptom: a football player's Composite-card scope selector only offered
-- "All" (positionless) and "By League" (= position x league). The standalone
-- within-position cohort was hardcoded NFL-only in the engine (migration 067's
-- _compute_rating_bundle pct_position/pos_pct/pos_score, and 058's
-- recalculate_percentiles scoped_percentiles 'position' cohort), so a football
-- player never carried a rating_scoped_ranks.position key. The frontend builds the
-- scope dropdown from those keys (and already maps position -> "By Position"), so
-- the option simply never appeared.
--
-- Fix (additive, backend-only): extend the within-position cohort from 'NFL' to
-- ('NFL','FOOTBALL') in those four branches, then recompute football seasons.
-- "By Position" = ranked within the player's position across ALL leagues — distinct
-- from the positionless "All" and the narrower position x league "By League" (kept).
-- No frontend change; no engine-math change (composite/specialist/ranks untouched —
-- gated below). The two functions are the live defs (dumped via pg_get_functiondef)
-- with only the position-only branches flipped.
--
-- Recompute is superuser + session_replication_role=replica (lock-free; suppresses
-- the percentile_changed NOTIFY storm without ALTER TABLE's ACCESS EXCLUSIVE lock).
--
-- Apply: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/077_football_position_scope.sql
-- ============================================================================

BEGIN;

-- ── 1. recalculate_percentiles — within-position cohort now NFL + FOOTBALL ────────
CREATE OR REPLACE FUNCTION public.recalculate_percentiles(p_sport text, p_season integer, p_inverse_stats text[] DEFAULT ARRAY[]::text[])
 RETURNS TABLE(players_updated integer, teams_updated integer)
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
    v_inverse TEXT[];
BEGIN
    SELECT array_agg(DISTINCT key_name) INTO v_inverse
    FROM (
        SELECT key_name FROM stat_definitions WHERE sport = p_sport AND is_inverse = true
        UNION SELECT unnest(p_inverse_stats)
    ) combined;
    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- Player percentiles (sport-wide, partitioned by ps.position) — UNCHANGED
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ps.player_id, COALESCE(ps.position, 'Unknown') AS position,
               sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps CROSS JOIN stat_keys sk
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, position, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, position, max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_position_group', position, '_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY player_id, position
    )
    UPDATE player_stats ps SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ps.player_id = agg.player_id AND ps.sport = p_sport AND ps.season = p_season;
    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- Team percentiles (no position partitioning) — UNCHANGED
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ts.team_id, sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id, jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY team_id
    )
    UPDATE team_stats ts SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ts.team_id = agg.team_id AND ts.sport = p_sport AND ts.season = p_season;
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    -- Player scoped percentiles — MULTI-SCOPE nested {scope: {key: pct}}
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_scope AS (
        SELECT ps.player_id, ps.league_id, sc.scope_name, sc.cohort_key
        FROM player_stats ps
        LEFT JOIN teams t ON t.id = ps.team_id AND t.sport = ps.sport
        CROSS JOIN LATERAL (VALUES
            ('position',   CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN COALESCE(ps.position,'Unknown') END),
            ('conference', CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(ps.position,'Unknown')||'|'||COALESCE(t.conference,'Unknown') END),
            ('division',   CASE WHEN p_sport='NFL' THEN COALESCE(ps.position,'Unknown')||'|'||COALESCE(t.division,'Unknown') END),
            ('league',     CASE WHEN p_sport='FOOTBALL' THEN COALESCE(ps.position,'Unknown')||'|'||ps.league_id::text END),
            -- positionless baseline for the "All" option on every pizza (the template's
            -- base pct is within-position, so 'all' must carry the sport-wide rank).
            ('all',        'ALL')
        ) sc(scope_name, cohort_key)
        WHERE sc.cohort_key IS NOT NULL AND ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT psc.player_id, psc.league_id, psc.scope_name, psc.cohort_key,
               sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_scope psc
        JOIN player_stats ps ON ps.player_id=psc.player_id AND ps.league_id=psc.league_id
                            AND ps.sport=p_sport AND ps.season=p_season
        CROSS JOIN stat_keys sk
        WHERE ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, league_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
            END AS percentile
        FROM expanded
    ),
    per_scope AS (
        SELECT player_id, league_id, scope_name, jsonb_object_agg(stat_key, percentile) AS scope_pcts
        FROM ranked GROUP BY player_id, league_id, scope_name
    ),
    aggregated AS (
        SELECT player_id, league_id, jsonb_object_agg(scope_name, scope_pcts) AS scoped_json
        FROM per_scope GROUP BY player_id, league_id
    )
    UPDATE player_stats ps SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg WHERE ps.player_id=agg.player_id AND ps.league_id=agg.league_id
                          AND ps.sport=p_sport AND ps.season=p_season;

    -- Team scoped percentiles — MULTI-SCOPE nested {scope: {key: pct}}
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    team_scope AS (
        SELECT ts.team_id, ts.league_id, sc.scope_name, sc.cohort_key
        FROM team_stats ts
        JOIN teams t ON t.id = ts.team_id AND t.sport = ts.sport
        CROSS JOIN LATERAL (VALUES
            ('conference', CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(t.conference,'Unknown') END),
            ('division',   CASE WHEN p_sport IN ('NFL','NBA') THEN COALESCE(t.division,'Unknown') END),
            ('league',     ts.league_id::text)
        ) sc(scope_name, cohort_key)
        WHERE sc.cohort_key IS NOT NULL AND ts.sport = p_sport AND ts.season = p_season
    ),
    expanded AS (
        SELECT tsc.team_id, tsc.league_id, tsc.scope_name, tsc.cohort_key,
               sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_scope tsc
        JOIN team_stats ts ON ts.team_id=tsc.team_id AND ts.league_id=tsc.league_id
                          AND ts.sport=p_sport AND ts.season=p_season
        CROSS JOIN stat_keys sk
        WHERE ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, league_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_name, cohort_key, stat_key ORDER BY stat_value ASC))::numeric*100,1)
            END AS percentile
        FROM expanded
    ),
    per_scope AS (
        SELECT team_id, league_id, scope_name, jsonb_object_agg(stat_key, percentile) AS scope_pcts
        FROM ranked GROUP BY team_id, league_id, scope_name
    ),
    aggregated AS (
        SELECT team_id, league_id, jsonb_object_agg(scope_name, scope_pcts) AS scoped_json
        FROM per_scope GROUP BY team_id, league_id
    )
    UPDATE team_stats ts SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg WHERE ts.team_id=agg.team_id AND ts.league_id=agg.league_id
                          AND ts.sport=p_sport AND ts.season=p_season;

    RETURN QUERY SELECT v_players, v_teams;
END;
$function$

;

-- ── 2. _compute_rating_bundle — pct_position / pos_pct / pos_score now NFL + FOOTBALL ──
CREATE OR REPLACE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, composite_score numeric, specialist numeric, specialist_rank numeric, specialist_score numeric, specialty text, breakdown jsonb, scoped_ranks jsonb, scoped_scores jsonb)
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
               CASE WHEN p_sport IN ('NFL','FOOTBALL') AND position IS NOT NULL
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
$function$

;

-- ── 3. Recompute every FOOTBALL season (additive: only the 'position' scoped key is new) ──
CREATE TEMP TABLE _077_pre AS
    SELECT player_id, season, COALESCE(league_id,0) AS lid,
           rating_composite, rating_composite_rank,
           rating_specialist, rating_specialist_rank, rating_specialty
    FROM player_stats WHERE sport='FOOTBALL' AND rating_composite IS NOT NULL;

SET session_replication_role = replica;  -- skip the percentile NOTIFY (+ idempotent derived-stats) triggers; no table lock
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT season FROM player_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM recalculate_percentiles('FOOTBALL', r.season);
        PERFORM compute_rating('FOOTBALL', r.season);
    END LOOP;
END $$;
SET session_replication_role = DEFAULT;

-- ── 4. Parity gate: rating SCALARS byte-identical (only the additive 'position' key changes) ──
DO $$
DECLARE v_drift BIGINT;
BEGIN
    SELECT count(*) INTO v_drift
    FROM _077_pre pre JOIN player_stats ps
      ON ps.player_id=pre.player_id AND ps.sport='FOOTBALL' AND ps.season=pre.season
     AND COALESCE(ps.league_id,0)=pre.lid
    WHERE pre.rating_composite       IS DISTINCT FROM ps.rating_composite
       OR pre.rating_composite_rank  IS DISTINCT FROM ps.rating_composite_rank
       OR pre.rating_specialist      IS DISTINCT FROM ps.rating_specialist
       OR pre.rating_specialist_rank IS DISTINCT FROM ps.rating_specialist_rank
       OR pre.rating_specialty       IS DISTINCT FROM ps.rating_specialty;
    IF v_drift > 0 THEN
        RAISE EXCEPTION '077 PARITY FAIL: % football rows changed rating scalars (engine math must be byte-identical)', v_drift;
    END IF;
    RAISE NOTICE '077 parity OK: rating scalars byte-identical across % football rows', (SELECT count(*) FROM _077_pre);
END $$;

-- ── 5. Verify: football players now carry the By-Position scope ──
DO $$
DECLARE v_pos BIGINT; v_tot BIGINT;
BEGIN
    SELECT count(*) FILTER (WHERE rating_scoped_ranks ? 'position'), count(*)
      INTO v_pos, v_tot
      FROM player_stats
     WHERE sport='FOOTBALL' AND rating_composite IS NOT NULL AND position IS NOT NULL;
    RAISE NOTICE '077 football By-Position: % / % rated players carry rating_scoped_ranks.position', v_pos, v_tot;
    IF v_pos = 0 THEN
        RAISE EXCEPTION '077 FAIL: no football player received a position scope';
    END IF;
END $$;

COMMIT;

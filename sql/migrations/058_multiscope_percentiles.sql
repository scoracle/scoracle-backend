-- ============================================================================
-- 058 — Multi-scope cohort percentiles (player position-scope fix + team scopes)
--
-- The counting-stat pizza (template_block / datapoints_block, migrations 055/056)
-- carried only ONE percentile per slice — the within-position `percentiles` column
-- — so the cohort-scope selector had nothing to swap to and the slices looked
-- frozen (the reported "position scopes not working"). The z-pizza + fantasy paths
-- worked because they carry pct + scoped_pct. This migration makes the per-stat
-- scoped_percentiles MULTI-SCOPE and threads it through every pizza source +
-- the headline ranks.
--
-- scoped_percentiles becomes NESTED: { <scope>: { <stat_key>: pct, ... }, ... }.
-- Per-sport cohorts (the LATERAL VALUES below are the single source):
--   players  NFL      : position | conference | division   (each WITHIN position)
--            NBA      : all (positionless) | conference (within position)
--            FOOTBALL : all (positionless) | league (within position)
--   teams    NFL/NBA  : conference | division | league (= positionless, uniform league_id)
--            FOOTBALL : league (within competition)
-- The 'all' scope uses the positionless base `pct` on the frontend, so it is NOT
-- stored for players that have it as base (NFL z-pizza/headline) — only NBA/FOOTBALL
-- store an explicit 'all' per-stat percentile (their base is within-position).
--
-- Surfaces updated together:
--   * recalculate_percentiles — scoped_percentiles multi-scope (players + teams)
--   * template_block / datapoints_block / team_template_block / team_datapoints_block
--     — gain a p_scoped arg (templates) / nested read, emit scoped_pct = {scope: pct}
--   * _compute_rating_bundle — rating_breakdown[].scoped_pct + rating_scoped_ranks
--     gain the per-sport cohorts (NBA/FOOTBALL drop the old 'position' scope)
-- Go: pass ps.scoped_percentiles / ts.scoped_percentiles to the two *_template_block
-- calls (the only signature change → API restart required). Team rating headline
-- (compute_team_rating) already emits {conference,division,league} — unchanged.
--
-- Parity: composite/specialist/ranks/pct/breakdown VALUES are byte-identical; only
-- the additive scoped_pct / scoped_ranks keys change (gate 2).
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/058_multiscope_percentiles.sql
-- ============================================================================

BEGIN;

-- NOTE: the old single-scope arities template_block(text,text,jsonb,jsonb) and
-- team_template_block(text,jsonb,jsonb) are intentionally NOT dropped here — the
-- live API binary still calls them until it is restarted onto the new build, so
-- keeping them avoids a sparkline error window between apply and restart (the new
-- p_scoped forms are added alongside). A later cleanup migration can drop them once
-- no binary references them. datapoints_block / team_datapoints_block keep their
-- 4-arg arity (replaced in place; they now read the nested scoped format).

-- ── 1. recalculate_percentiles — multi-scope scoped_percentiles ─────────────
CREATE OR REPLACE FUNCTION recalculate_percentiles(
    p_sport TEXT, p_season INTEGER, p_inverse_stats TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
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
            ('position',   CASE WHEN p_sport='NFL' THEN COALESCE(ps.position,'Unknown') END),
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
$$ LANGUAGE plpgsql;

-- ── 2. Block functions — emit nested multi-scope scoped_pct {scope: pct} ─────
-- fantasy_block reads the SAME scoped_percentiles (now nested) — update it too or its
-- scoped_ranks silently break (flat ?key lookups miss the nested keys). Same 3-arg
-- arity → replaced in place, so the running binary picks it up immediately.
CREATE OR REPLACE FUNCTION public.fantasy_block(p_stats jsonb, p_pct jsonb, p_scoped jsonb)
RETURNS jsonb LANGUAGE sql STABLE AS $$
    SELECT CASE WHEN p_stats ? 'fantasy_points' THEN jsonb_object_agg(m.mode, m.blk) END
    FROM (
        SELECT v.mode,
            CASE WHEN p_stats ? v.key THEN jsonb_build_object(
                'points', (p_stats->>v.key)::numeric,
                'rank',   (p_pct->>v.key)::numeric,
                'scoped_ranks', (
                    SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>v.key)::numeric)
                                  FILTER (WHERE s.keys ? v.key), '{}'::jsonb)
                    FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
                )
            ) END AS blk
        FROM (
            SELECT 'default'::text AS mode, 'fantasy_points'::text AS key
            UNION
            SELECT DISTINCT rm.mode, 'fantasy_points' || rm.suffix FROM public.rate_modes rm
        ) v
    ) m
    WHERE m.blk IS NOT NULL;
$$;

CREATE OR REPLACE FUNCTION public.template_block(p_sport TEXT, p_position TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    WITH tmpl AS (
        SELECT t.stat_key, COALESCE(sd.rate_base, t.stat_key) AS rate_base,
               COALESCE(sd.display_name, t.stat_key) AS label, t.facet, t.sort_order
        FROM public.stat_templates t
        LEFT JOIN public.stat_definitions sd
               ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'player'
        WHERE t.sport = upper(p_sport) AND t.position_group = public.position_group(p_sport, p_position)
    ),
    modes(mode, suffix) AS (
        SELECT 'default'::text, ''::text
        UNION SELECT DISTINCT rm.mode, rm.suffix FROM public.rate_modes rm
    )
    SELECT jsonb_object_agg(m.mode, m.items)
    FROM (
        SELECT md.mode,
            (SELECT jsonb_agg(jsonb_build_object(
                'key',   t.stat_key,
                'label', t.label,
                'value', COALESCE((p_stats->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric,
                                  (p_stats->>t.stat_key)::numeric, 0),
                'pct',   COALESCE((p_pct->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric,
                                  (p_pct->>t.stat_key)::numeric, 0),
                'scoped_pct', (
                    SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric)
                                  FILTER (WHERE s.keys ? (CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END)), '{}'::jsonb)
                    FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
                ),
                'facet', t.facet,
                'sort',  t.sort_order
            ) ORDER BY t.sort_order) FROM tmpl t) AS items
        FROM modes md
        WHERE EXISTS (SELECT 1 FROM tmpl t
                      WHERE p_stats ? (CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))
    ) m
    WHERE m.items IS NOT NULL;
$$;

CREATE OR REPLACE FUNCTION public.datapoints_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key', sd.key_name, 'label', sd.display_name,
               'value', COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',   (p_pct->>sd.key_name)::numeric,
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>sd.key_name)::numeric)
                                 FILTER (WHERE s.keys ? sd.key_name), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', sd.category, 'sort', sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'player' AND sd.key_name = k.key
    WHERE NOT EXISTS (SELECT 1 FROM public.rate_modes rm
                      WHERE rm.sport = upper(p_sport) AND right(k.key, length(rm.suffix)) = rm.suffix);
$$;

CREATE OR REPLACE FUNCTION public.team_template_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_build_object('default', jsonb_agg(jsonb_build_object(
               'key', t.stat_key, 'label', COALESCE(sd.display_name, t.stat_key),
               'value', COALESCE((p_stats->>t.stat_key)::numeric, 0),
               'pct',   COALESCE((p_pct->>t.stat_key)::numeric, 0),
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>t.stat_key)::numeric)
                                 FILTER (WHERE s.keys ? t.stat_key), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', t.facet, 'sort', t.sort_order
           ) ORDER BY t.sort_order, t.stat_key))
    FROM public.stat_templates t
    LEFT JOIN public.stat_definitions sd
           ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'team'
    WHERE t.sport = upper(p_sport) AND t.position_group = 'team'
    HAVING count(*) > 0;
$$;

CREATE OR REPLACE FUNCTION public.team_datapoints_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key', sd.key_name, 'label', sd.display_name,
               'value', COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',   (p_pct->>sd.key_name)::numeric,
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>sd.key_name)::numeric)
                                 FILTER (WHERE s.keys ? sd.key_name), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', sd.category, 'sort', sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'team' AND sd.key_name = k.key;
$$;

-- ── 3. _compute_rating_bundle — per-sport cohort scoped_pct + scoped_ranks ───
CREATE OR REPLACE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, specialist numeric, specialist_rank numeric, specialty text, breakdown jsonb, scoped_ranks jsonb)
 LANGUAGE sql STABLE
AS $function$
    WITH dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
               tm.conference, tm.division,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet
        FROM player_stats ps
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        CROSS JOIN LATERAL rating_datapoints(p_sport, ps.stats, p_rate_mode) d
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
               -- per-sport cohort percentiles for the slice re-rank (same sign*z,
               -- ranked within the cohort). NBA/FOOTBALL drop 'position' (their base
               -- is the positionless pct); they instead carry conference/league.
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

-- ── 4. Recompute every (sport, season): scoped_percentiles + player scoped_ranks
-- Parity snapshot: the rating SCALARS must be byte-identical after recompute (only the
-- additive scoped_pct / scoped_ranks keys change). _compute_rating_bundle's composite/
-- specialist/pct math is untouched; the teams LEFT JOIN adds cohort columns without
-- changing dp multiplicity (teams.id is unique).
CREATE TEMP TABLE _058_pre AS
    SELECT player_id, sport, season, league_id,
           rating_composite, rating_composite_rank,
           rating_specialist, rating_specialist_rank, rating_specialty
    FROM player_stats WHERE rating_composite IS NOT NULL;

ALTER TABLE player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;
ALTER TABLE team_stats   DISABLE TRIGGER trg_percentile_changed_team_stats;

DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats ORDER BY sport, season LOOP
        PERFORM recalculate_percentiles(r.sport, r.season);
        PERFORM compute_rating(r.sport, r.season);
    END LOOP;
END $$;

ALTER TABLE player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;
ALTER TABLE team_stats   ENABLE TRIGGER trg_percentile_changed_team_stats;

-- ── 5. Gates ─────────────────────────────────────────────────────────────────
-- Parity: rating scalars unchanged (the engine math is untouched).
DO $$
DECLARE v_drift BIGINT;
BEGIN
    SELECT count(*) INTO v_drift
    FROM _058_pre pre JOIN player_stats ps
      ON ps.player_id=pre.player_id AND ps.sport=pre.sport AND ps.season=pre.season AND ps.league_id=pre.league_id
    WHERE pre.rating_composite      IS DISTINCT FROM ps.rating_composite
       OR pre.rating_composite_rank IS DISTINCT FROM ps.rating_composite_rank
       OR pre.rating_specialist     IS DISTINCT FROM ps.rating_specialist
       OR pre.rating_specialist_rank IS DISTINCT FROM ps.rating_specialist_rank
       OR pre.rating_specialty      IS DISTINCT FROM ps.rating_specialty;
    IF v_drift > 0 THEN
        RAISE EXCEPTION '058 PARITY FAIL: % rows changed rating scalars (engine math must be byte-identical)', v_drift;
    END IF;
    RAISE NOTICE '058 parity OK: rating scalars byte-identical across % snapshotted rows', (SELECT count(*) FROM _058_pre);
END $$;

DO $$
DECLARE
    v_nfl_qb BIGINT; v_nba BIGINT; v_fb BIGINT; v_team BIGINT; v_hdr BIGINT;
BEGIN
    -- player scoped_percentiles carry the expected per-sport scopes
    SELECT count(*) INTO v_nfl_qb FROM player_stats
        WHERE sport='NFL' AND position='Quarterback'
          AND scoped_percentiles ?& array['position','conference','division'];
    SELECT count(*) INTO v_nba FROM player_stats
        WHERE sport='NBA' AND scoped_percentiles ?& array['all','conference'];
    SELECT count(*) INTO v_fb FROM player_stats
        WHERE sport='FOOTBALL' AND scoped_percentiles ?& array['all','league'];
    SELECT count(*) INTO v_team FROM team_stats
        WHERE sport='NFL' AND scoped_percentiles ?& array['conference','division','league'];
    -- player headline scoped_ranks gains the cohorts (NFL has all three)
    SELECT count(*) INTO v_hdr FROM player_stats
        WHERE sport='NFL' AND rating_scoped_ranks ?& array['position','conference','division'];
    IF v_nfl_qb=0 OR v_nba=0 OR v_fb=0 OR v_team=0 OR v_hdr=0 THEN
        RAISE EXCEPTION '058 gate FAIL: scope coverage NFLqb=% NBA=% FB=% team=% hdr=%', v_nfl_qb, v_nba, v_fb, v_team, v_hdr;
    END IF;
    RAISE NOTICE '058 OK: multi-scope scoped_percentiles + headline ranks (NFLqb=% NBA=% FB=% team=% hdr=%)', v_nfl_qb, v_nba, v_fb, v_team, v_hdr;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('058_multiscope_percentiles')
ON CONFLICT (version) DO NOTHING;

COMMIT;

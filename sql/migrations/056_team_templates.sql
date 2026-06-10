-- ============================================================================
-- 056 — Team templates + team datapoints (build-order ③)
--
-- Teams join the 055 counting-stat world: curated offense/defense templates per
-- sport (the team Composite flips from z-pizzas to counting-stat facet pizzas)
-- plus a generic team datapoints block. Mirrors 055's player machinery but with
-- SEPARATE team functions — deliberately NOT a generalized player function:
--   * no signature change on public.datapoints_block → no prepared-statement
--     compat gap on the live API, zero player-path risk;
--   * teams have no rate modes, so team_template_block emits 'default' only and
--     team_datapoints_block applies NO rate-sibling exclusion — the player
--     exclusion would wrongly drop genuine NFL team base keys that merely END
--     in a mode suffix (points_per_game, yards_per_game, …).
--
-- Template facets are 'offense'/'defense' — the SAME keys as the team z facets
-- and rating_categories, so the frontend's per-facet sub-score footers line up
-- unchanged. Reversible per sport: DELETE the sport's position_group='team'
-- rows → that sport's teams fall back to the z-pizza.
--
-- Curation grounded in measured non-zero coverage across all team-season rows
-- (NFL 256 / NBA 240 / FOOTBALL 582): every seeded key ≥94% non-zero coverage.
-- Excluded for coverage: NFL qb_hits (33%), first-down/red-zone family (75%,
-- absent 2018-19); FOOTBALL chances_created (33%), ball_recovery (38%),
-- passes_final_third (38%). NBA def_fg_pct/def_fg3_pct lack stat_definitions.
-- ============================================================================

BEGIN;

-- ----------------------------------------------------------------------------
-- §1 Seed team templates (position_group = 'team'; PK sport+group+key).
--    sort_order encodes facet order (10s offense, 20s defense) + wedge order.
-- ----------------------------------------------------------------------------

INSERT INTO public.stat_templates (sport, position_group, stat_key, facet, sort_order) VALUES
    -- NFL offense (7)
    ('NFL', 'team', 'points_for',              'offense', 10),
    ('NFL', 'team', 'total_yards',             'offense', 11),
    ('NFL', 'team', 'passing_yards',           'offense', 12),
    ('NFL', 'team', 'rushing_yards',           'offense', 13),
    ('NFL', 'team', 'passing_touchdowns',      'offense', 14),
    ('NFL', 'team', 'rushing_touchdowns',      'offense', 15),
    ('NFL', 'team', 'turnovers',               'offense', 16),  -- is_inverse: giveaways
    -- NFL defense (7)
    ('NFL', 'team', 'points_against',          'defense', 20),  -- is_inverse
    ('NFL', 'team', 'defensive_sacks',         'defense', 21),
    ('NFL', 'team', 'defensive_interceptions', 'defense', 22),
    ('NFL', 'team', 'takeaways',               'defense', 23),
    ('NFL', 'team', 'total_tackles',           'defense', 24),
    ('NFL', 'team', 'passes_defended',         'defense', 25),
    ('NFL', 'team', 'tackles_for_loss',        'defense', 26),

    -- NBA offense (6) — NBA team stats are per-game averages by nature
    ('NBA', 'team', 'pts',                     'offense', 10),
    ('NBA', 'team', 'ast',                     'offense', 11),
    ('NBA', 'team', 'fg3m',                    'offense', 12),
    ('NBA', 'team', 'true_shooting_pct',       'offense', 13),
    ('NBA', 'team', 'oreb',                    'offense', 14),
    ('NBA', 'team', 'turnover',                'offense', 15),  -- is_inverse
    -- NBA defense (5)
    ('NBA', 'team', 'pts_allowed',             'defense', 20),  -- is_inverse
    ('NBA', 'team', 'reb',                     'defense', 21),
    ('NBA', 'team', 'stl',                     'defense', 22),
    ('NBA', 'team', 'blk',                     'defense', 23),
    ('NBA', 'team', 'dreb',                    'defense', 24),

    -- FOOTBALL offense (7)
    ('FOOTBALL', 'team', 'goals_for',           'offense', 10),
    ('FOOTBALL', 'team', 'shots_on_target',     'offense', 11),
    ('FOOTBALL', 'team', 'big_chances_created', 'offense', 12),
    ('FOOTBALL', 'team', 'key_passes',          'offense', 13),
    ('FOOTBALL', 'team', 'assists',             'offense', 14),
    ('FOOTBALL', 'team', 'possession_pct',      'offense', 15),
    ('FOOTBALL', 'team', 'pass_accuracy',       'offense', 16),
    -- FOOTBALL defense (7)
    ('FOOTBALL', 'team', 'goals_against',       'defense', 20),  -- is_inverse
    ('FOOTBALL', 'team', 'tackles',             'defense', 21),
    ('FOOTBALL', 'team', 'interceptions',       'defense', 22),
    ('FOOTBALL', 'team', 'clearances',          'defense', 23),
    ('FOOTBALL', 'team', 'blocked_shots',       'defense', 24),
    ('FOOTBALL', 'team', 'saves',               'defense', 25),
    ('FOOTBALL', 'team', 'aerials_won',         'defense', 26)
ON CONFLICT (sport, position_group, stat_key) DO NOTHING;

ANALYZE public.stat_templates;

-- ----------------------------------------------------------------------------
-- §2 team_template_block — the team sibling of template_block (055). Teams have
--    no rate modes, so the block is {'default': items} only; the frontend's
--    templateForMode falls back to 'default' for any requested mode. NULL when
--    the sport has no team template rows → z-pizza fallback.
-- ----------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.team_template_block(p_sport TEXT, p_stats JSONB, p_pct JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_build_object('default', jsonb_agg(jsonb_build_object(
               'key',   t.stat_key,
               'label', COALESCE(sd.display_name, t.stat_key),
               'value', COALESCE((p_stats->>t.stat_key)::numeric, 0),
               'pct',   COALESCE((p_pct  ->>t.stat_key)::numeric, 0),
               'facet', t.facet,
               'sort',  t.sort_order
           ) ORDER BY t.sort_order, t.stat_key))
    FROM public.stat_templates t
    LEFT JOIN public.stat_definitions sd
           ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'team'
    WHERE t.sport = upper(p_sport) AND t.position_group = 'team'
    HAVING count(*) > 0;
$$;

-- ----------------------------------------------------------------------------
-- §3 team_datapoints_block — the team sibling of datapoints_block (055): every
--    percentile-ranked team stat, labeled/faceted/sorted from stat_definitions
--    (entity_type='team'; the percentiles meta keys — _sample_size etc. — have
--    no definition row and drop out). NO rate-sibling exclusion: teams carry no
--    rate siblings, and NFL team base keys legitimately end in '_per_game'.
--    scoped_pct is labeled by the team cohort scope: league (FOOTBALL) /
--    conference (NBA, NFL) — mirroring recalculate_percentiles' team scoping.
-- ----------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.team_datapoints_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key',        sd.key_name,
               'label',      sd.display_name,
               'value',      COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',        (p_pct->>sd.key_name)::numeric,
               'scoped_pct', CASE WHEN p_scoped ? sd.key_name
                                  THEN jsonb_build_object(
                                       CASE WHEN upper(p_sport) = 'FOOTBALL' THEN 'league' ELSE 'conference' END,
                                       (p_scoped->>sd.key_name)::numeric) END,
               'facet',      sd.category,
               'sort',       sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'team' AND sd.key_name = k.key;
$$;

-- ----------------------------------------------------------------------------
-- §4 Gate 1 — seed integrity: expected row counts per sport, facets only
--    offense/defense, and every seeded key resolves to a team stat definition
--    (label source) with the negative-direction keys flagged is_inverse.
-- ----------------------------------------------------------------------------

DO $$
DECLARE
    v_bad INTEGER;
    v_counts TEXT;
BEGIN
    SELECT string_agg(sport || '=' || n, ', ' ORDER BY sport) INTO v_counts
    FROM (SELECT sport, count(*) AS n FROM public.stat_templates
          WHERE position_group = 'team' GROUP BY sport) c;
    IF v_counts IS DISTINCT FROM 'FOOTBALL=14, NBA=11, NFL=14' THEN
        RAISE EXCEPTION '056 gate 1 FAIL: unexpected seed counts (%)', v_counts;
    END IF;

    SELECT count(*) INTO v_bad FROM public.stat_templates
    WHERE position_group = 'team' AND facet NOT IN ('offense', 'defense');
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 1 FAIL: % team template rows with unexpected facet', v_bad;
    END IF;

    SELECT count(*) INTO v_bad
    FROM public.stat_templates t
    LEFT JOIN public.stat_definitions sd
           ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'team'
    WHERE t.position_group = 'team' AND sd.key_name IS NULL;
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 1 FAIL: % seeded keys lack a team stat definition', v_bad;
    END IF;

    SELECT count(*) INTO v_bad
    FROM public.stat_templates t
    JOIN public.stat_definitions sd
      ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'team'
    WHERE t.position_group = 'team'
      AND t.stat_key IN ('turnovers', 'points_against', 'turnover', 'pts_allowed', 'goals_against')
      AND NOT sd.is_inverse;
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 1 FAIL: % negative-direction keys missing is_inverse', v_bad;
    END IF;

    RAISE NOTICE '056 gate 1 OK: 39 team template rows seeded (%), facets clean, definitions resolve', v_counts;
END $$;

-- ----------------------------------------------------------------------------
-- §5 Gate 2 — template shape: every team_stats row emits a default-mode array
--    of exactly the sport's seed count, every item faceted offense/defense.
-- ----------------------------------------------------------------------------

DO $$
DECLARE
    v_bad INTEGER;
    v_teams INTEGER;
BEGIN
    SELECT count(*) INTO v_bad
    FROM public.team_stats ts
    CROSS JOIN LATERAL public.team_template_block(ts.sport, ts.stats, ts.percentiles) AS tb(tmpl)
    WHERE tb.tmpl IS NULL
       OR jsonb_array_length(tb.tmpl->'default') IS DISTINCT FROM
          (SELECT count(*)::int FROM public.stat_templates t
           WHERE t.sport = ts.sport AND t.position_group = 'team');
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 2 FAIL: % team rows with missing/short template', v_bad;
    END IF;

    SELECT count(*) INTO v_bad
    FROM public.team_stats ts
    CROSS JOIN LATERAL public.team_template_block(ts.sport, ts.stats, ts.percentiles) AS tb(tmpl)
    CROSS JOIN LATERAL jsonb_array_elements(tb.tmpl->'default') AS item
    WHERE item->>'facet' NOT IN ('offense', 'defense') OR item->>'label' IS NULL;
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 2 FAIL: % template items with bad facet/label', v_bad;
    END IF;

    SELECT count(*) INTO v_teams FROM public.team_stats;
    RAISE NOTICE '056 gate 2 OK: % team rows templated, every item faceted offense/defense', v_teams;
END $$;

-- ----------------------------------------------------------------------------
-- §6 Gate 3 — datapoints invariants: NULL ⟺ no ranked key has a team
--    definition; no percentile meta keys leak; scoped_pct labeled by the
--    sport's team scope; NFL '_per_game' base keys present (no false
--    rate-sibling exclusion).
-- ----------------------------------------------------------------------------

DO $$
DECLARE
    v_bad INTEGER;
    v_with INTEGER;
BEGIN
    CREATE TEMP TABLE _056_dp ON COMMIT DROP AS
    SELECT ts.sport, ts.team_id, ts.season, COALESCE(ts.league_id, 0) AS league_id,
           public.team_datapoints_block(ts.sport, ts.stats, ts.percentiles, ts.scoped_percentiles) AS dp,
           ts.percentiles
    FROM public.team_stats ts;
    ANALYZE _056_dp;

    -- NULL ⟺ no qualifying key
    SELECT count(*) INTO v_bad FROM _056_dp d
    WHERE (d.dp IS NULL) IS DISTINCT FROM NOT EXISTS (
        SELECT 1 FROM jsonb_object_keys(COALESCE(d.percentiles, '{}'::jsonb)) k(key)
        JOIN public.stat_definitions sd
          ON sd.sport = d.sport AND sd.entity_type = 'team' AND sd.key_name = k.key);
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 3 FAIL: % rows violate the NULL ⟺ no-qualifying-key invariant', v_bad;
    END IF;

    -- no percentile meta keys leak into the output
    SELECT count(*) INTO v_bad
    FROM _056_dp d CROSS JOIN LATERAL jsonb_array_elements(d.dp) item
    WHERE d.dp IS NOT NULL
      AND (item->>'key' LIKE '\_%' OR item->>'key' IN ('scope_type', 'scope_id', 'scope_name'));
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 3 FAIL: % meta keys leaked into datapoints', v_bad;
    END IF;

    -- scoped_pct labeled by the team cohort scope
    SELECT count(*) INTO v_bad
    FROM _056_dp d CROSS JOIN LATERAL jsonb_array_elements(d.dp) item
    WHERE d.dp IS NOT NULL AND item->'scoped_pct' IS NOT NULL
      AND jsonb_typeof(item->'scoped_pct') = 'object'
      AND NOT (item->'scoped_pct') ? (CASE WHEN d.sport = 'FOOTBALL' THEN 'league' ELSE 'conference' END);
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 3 FAIL: % scoped_pct entries with the wrong scope label', v_bad;
    END IF;

    -- NFL base keys ending in a rate-mode suffix must NOT be excluded
    SELECT count(*) INTO v_bad FROM _056_dp d
    WHERE d.sport = 'NFL' AND d.percentiles ? 'points_per_game'
      AND NOT EXISTS (SELECT 1 FROM jsonb_array_elements(d.dp) item
                      WHERE item->>'key' = 'points_per_game');
    IF v_bad > 0 THEN
        RAISE EXCEPTION '056 gate 3 FAIL: % NFL rows wrongly dropped points_per_game', v_bad;
    END IF;

    SELECT count(*) INTO v_with FROM _056_dp WHERE dp IS NOT NULL;
    RAISE NOTICE '056 gate 3 OK: datapoints emitted for % team rows, meta-free, scope labels correct', v_with;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('056_team_templates')
ON CONFLICT (version) DO NOTHING;

COMMIT;

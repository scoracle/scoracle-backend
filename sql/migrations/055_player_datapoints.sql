-- ============================================================================
-- 055 — Generic player datapoints block + template-driven facet grouping
--
-- Two product moves, one migration:
--
--   1. stat_templates grows a `facet` column and FOOTBALL gets curated template
--      rows per position group (goalkeeper/defender/midfielder/attacker). The
--      football Composite flips from the flat 19-wedge z-pizza to counting-stat
--      template pizzas grouped by facet — and the frontend goalkeeper hardcode
--      (GK_LABELS/GK_PASSING_LABELS) becomes ordinary template rows. Fully
--      reversible: DELETE the FOOTBALL rows → template_block returns NULL → the
--      frontend falls back to the z-pizza.
--
--   2. A new generic `datapoints_block` — EVERY percentile-ranked base stat for
--      a player (default rate mode only), labeled + faceted from
--      stat_definitions. Ships in the sparkline payload as a third sibling to
--      fantasy_block/template_block; generic data layer for future cards.
--
-- Behavior guards:
--   - Gate 1 (parity): NFL/NBA template_block output is byte-identical modulo
--     the new per-item "facet": null.
--   - Gate 2 (shape): every football player whose stats carry ≥1 of their
--     position group's template keys gets a faceted template.
--   - Gate 3 (sanity): datapoints_block emits exactly the registered base keys
--     (no rate-mode siblings), null only when no qualifying key exists.
--
-- template_block also gains a mode-invariant fallback: keys with no rate-mode
-- sibling (save_pct, pass_accuracy, …) now show their base value/pct in per-X
-- modes instead of zeroing out. Gate 1 proves this changes nothing for the
-- existing NFL/NBA templates (their keys all have true siblings).
-- ============================================================================

BEGIN;

-- ── §0 Baseline: capture current template output for the sports that have one ──
CREATE TEMP TABLE _055_tmpl_baseline ON COMMIT DROP AS
SELECT ps.sport, ps.player_id, ps.season, COALESCE(ps.league_id, 0) AS league_id,
       public.template_block(ps.sport, ps.position, ps.stats, ps.percentiles) AS tmpl
FROM public.player_stats ps
WHERE ps.sport IN ('NBA', 'NFL');
ANALYZE _055_tmpl_baseline;  -- CTAS tables have no stats (054 lesson)

-- ── §1 Facet column ─────────────────────────────────────────────────────────
ALTER TABLE public.stat_templates ADD COLUMN facet TEXT;
COMMENT ON COLUMN public.stat_templates.facet IS
    'Pizza grouping for the Composite card. NULL = single unfaceted pizza '
    '(the NFL/NBA fantasy templates); non-NULL = one pizza per facet, ordered '
    'by item sort_order (the football counting-stat pizzas).';

-- ── §2 position_group learns football ───────────────────────────────────────
-- Raw provider position → template group. NFL offensive skill + NBA (one group)
-- as before; FOOTBALL now maps its four provider positions. NULL position (rare,
-- unrated fringe) stays NULL → frontend z-pizza fallback.
CREATE OR REPLACE FUNCTION public.position_group(p_sport TEXT, p_position TEXT)
RETURNS TEXT LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE upper(p_sport)
        WHEN 'NBA' THEN 'ALL'  -- one Fantasy-mode template for all NBA players
        WHEN 'NFL' THEN CASE p_position
            WHEN 'Quarterback' THEN 'quarterback'
            WHEN 'QB'          THEN 'quarterback'
            WHEN 'Running Back' THEN 'running-back'
            WHEN 'Fullback'     THEN 'running-back'
            WHEN 'RB'           THEN 'running-back'
            WHEN 'FB'           THEN 'running-back'
            WHEN 'HB'           THEN 'running-back'
            WHEN 'Wide Receiver' THEN 'receiver'
            WHEN 'Tight End'     THEN 'receiver'
            WHEN 'WR'            THEN 'receiver'
            WHEN 'TE'            THEN 'receiver'
            ELSE NULL
        END
        WHEN 'FOOTBALL' THEN CASE p_position
            WHEN 'Goalkeeper' THEN 'goalkeeper'
            WHEN 'Defender'   THEN 'defender'
            WHEN 'Midfielder' THEN 'midfielder'
            WHEN 'Attacker'   THEN 'attacker'
            ELSE NULL
        END
        ELSE NULL
    END;
$$;

-- ── §3 Football template seeds ──────────────────────────────────────────────
-- Curation rules: broadly-covered keys only (expected_goals is empty provider-
-- wide; through_balls/penalty_goals < 40% coverage — excluded). Volume keys all
-- carry per_90/per_game siblings; percentage keys are mode-invariant and rely
-- on the §4 base fallback. sort_order encodes facet order (10s/20s/30s) and
-- wedge order within the facet.
DELETE FROM public.stat_templates WHERE sport = 'FOOTBALL';
INSERT INTO public.stat_templates (sport, position_group, stat_key, facet, sort_order) VALUES
    -- Goalkeeper: shot-stopping (the old GK_LABELS hardcode, now data) + distribution
    ('FOOTBALL', 'goalkeeper', 'saves',            'shot-stopping', 10),
    ('FOOTBALL', 'goalkeeper', 'save_pct',         'shot-stopping', 11),
    ('FOOTBALL', 'goalkeeper', 'saves_insidebox',  'shot-stopping', 12),
    ('FOOTBALL', 'goalkeeper', 'goals_conceded',   'shot-stopping', 13),
    ('FOOTBALL', 'goalkeeper', 'penalties_saved',  'shot-stopping', 14),
    ('FOOTBALL', 'goalkeeper', 'punches',          'shot-stopping', 15),
    ('FOOTBALL', 'goalkeeper', 'good_high_claim',  'shot-stopping', 16),
    ('FOOTBALL', 'goalkeeper', 'passes_total',       'passing', 20),
    ('FOOTBALL', 'goalkeeper', 'pass_accuracy',      'passing', 21),
    ('FOOTBALL', 'goalkeeper', 'long_balls',         'passing', 22),
    ('FOOTBALL', 'goalkeeper', 'long_balls_won',     'passing', 23),
    ('FOOTBALL', 'goalkeeper', 'long_ball_accuracy', 'passing', 24),
    -- Defender: defending first, then distribution, then set-piece/attacking threat
    ('FOOTBALL', 'defender', 'tackles',       'defending', 10),
    ('FOOTBALL', 'defender', 'interceptions', 'defending', 11),
    ('FOOTBALL', 'defender', 'clearances',    'defending', 12),
    ('FOOTBALL', 'defender', 'blocks',        'defending', 13),
    ('FOOTBALL', 'defender', 'aeriels_won',   'defending', 14),
    ('FOOTBALL', 'defender', 'ball_recovery', 'defending', 15),
    ('FOOTBALL', 'defender', 'duels_won',     'defending', 16),
    ('FOOTBALL', 'defender', 'passes_total',          'passing', 20),
    ('FOOTBALL', 'defender', 'pass_accuracy',         'passing', 21),
    ('FOOTBALL', 'defender', 'long_balls_won',        'passing', 22),
    ('FOOTBALL', 'defender', 'long_ball_accuracy',    'passing', 23),
    ('FOOTBALL', 'defender', 'passes_in_final_third', 'passing', 24),
    ('FOOTBALL', 'defender', 'key_passes',            'passing', 25),
    ('FOOTBALL', 'defender', 'goals',           'attacking', 30),
    ('FOOTBALL', 'defender', 'assists',         'attacking', 31),
    ('FOOTBALL', 'defender', 'shots_total',     'attacking', 32),
    ('FOOTBALL', 'defender', 'shots_on_target', 'attacking', 33),
    -- Midfielder: creation first
    ('FOOTBALL', 'midfielder', 'key_passes',            'passing', 10),
    ('FOOTBALL', 'midfielder', 'chances_created',       'passing', 11),
    ('FOOTBALL', 'midfielder', 'big_chances_created',   'passing', 12),
    ('FOOTBALL', 'midfielder', 'passes_total',          'passing', 13),
    ('FOOTBALL', 'midfielder', 'pass_accuracy',         'passing', 14),
    ('FOOTBALL', 'midfielder', 'passes_in_final_third', 'passing', 15),
    ('FOOTBALL', 'midfielder', 'goals',            'attacking', 20),
    ('FOOTBALL', 'midfielder', 'assists',          'attacking', 21),
    ('FOOTBALL', 'midfielder', 'shots_total',      'attacking', 22),
    ('FOOTBALL', 'midfielder', 'shots_on_target',  'attacking', 23),
    ('FOOTBALL', 'midfielder', 'shot_accuracy',    'attacking', 24),
    ('FOOTBALL', 'midfielder', 'dribbles_success', 'attacking', 25),
    ('FOOTBALL', 'midfielder', 'tackles',           'defending', 30),
    ('FOOTBALL', 'midfielder', 'interceptions',     'defending', 31),
    ('FOOTBALL', 'midfielder', 'ball_recovery',     'defending', 32),
    ('FOOTBALL', 'midfielder', 'duels_won',         'defending', 33),
    ('FOOTBALL', 'midfielder', 'duel_success_rate', 'defending', 34),
    -- Attacker: goalscoring first
    ('FOOTBALL', 'attacker', 'goals',            'attacking', 10),
    ('FOOTBALL', 'attacker', 'assists',          'attacking', 11),
    ('FOOTBALL', 'attacker', 'shots_total',      'attacking', 12),
    ('FOOTBALL', 'attacker', 'shots_on_target',  'attacking', 13),
    ('FOOTBALL', 'attacker', 'shot_accuracy',    'attacking', 14),
    ('FOOTBALL', 'attacker', 'dribbles_success', 'attacking', 15),
    ('FOOTBALL', 'attacker', 'key_passes',          'passing', 20),
    ('FOOTBALL', 'attacker', 'chances_created',     'passing', 21),
    ('FOOTBALL', 'attacker', 'big_chances_created', 'passing', 22),
    ('FOOTBALL', 'attacker', 'crosses_accurate',    'passing', 23),
    ('FOOTBALL', 'attacker', 'pass_accuracy',       'passing', 24),
    ('FOOTBALL', 'attacker', 'tackles',           'defending', 30),
    ('FOOTBALL', 'attacker', 'interceptions',     'defending', 31),
    ('FOOTBALL', 'attacker', 'ball_recovery',     'defending', 32),
    ('FOOTBALL', 'attacker', 'duels_won',         'defending', 33),
    ('FOOTBALL', 'attacker', 'duel_success_rate', 'defending', 34),
    ('FOOTBALL', 'attacker', 'aeriels_won',       'defending', 35);
ANALYZE public.stat_templates;  -- in-transaction changes invisible to autovacuum

-- ── §4 template_block: emit facet + mode-invariant base fallback ────────────
CREATE OR REPLACE FUNCTION public.template_block(p_sport TEXT, p_position TEXT, p_stats JSONB, p_pct JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    WITH tmpl AS (
        SELECT t.stat_key,
               -- rate modes read the sibling; the sibling base is the legacy alias from
               -- stat_definitions.rate_base when it differs (e.g. turnover → tov).
               COALESCE(sd.rate_base, t.stat_key) AS rate_base,
               COALESCE(sd.display_name, t.stat_key) AS label,
               t.facet,
               t.sort_order
        FROM public.stat_templates t
        LEFT JOIN public.stat_definitions sd
               ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'player'
        WHERE t.sport = upper(p_sport)
          AND t.position_group = public.position_group(p_sport, p_position)
    ),
    modes(mode, suffix) AS (
        SELECT 'default'::text, ''::text
        UNION
        SELECT DISTINCT rm.mode, rm.suffix FROM public.rate_modes rm
    )
    SELECT jsonb_object_agg(m.mode, m.items)
    FROM (
        SELECT md.mode,
            -- value/pct read the mode sibling, falling back to the base key for
            -- mode-invariant stats (percentages have no per-X sibling — they'd
            -- otherwise zero out in per-X modes).
            (SELECT jsonb_agg(jsonb_build_object(
                'key',   t.stat_key,
                'label', t.label,
                'value', COALESCE((p_stats->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE t.rate_base || md.suffix END))::numeric,
                                  (p_stats->>t.stat_key)::numeric, 0),
                'pct',   COALESCE((p_pct  ->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE t.rate_base || md.suffix END))::numeric,
                                  (p_pct  ->>t.stat_key)::numeric, 0),
                'facet', t.facet,
                'sort',  t.sort_order
            ) ORDER BY t.sort_order) FROM tmpl t) AS items
        FROM modes md
        -- emit a mode only when its rate sibling actually exists for this sport
        WHERE EXISTS (SELECT 1 FROM tmpl t
                      WHERE p_stats ? (CASE WHEN md.suffix = '' THEN t.stat_key
                                            ELSE t.rate_base || md.suffix END))
    ) m
    WHERE m.items IS NOT NULL;
$$;

-- ── §5 Generic datapoints block ─────────────────────────────────────────────
-- EVERY percentile-ranked base stat for one player, default rate mode only:
-- the keys of `percentiles` minus rate-mode siblings, labeled/faceted/sorted
-- from stat_definitions (keys without a registered definition are dropped).
-- Sibling to fantasy_block/template_block in the sparkline payload; NULL when
-- no ranked key has a definition (unranked fringe players, teams).
CREATE OR REPLACE FUNCTION public.datapoints_block(p_sport TEXT, p_stats JSONB, p_pct JSONB, p_scoped JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    SELECT jsonb_agg(jsonb_build_object(
               'key',        sd.key_name,
               'label',      sd.display_name,
               'value',      COALESCE((p_stats->>sd.key_name)::numeric, 0),
               'pct',        (p_pct->>sd.key_name)::numeric,
               'scoped_pct', CASE WHEN p_scoped ? sd.key_name
                                  THEN jsonb_build_object('position', (p_scoped->>sd.key_name)::numeric) END,
               'facet',      sd.category,
               'sort',       sd.sort_order
           ) ORDER BY sd.sort_order, sd.key_name)
    FROM jsonb_object_keys(COALESCE(p_pct, '{}'::jsonb)) AS k(key)
    JOIN public.stat_definitions sd
      ON sd.sport = upper(p_sport) AND sd.entity_type = 'player' AND sd.key_name = k.key
    -- default mode only: drop rate-mode sibling keys (goals_per_90, pts_per_36, …)
    WHERE NOT EXISTS (SELECT 1 FROM public.rate_modes rm
                      WHERE rm.sport = upper(p_sport)
                        AND right(k.key, length(rm.suffix)) = rm.suffix);
$$;

-- ── Gate 1: NFL/NBA template parity (modulo the new "facet": null) ──────────
DO $$
DECLARE
    v_bad BIGINT;
BEGIN
    SELECT count(*) INTO v_bad
    FROM _055_tmpl_baseline b
    JOIN public.player_stats ps
      ON ps.sport = b.sport AND ps.player_id = b.player_id
     AND ps.season = b.season AND COALESCE(ps.league_id, 0) = b.league_id
    CROSS JOIN LATERAL (
        SELECT public.template_block(ps.sport, ps.position, ps.stats, ps.percentiles) AS tmpl
    ) n
    WHERE (b.tmpl IS NULL) IS DISTINCT FROM (n.tmpl IS NULL)
       OR (b.tmpl IS NOT NULL AND b.tmpl IS DISTINCT FROM (
              SELECT jsonb_object_agg(m.key, (
                  SELECT jsonb_agg(x.item - 'facet' ORDER BY x.ord)
                  FROM jsonb_array_elements(m.value) WITH ORDINALITY AS x(item, ord)
              ))
              FROM jsonb_each(n.tmpl) m
          ));
    IF v_bad > 0 THEN
        RAISE EXCEPTION '055 gate 1 FAILED: % NFL/NBA template rows changed beyond the facet field', v_bad;
    END IF;
    RAISE NOTICE '055 gate 1 OK: NFL/NBA template output unchanged (modulo facet:null)';
END $$;

-- ── Gate 2: football template shape ─────────────────────────────────────────
DO $$
DECLARE
    v_bad    BIGINT;
    v_with   BIGINT;
BEGIN
    -- seed integrity: every FOOTBALL template key is a registered player stat
    SELECT count(*) INTO v_bad
    FROM public.stat_templates t
    WHERE t.sport = 'FOOTBALL'
      AND NOT EXISTS (SELECT 1 FROM public.stat_definitions sd
                      WHERE sd.sport = t.sport AND sd.key_name = t.stat_key
                        AND sd.entity_type = 'player');
    IF v_bad > 0 THEN
        RAISE EXCEPTION '055 gate 2 FAILED: % FOOTBALL template keys missing from stat_definitions', v_bad;
    END IF;

    -- every football player whose stats carry ≥1 template key for their group
    -- gets a template, and every emitted item carries a facet
    SELECT
        count(*) FILTER (WHERE n.tmpl IS NULL),
        count(*) FILTER (WHERE n.tmpl IS NOT NULL)
    INTO v_bad, v_with
    FROM public.player_stats ps
    CROSS JOIN LATERAL (
        SELECT public.template_block(ps.sport, ps.position, ps.stats, ps.percentiles) AS tmpl
    ) n
    WHERE ps.sport = 'FOOTBALL'
      AND EXISTS (SELECT 1 FROM public.stat_templates t
                  WHERE t.sport = 'FOOTBALL'
                    AND t.position_group = public.position_group(ps.sport, ps.position)
                    AND ps.stats ? t.stat_key);
    IF v_bad > 0 THEN
        RAISE EXCEPTION '055 gate 2 FAILED: % football players with template keys got NULL template', v_bad;
    END IF;

    SELECT count(*) INTO v_bad
    FROM public.player_stats ps
    CROSS JOIN LATERAL (
        SELECT public.template_block(ps.sport, ps.position, ps.stats, ps.percentiles) AS tmpl
    ) n,
    LATERAL jsonb_each(n.tmpl) m,
    LATERAL jsonb_array_elements(m.value) d
    WHERE ps.sport = 'FOOTBALL' AND n.tmpl IS NOT NULL
      AND d->>'facet' IS NULL;
    IF v_bad > 0 THEN
        RAISE EXCEPTION '055 gate 2 FAILED: % football template items missing a facet', v_bad;
    END IF;
    RAISE NOTICE '055 gate 2 OK: % football players templated, all items faceted', v_with;
END $$;

-- ── Gate 3: datapoints_block sanity ─────────────────────────────────────────
DO $$
DECLARE
    v_bad  BIGINT;
    v_with BIGINT;
BEGIN
    CREATE TEMP TABLE _055_dp ON COMMIT DROP AS
    SELECT ps.sport, ps.percentiles,
           public.datapoints_block(ps.sport, ps.stats, ps.percentiles, ps.scoped_percentiles) AS dp
    FROM public.player_stats ps;
    ANALYZE _055_dp;

    -- NULL exactly when no ranked key has a base definition
    SELECT count(*) INTO v_bad
    FROM _055_dp t
    WHERE t.dp IS NULL
      AND EXISTS (
          SELECT 1 FROM jsonb_object_keys(COALESCE(t.percentiles, '{}'::jsonb)) k(key)
          JOIN public.stat_definitions sd
            ON sd.sport = t.sport AND sd.entity_type = 'player' AND sd.key_name = k.key
          WHERE NOT EXISTS (SELECT 1 FROM public.rate_modes rm
                            WHERE rm.sport = t.sport
                              AND right(k.key, length(rm.suffix)) = rm.suffix));
    IF v_bad > 0 THEN
        RAISE EXCEPTION '055 gate 3 FAILED: % players with ranked base keys got NULL datapoints', v_bad;
    END IF;

    -- no rate-mode sibling leaks into the default-only block
    SELECT count(*) INTO v_bad
    FROM _055_dp t
    CROSS JOIN LATERAL jsonb_array_elements(t.dp) d
    JOIN public.rate_modes rm
      ON rm.sport = t.sport AND right(d->>'key', length(rm.suffix)) = rm.suffix
    WHERE t.dp IS NOT NULL;
    IF v_bad > 0 THEN
        RAISE EXCEPTION '055 gate 3 FAILED: % rate-mode sibling keys leaked into datapoints', v_bad;
    END IF;

    SELECT count(*) INTO v_with FROM _055_dp WHERE dp IS NOT NULL;
    RAISE NOTICE '055 gate 3 OK: datapoints emitted for % player rows, base keys only', v_with;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('055_player_datapoints')
ON CONFLICT (version) DO NOTHING;

COMMIT;

-- ============================================================================
-- 047_stat_templates.sql
-- Per-position COUNTING-STAT TEMPLATES for the Composite pizza. Each wedge shows a
-- real counting stat (QB: attempts / yards / TDs / INTs …) with its within-position
-- percentile, instead of the z-score datapoints — the visual the cards needed.
--
-- Opt-in per position: a template exists only for NFL OFFENSIVE SKILL (QB/RB/WR/TE),
-- where fantasy has long since standardized a rich counting-stat line. Everywhere else
-- — NFL defenders / OL / special teams, and ALL of NBA + football — the `template` block
-- is NULL and the frontend keeps the existing z-score pizza (NBA + football z-wheels are
-- already intuitive; NFL defenders keep their rich defense-facet wheel). Fantasy mode on
-- those sports is just the Phase-2 headline swap, not a pizza change.
--
-- PURELY ADDITIVE METADATA — no data migration, no recompute, no recalc. The wedge
-- percentiles already live in player_stats.percentiles (partitioned by position, with
-- is_inverse applied; INT/fumbles are already is_inverse=true), and the rate-mode
-- siblings (…_per_game/_per_36/_per_season) were ranked by the 045/046 recalcs. This
-- migration only adds the template table + the position_group + template_block functions
-- that the sparkline statement reads.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/047_stat_templates.sql
-- ============================================================================

BEGIN;

-- ── 1. Template table ───────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.stat_templates (
    sport          TEXT    NOT NULL,
    position_group TEXT    NOT NULL,
    stat_key       TEXT    NOT NULL,
    -- Per-rate sibling base, when the legacy alias differs from stat_key. NULL ⇒ the
    -- siblings are <stat_key>_<mode>; NBA 'turnover' → 'tov' (tov_per_36 / tov_per_season).
    rate_base      TEXT,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (sport, position_group, stat_key)
);

COMMENT ON TABLE public.stat_templates IS
    'Per-(sport, position_group) counting-stat templates for the Composite pizza. '
    'Absent position_group => no template => frontend falls back to the z-score pizza.';

-- ── 2. Seeds (NFL offensive skill only) ─────────────────────────────────────
INSERT INTO public.stat_templates (sport, position_group, stat_key, sort_order) VALUES
    -- NFL quarterback
    ('NFL', 'quarterback', 'passing_attempts',      10),
    ('NFL', 'quarterback', 'passing_yards',         20),
    ('NFL', 'quarterback', 'passing_touchdowns',    30),
    ('NFL', 'quarterback', 'passing_interceptions', 40),
    ('NFL', 'quarterback', 'rushing_yards',         50),
    ('NFL', 'quarterback', 'rushing_touchdowns',    60),
    -- NFL running-back
    ('NFL', 'running-back', 'rushing_attempts',     10),
    ('NFL', 'running-back', 'rushing_yards',        20),
    ('NFL', 'running-back', 'rushing_touchdowns',   30),
    ('NFL', 'running-back', 'receptions',           40),
    ('NFL', 'running-back', 'receiving_yards',      50),
    ('NFL', 'running-back', 'receiving_touchdowns', 60),
    -- NFL receiver (WR + TE)
    ('NFL', 'receiver', 'receiving_targets',     10),
    ('NFL', 'receiver', 'receptions',            20),
    ('NFL', 'receiver', 'receiving_yards',       30),
    ('NFL', 'receiver', 'receiving_touchdowns',  40),
    ('NFL', 'receiver', 'receiving_first_downs', 50)
ON CONFLICT (sport, position_group, stat_key) DO NOTHING;
-- (rate_base stays NULL for all NFL keys — their _per_game siblings follow the
-- <stat_key>_per_game convention. The column + template_block logic are kept for
-- future templates whose rate sibling uses a legacy alias, e.g. football shots_total.)

-- ── 3. position_group — raw provider position → template group ──────────────
-- Mirrors the frontend lib/utils/position-groups map, matching the FULL position
-- names the seeder stores (e.g. "Wide Receiver"), plus the abbreviations defensively.
-- Only NFL offensive skill groups map to a template; everything else (NFL defense /
-- OL / special, all NBA + football) returns NULL → no template → z-score pizza.
CREATE OR REPLACE FUNCTION public.position_group(p_sport TEXT, p_position TEXT)
RETURNS TEXT LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE upper(p_sport)
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
        ELSE NULL
    END;
$$;

-- ── 4. template_block — the sparkline payload helper (pure passthrough) ─────
-- Builds the per-position counting-stat template for one player, pre-expanded by rate
-- mode (default + the sport's siblings) so the client switch is free. NULL when the
-- player's position has no template (→ frontend z-pizza fallback). `value` is the raw
-- counting stat (0 when absent); `pct` is the within-position percentile (the
-- percentiles column — partitioned by position, is_inverse already applied).
CREATE OR REPLACE FUNCTION public.template_block(p_sport TEXT, p_position TEXT, p_stats JSONB, p_pct JSONB)
RETURNS JSONB LANGUAGE sql STABLE AS $$
    WITH tmpl AS (
        SELECT t.stat_key, t.rate_base,
               COALESCE(sd.display_name, t.stat_key) AS label,
               t.sort_order
        FROM public.stat_templates t
        LEFT JOIN public.stat_definitions sd
               ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'player'
        WHERE t.sport = upper(p_sport)
          AND t.position_group = public.position_group(p_sport, p_position)
    ),
    modes(mode, suffix) AS (
        VALUES ('default', ''), ('per_game', '_per_game'),
               ('per_season', '_per_season'), ('per_36', '_per_36')
    )
    SELECT jsonb_object_agg(m.mode, m.items)
    FROM (
        SELECT md.mode,
            (SELECT jsonb_agg(jsonb_build_object(
                'key',   t.stat_key,
                'label', t.label,
                -- default mode reads the base key; rate modes read the sibling
                -- (rate_base when the legacy alias differs, e.g. turnover → tov).
                'value', COALESCE((p_stats->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE COALESCE(t.rate_base, t.stat_key) || md.suffix END))::numeric, 0),
                'pct',   COALESCE((p_pct  ->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE COALESCE(t.rate_base, t.stat_key) || md.suffix END))::numeric, 0),
                'sort',  t.sort_order
            ) ORDER BY t.sort_order) FROM tmpl t) AS items
        FROM modes md
        -- emit a mode only when its rate sibling actually exists for this sport
        WHERE EXISTS (SELECT 1 FROM tmpl t
                      WHERE p_stats ? (CASE WHEN md.suffix = '' THEN t.stat_key
                                            ELSE COALESCE(t.rate_base, t.stat_key) || md.suffix END))
    ) m
    WHERE m.items IS NOT NULL;
$$;

-- ── 5. Grants ───────────────────────────────────────────────────────────────
GRANT SELECT ON public.stat_templates TO web_anon, web_user;

COMMIT;

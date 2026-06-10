-- ============================================================================
-- 054_engine_metadata.sql
-- Audit P2b: centralize the rating engine's per-sport hardcodes into metadata so a new
-- sport (or a new rate mode) is data inserts, not edits across five functions.
--
--   public.rate_modes         — (sport, mode) → suffix, denominator key, formula, rounding.
--                               Single source for: the three derived-stats triggers (via the
--                               new public.apply_rate_siblings helper), compute_rating's
--                               v_modes, rating_datapoints' rate-key lookup, fantasy_block /
--                               template_block mode lists, notify_percentile_changed's
--                               sibling exclusion.
--   public.rating_thresholds  — (sport, stat_key) → min_value eligibility gates
--                               (was a CASE in _compute_rating_bundle).
--   stat_definitions           + rate_sibling (which keys get rate siblings; was the
--                               per_X_keys arrays in the triggers) + rate_base (legacy
--                               sibling alias: turnover→tov, shots_total→shots; was
--                               hardcoded in triggers AND stat_templates.rate_base —
--                               the latter column is dropped here).
--
-- Behavior-preserving. Three parity gates abort the transaction on any byte drift:
--   gate 1 — strip all rate siblings + fantasy_points, re-fire triggers, assert every
--            stats jsonb is regenerated identically (strip-first so a missing-sibling
--            bug can't hide behind the jsonb || merge);
--   gate 2 — recompute ratings for every (sport, season), assert all rating_* columns
--            and rating_modes are unchanged;
--   gate 3 — assert fantasy_block / template_block outputs are unchanged for all rows.
--
-- Function bodies are derived from prod (pg_get_functiondef), not canonical (049 lesson).
-- ROUND expression order is preserved exactly per mode (numeric division is not exact at
-- tie boundaries, so v/denom*36 vs v*36/denom can round differently — see formula kinds).
--
-- Apply with: sql/migrate.sh   (or psql -f ...)
-- ============================================================================

BEGIN;

-- ----------------------------------------------------------------------------
-- 0. Parity baselines, captured with the OLD functions before anything changes.
-- ----------------------------------------------------------------------------
CREATE TEMP TABLE _054_before ON COMMIT DROP AS
SELECT ps.player_id, ps.sport, ps.season, COALESCE(ps.league_id, -1) AS lid,
       md5(ps.stats::text)                                            AS stats_md5,
       md5(ROW(ps.rating_composite, ps.rating_specialist, ps.rating_specialty,
               ps.rating_composite_rank, ps.rating_specialist_rank)::text) AS rating_md5,
       md5(COALESCE(ps.rating_breakdown::text,    ''))                AS breakdown_md5,
       md5(COALESCE(ps.rating_scoped_ranks::text, ''))                AS scoped_md5,
       md5(COALESCE(ps.rating_modes::text,        ''))                AS modes_md5,
       public.fantasy_block(ps.stats, ps.percentiles, ps.scoped_percentiles) AS fantasy_before,
       public.template_block(ps.sport, ps.position, ps.stats, ps.percentiles) AS template_before
FROM player_stats ps;

CREATE INDEX ON _054_before (sport, season, player_id, lid);

-- ----------------------------------------------------------------------------
-- 1. Metadata tables.
-- ----------------------------------------------------------------------------
CREATE TABLE public.rate_modes (
    sport        TEXT     NOT NULL REFERENCES sports(id),
    mode         TEXT     NOT NULL,   -- rating_modes / payload-block key ('per_36', ...)
    suffix       TEXT     NOT NULL,   -- sibling stat-key suffix ('_per_36', ...)
    denom_key    TEXT     NOT NULL,   -- stats key of the denominator
    formula      TEXT     NOT NULL CHECK (formula IN ('div_mul', 'mul_div', 'div', 'mul')),
    unit         NUMERIC,             -- 36 / 90; NULL for div|mul
    round_digits SMALLINT NOT NULL,
    PRIMARY KEY (sport, mode)
);
COMMENT ON COLUMN public.rate_modes.formula IS
    'Exact ROUND expression order (numeric division is inexact, so order matters at rounding ties): '
    'div_mul = ROUND(v / denom * unit, d) · mul_div = ROUND(v * unit / denom, d) · '
    'div = ROUND(v / denom, d) · mul = ROUND(v * denom, d)';

INSERT INTO public.rate_modes (sport, mode, suffix, denom_key, formula, unit, round_digits) VALUES
    ('NBA',      'per_36',     '_per_36',     'minutes',        'div_mul', 36,   1),
    ('NBA',      'per_season', '_per_season', 'games_played',   'mul',     NULL, 0),
    ('FOOTBALL', 'per_90',     '_per_90',     'minutes_played', 'mul_div', 90,   3),
    ('FOOTBALL', 'per_game',   '_per_game',   'appearances',    'div',     NULL, 3),
    ('NFL',      'per_game',   '_per_game',   'games_played',   'div',     NULL, 2);

CREATE TABLE public.rating_thresholds (
    sport     TEXT    NOT NULL REFERENCES sports(id),
    stat_key  TEXT    NOT NULL,
    min_value NUMERIC NOT NULL,
    PRIMARY KEY (sport, stat_key)
);
COMMENT ON TABLE public.rating_thresholds IS
    'Rating eligibility gates: a player enters _compute_rating_bundle only when EVERY row '
    'for their sport passes (COALESCE(stats->>stat_key, 0) >= min_value). A sport with no '
    'rows rates nobody.';

INSERT INTO public.rating_thresholds (sport, stat_key, min_value) VALUES
    ('NBA',      'games_played', 30),
    ('NBA',      'minutes',      20),
    ('FOOTBALL', 'appearances',  15),
    ('NFL',      'games_played',  8);

-- Tables created in this transaction have no statistics until we ANALYZE them
-- ourselves (autovacuum can't see uncommitted tables). Without stats the planner
-- can misprice every plan that touches them — including the gate-2 recompute.
ANALYZE public.rate_modes;
ANALYZE public.rating_thresholds;

-- ----------------------------------------------------------------------------
-- 2. stat_definitions: rate_sibling flag (was the triggers' per_X_keys arrays)
--    + rate_base legacy alias (was trigger special-cases + stat_templates.rate_base).
-- ----------------------------------------------------------------------------
ALTER TABLE public.stat_definitions
    ADD COLUMN rate_sibling BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN rate_base    TEXT;
COMMENT ON COLUMN public.stat_definitions.rate_sibling IS
    'TRUE = the derived-stats trigger emits <key or rate_base><rate_modes.suffix> siblings.';
COMMENT ON COLUMN public.stat_definitions.rate_base IS
    'Legacy sibling base name when it differs from key_name (turnover→tov, shots_total→shots).';

DO $$
DECLARE n INT;
BEGIN
    -- NBA: the per_36_keys array (15) + turnover via its legacy 'tov' alias.
    UPDATE public.stat_definitions SET rate_sibling = TRUE
    WHERE sport = 'NBA' AND entity_type = 'player' AND key_name = ANY (ARRAY[
        'pts','reb','ast','stl','blk','pf',
        'oreb','dreb','fgm','fga','fg3m','fg3a','ftm','fta',
        'fantasy_points']);
    GET DIAGNOSTICS n = ROW_COUNT;
    IF n <> 15 THEN RAISE EXCEPTION '054: NBA rate_sibling seeded % rows, expected 15', n; END IF;

    UPDATE public.stat_definitions SET rate_sibling = TRUE, rate_base = 'tov'
    WHERE sport = 'NBA' AND entity_type = 'player' AND key_name = 'turnover';
    GET DIAGNOSTICS n = ROW_COUNT;
    IF n <> 1 THEN RAISE EXCEPTION '054: NBA turnover alias seeded % rows, expected 1', n; END IF;

    -- FOOTBALL: the per_90_keys array (37) + shots_total via its legacy 'shots' alias.
    UPDATE public.stat_definitions SET rate_sibling = TRUE
    WHERE sport = 'FOOTBALL' AND entity_type = 'player' AND key_name = ANY (ARRAY[
        'goals','assists','expected_goals','shots_on_target','key_passes',
        'passes_total','passes_accurate','crosses_total','crosses_accurate',
        'clearances','blocks','duels_total','duels_won','dribbles_attempts',
        'dribbles_success','saves','goals_conceded','saves_insidebox',
        'chances_created','big_chances_created','long_balls','long_balls_won',
        'through_balls','through_balls_won','passes_in_final_third','tackles',
        'tackles_won','interceptions','dribbled_past','dispossessed',
        'possession_lost','turnovers','ball_recovery','aerials','aeriels_won',
        'fouls_committed','fouls_drawn']);
    GET DIAGNOSTICS n = ROW_COUNT;
    IF n <> 37 THEN RAISE EXCEPTION '054: FOOTBALL rate_sibling seeded % rows, expected 37', n; END IF;

    UPDATE public.stat_definitions SET rate_sibling = TRUE, rate_base = 'shots'
    WHERE sport = 'FOOTBALL' AND entity_type = 'player' AND key_name = 'shots_total';
    GET DIAGNOSTICS n = ROW_COUNT;
    IF n <> 1 THEN RAISE EXCEPTION '054: FOOTBALL shots_total alias seeded % rows, expected 1', n; END IF;

    -- NFL: the per_game_keys array (47). No aliases.
    UPDATE public.stat_definitions SET rate_sibling = TRUE
    WHERE sport = 'NFL' AND entity_type = 'player' AND key_name = ANY (ARRAY[
        'passing_completions','passing_attempts','passing_yards',
        'passing_touchdowns','passing_interceptions','sacks_taken','sack_yards_lost',
        'rushing_attempts','rushing_yards','rushing_touchdowns','rushing_first_downs',
        'receptions','receiving_targets','receiving_yards','receiving_touchdowns','receiving_first_downs',
        'total_tackles','solo_tackles','assist_tackles','defensive_sacks','defensive_sack_yards',
        'defensive_interceptions','interception_touchdowns','interception_yards',
        'fumbles_forced','fumbles_recovered','fumbles_touchdowns',
        'tackles_for_loss','passes_defended','qb_hits',
        'fumbles','fumbles_lost',
        'field_goal_attempts','field_goals_made','extra_points_made','total_points','touchbacks',
        'punts','punt_yards','punts_inside_20',
        'kick_returns','kick_return_yards','kick_return_touchdowns',
        'punt_returner_returns','punt_returner_return_yards','punt_return_touchdowns',
        'fantasy_points']);
    GET DIAGNOSTICS n = ROW_COUNT;
    IF n <> 47 THEN RAISE EXCEPTION '054: NFL rate_sibling seeded % rows, expected 47', n; END IF;
END $$;

-- ----------------------------------------------------------------------------
-- 3. Shared sibling emitter + the three triggers rewritten around it.
--    Sport-specific formula sections (fantasy, shooting %s, ratios) are unchanged.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.apply_rate_siblings(p_sport TEXT, p_stats JSONB)
RETURNS JSONB
LANGUAGE plpgsql STABLE
AS $$
DECLARE
    result JSONB := p_stats;
    rm     RECORD;
    sk     RECORD;
    denom  NUMERIC;
    v      NUMERIC;
BEGIN
    FOR rm IN
        SELECT suffix, denom_key, formula, unit, round_digits
        FROM public.rate_modes WHERE sport = p_sport ORDER BY mode
    LOOP
        denom := (p_stats->>rm.denom_key)::NUMERIC;
        IF denom IS NOT NULL AND denom > 0 THEN
            FOR sk IN
                SELECT sd.key_name, COALESCE(sd.rate_base, sd.key_name) || rm.suffix AS out_key
                FROM public.stat_definitions sd
                WHERE sd.sport = p_sport AND sd.entity_type = 'player' AND sd.rate_sibling
            LOOP
                IF p_stats ? sk.key_name THEN
                    v := (p_stats->>sk.key_name)::NUMERIC;
                    IF v IS NOT NULL THEN
                        result := result || jsonb_build_object(sk.out_key,
                            CASE rm.formula
                                WHEN 'div_mul' THEN ROUND(v / denom * rm.unit, rm.round_digits)
                                WHEN 'mul_div' THEN ROUND(v * rm.unit / denom, rm.round_digits)
                                WHEN 'div'     THEN ROUND(v / denom, rm.round_digits)
                                WHEN 'mul'     THEN ROUND(v * denom, rm.round_digits)
                            END);
                    END IF;
                END IF;
            END LOOP;
        END IF;
    END LOOP;
    RETURN result;
END;
$$;

CREATE OR REPLACE FUNCTION nba.compute_derived_player_stats()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    pts NUMERIC; reb NUMERIC; ast NUMERIC; stl NUMERIC; blk NUMERIC;
    fga NUMERIC; fgm NUMERIC; fta NUMERIC; ftm NUMERIC; turnover NUMERIC;
    tsa NUMERIC;
BEGIN
    pts      := (NEW.stats->>'pts')::NUMERIC;
    reb      := (NEW.stats->>'reb')::NUMERIC;
    ast      := (NEW.stats->>'ast')::NUMERIC;
    stl      := (NEW.stats->>'stl')::NUMERIC;
    blk      := (NEW.stats->>'blk')::NUMERIC;
    fga      := (NEW.stats->>'fga')::NUMERIC;
    fgm      := (NEW.stats->>'fgm')::NUMERIC;
    fta      := (NEW.stats->>'fta')::NUMERIC;
    ftm      := (NEW.stats->>'ftm')::NUMERIC;
    turnover := (NEW.stats->>'turnover')::NUMERIC;

    -- Fantasy points (DraftKings, per-game) — computed before the sibling pass so the
    -- rate loops pick up its siblings.
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', nba.fantasy_points(NEW.stats));

    -- Rate siblings (per_36, per_season) — driven by rate_modes + stat_definitions.rate_sibling.
    NEW.stats := public.apply_rate_siblings('NBA', NEW.stats);

    IF pts IS NOT NULL AND fga IS NOT NULL AND fta IS NOT NULL THEN
        tsa := fga + 0.44 * fta;
        IF tsa > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('true_shooting_pct', ROUND(pts / (2 * tsa) * 100, 1)); END IF;
    END IF;

    IF pts IS NOT NULL AND reb IS NOT NULL AND ast IS NOT NULL AND stl IS NOT NULL AND blk IS NOT NULL
       AND fga IS NOT NULL AND fgm IS NOT NULL AND fta IS NOT NULL AND ftm IS NOT NULL AND turnover IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efficiency', ROUND((pts + reb + ast + stl + blk) - ((fga - fgm) + (fta - ftm) + turnover), 1));
    END IF;

    IF fga IS NOT NULL AND fga > 0 AND fgm IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efg_pct', ROUND((fgm + 0.5 * COALESCE((NEW.stats->>'fg3m')::numeric, 0)) / fga * 100, 1));
    END IF;

    IF turnover IS NOT NULL AND turnover > 0 AND ast IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('ast_to_tov', ROUND(ast / turnover, 2));
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION football.compute_derived_player_stats()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    shots_t NUMERIC; shots_on NUMERIC; passes_t NUMERIC; passes_a NUMERIC;
    duels_t NUMERIC; duels_w NUMERIC;
    dribbles_a NUMERIC; dribbles_s NUMERIC;
    tackles NUMERIC; tackles_w NUMERIC;
    crosses_t NUMERIC; crosses_a NUMERIC;
    long_balls_t NUMERIC; long_balls_w NUMERIC;
    aerials_t NUMERIC; aerials_w NUMERIC;
    saves NUMERIC; conceded NUMERIC;
BEGIN
    -- Rate siblings (per_90, per_game) — driven by rate_modes + stat_definitions.rate_sibling.
    NEW.stats := public.apply_rate_siblings('FOOTBALL', NEW.stats);

    shots_t      := (NEW.stats->>'shots_total')::NUMERIC;
    shots_on     := (NEW.stats->>'shots_on_target')::NUMERIC;
    passes_t     := (NEW.stats->>'passes_total')::NUMERIC;
    passes_a     := (NEW.stats->>'passes_accurate')::NUMERIC;
    duels_t      := (NEW.stats->>'duels_total')::NUMERIC;
    duels_w      := (NEW.stats->>'duels_won')::NUMERIC;
    dribbles_a   := (NEW.stats->>'dribbles_attempts')::NUMERIC;
    dribbles_s   := (NEW.stats->>'dribbles_success')::NUMERIC;
    tackles      := (NEW.stats->>'tackles')::NUMERIC;
    tackles_w    := (NEW.stats->>'tackles_won')::NUMERIC;
    crosses_t    := (NEW.stats->>'crosses_total')::NUMERIC;
    crosses_a    := (NEW.stats->>'crosses_accurate')::NUMERIC;
    long_balls_t := (NEW.stats->>'long_balls')::NUMERIC;
    long_balls_w := (NEW.stats->>'long_balls_won')::NUMERIC;
    aerials_t    := (NEW.stats->>'aerials')::NUMERIC;
    aerials_w    := (NEW.stats->>'aeriels_won')::NUMERIC;
    saves        := (NEW.stats->>'saves')::NUMERIC;
    conceded     := (NEW.stats->>'goals_conceded')::NUMERIC;

    IF shots_t IS NOT NULL AND shots_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('shot_accuracy', ROUND(COALESCE(shots_on, 0) / shots_t * 100, 1)); END IF;
    IF passes_t IS NOT NULL AND passes_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('pass_accuracy', ROUND(COALESCE(passes_a, 0) / passes_t * 100, 1)); END IF;
    IF duels_t IS NOT NULL AND duels_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('duel_success_rate', ROUND(COALESCE(duels_w, 0) / duels_t * 100, 1)); END IF;
    IF dribbles_a IS NOT NULL AND dribbles_a > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('dribble_success_rate', ROUND(COALESCE(dribbles_s, 0) / dribbles_a * 100, 1)); END IF;
    IF tackles IS NOT NULL AND tackles > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('tackles_won_percentage', ROUND(COALESCE(tackles_w, 0) / tackles * 100, 1)); END IF;
    IF crosses_t IS NOT NULL AND crosses_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('cross_accuracy', ROUND(COALESCE(crosses_a, 0) / crosses_t * 100, 1)); END IF;
    IF long_balls_t IS NOT NULL AND long_balls_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('long_ball_accuracy', ROUND(COALESCE(long_balls_w, 0) / long_balls_t * 100, 1)); END IF;
    IF aerials_t IS NOT NULL AND aerials_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('aerials_won_percentage', ROUND(COALESCE(aerials_w, 0) / aerials_t * 100, 1)); END IF;
    IF saves IS NOT NULL AND conceded IS NOT NULL AND (saves + conceded) > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('save_pct', ROUND(saves / (saves + conceded) * 100, 1));
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION nfl.compute_derived_player_stats()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    pass_td NUMERIC; pass_int NUMERIC; rec NUMERIC; targets NUMERIC;
BEGIN
    pass_td  := (NEW.stats->>'passing_touchdowns')::NUMERIC;
    pass_int := (NEW.stats->>'passing_interceptions')::NUMERIC;
    rec      := (NEW.stats->>'receptions')::NUMERIC;
    targets  := (NEW.stats->>'receiving_targets')::NUMERIC;

    -- Fantasy points (PPR, season total) — before the sibling pass so its sibling emits.
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', nfl.fantasy_points(NEW.stats));

    -- Rate siblings (per_game) — driven by rate_modes + stat_definitions.rate_sibling.
    NEW.stats := public.apply_rate_siblings('NFL', NEW.stats);

    IF pass_td IS NOT NULL AND pass_int IS NOT NULL AND pass_int > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('td_int_ratio', ROUND(pass_td / pass_int, 2));
    END IF;
    IF rec IS NOT NULL AND targets IS NOT NULL AND targets > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('catch_pct', ROUND(rec / targets * 100, 1));
    END IF;
    RETURN NEW;
END;
$$;

-- ----------------------------------------------------------------------------
-- Gate 1 — strip every rate sibling + fantasy_points, re-fire the new triggers,
-- assert every stats jsonb regenerates byte-identical to the baseline.
-- ----------------------------------------------------------------------------
UPDATE player_stats ps
SET stats = ps.stats - ARRAY(
    SELECT k FROM jsonb_object_keys(ps.stats) k
    WHERE k = 'fantasy_points'
       OR EXISTS (SELECT 1 FROM public.rate_modes rm WHERE k ~ (rm.suffix || '$'))
);

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n
    FROM player_stats ps
    JOIN _054_before b
      ON b.player_id = ps.player_id AND b.sport = ps.sport
     AND b.season = ps.season AND b.lid = COALESCE(ps.league_id, -1)
    WHERE md5(ps.stats::text) <> b.stats_md5;
    IF n <> 0 THEN
        RAISE EXCEPTION '054 gate 1 FAILED: % rows did not regenerate identical stats after strip + trigger re-fire', n;
    END IF;
    RAISE NOTICE '054 gate 1 OK: all stats regenerated byte-identical';
END $$;

-- ----------------------------------------------------------------------------
-- 4. Rating engine: thresholds + modes + rate keys from metadata.
--    Bodies otherwise identical to prod (pg_get_functiondef).
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.rating_datapoints(p_sport text, p_stats jsonb, p_rate_mode text DEFAULT 'total'::text)
 RETURNS TABLE(label text, value numeric, in_comp boolean, in_spec boolean, sign integer, facet text)
 LANGUAGE sql
 STABLE PARALLEL SAFE
AS $function$
    -- Rate siblings resolve as <rate_base><rate_modes.suffix>; rows with NULL rate_base are
    -- mode-invariant (margins, sparse keys with no siblings). 'total' always reads raw.
    -- NBA (turnover keeps its legacy 'tov' sibling base; plus_minus is a margin).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'NBA' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all', 'pts'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all', 'reb'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all', 'ast'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all', 'stl'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all', 'blk'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all', 'fg3m'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all', NULL),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all', 'tov'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all', 'pf'),
        ('Foul Drawing',    NULLIF(p_stats->>'fta','')::numeric,        FALSE, TRUE,  1, 'all', 'fta')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NBA'

    UNION ALL
    -- FOOTBALL (shots_total keeps its legacy 'shots' sibling base; the four sparse
    -- GK/penalty terms have no rate sibling → rate_base NULL → read raw).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'FOOTBALL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all', 'goals'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all', 'assists'),
        ('Shooting',        NULLIF(p_stats->>'shots_total','')::numeric,      TRUE, TRUE,   1, 'all', 'shots'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all', 'key_passes'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        TRUE, TRUE,   1, 'all', 'duels_won'),
        ('Tackling',        NULLIF(p_stats->>'tackles','')::numeric,          TRUE, TRUE,   1, 'all', 'tackles'),
        ('Interceptions',   NULLIF(p_stats->>'interceptions','')::numeric,    TRUE, TRUE,   1, 'all', 'interceptions'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       TRUE, TRUE,   1, 'all', 'clearances'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           TRUE, TRUE,   1, 'all', 'blocks'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    TRUE, TRUE,   1, 'all', 'ball_recovery'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      TRUE, TRUE,   1, 'all', 'fouls_drawn'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all', NULL),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all', 'possession_lost'),
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all', 'saves'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all', NULL),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all', NULL),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all', NULL)
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'FOOTBALL'

    UNION ALL
    -- NFL. 'total' = season totals = Per Season. The three SUM rows branch inline on
    -- p_rate_mode, building sibling keys from the same suffix lookup.
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'NFL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Total Yards',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>'rushing_yards')::numeric,0)
                + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_yards' || rs.suffix))::numeric,(p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>('rushing_yards' || rs.suffix))::numeric,(p_stats->>'rushing_yards')::numeric,0)
                + COALESCE((p_stats->>('receiving_yards' || rs.suffix))::numeric,(p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>('kick_return_yards' || rs.suffix))::numeric,(p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>('punt_returner_return_yards' || rs.suffix))::numeric,(p_stats->>'punt_returner_return_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Touchdowns',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_touchdowns' || rs.suffix))::numeric,(p_stats->>'passing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('rushing_touchdowns' || rs.suffix))::numeric,(p_stats->>'rushing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('receiving_touchdowns' || rs.suffix))::numeric,(p_stats->>'receiving_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('kick_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'kick_return_touchdowns')::numeric,0)
                + COALESCE((p_stats->>('punt_return_touchdowns' || rs.suffix))::numeric,(p_stats->>'punt_return_touchdowns')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Receiving',        NULLIF(p_stats->>'receptions','')::numeric,          TRUE, TRUE,   1, 'offense', 'receptions'),
        ('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost')::numeric,0)
            ELSE
                  COALESCE((p_stats->>('passing_interceptions' || rs.suffix))::numeric,(p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>('fumbles_lost' || rs.suffix))::numeric,(p_stats->>'fumbles_lost')::numeric,0)
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,       TRUE, TRUE,   1, 'defense', 'total_tackles'),
        ('Tackles For Loss', NULLIF(p_stats->>'tackles_for_loss','')::numeric,    TRUE, TRUE,   1, 'defense', 'tackles_for_loss'),
        ('Sacks',            NULLIF(p_stats->>'defensive_sacks','')::numeric,     TRUE, TRUE,   1, 'defense', 'defensive_sacks'),
        ('Pass Defense',     NULLIF(p_stats->>'passes_defended','')::numeric,     TRUE, TRUE,   1, 'defense', 'passes_defended'),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense', 'defensive_interceptions'),
        ('Fumble Recovery',  NULLIF(p_stats->>'fumbles_recovered','')::numeric,   TRUE, TRUE,   1, 'defense', 'fumbles_recovered'),
        ('Field Goals',      NULLIF(p_stats->>'field_goals_made','')::numeric,    TRUE, TRUE,   1, 'special', 'field_goals_made'),
        ('Punting',          NULLIF(p_stats->>'punts_inside_20','')::numeric,     TRUE, TRUE,   1, 'special', 'punts_inside_20')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base)
    WHERE p_sport = 'NFL';
$function$;

CREATE OR REPLACE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, specialist numeric, specialist_rank numeric, specialty text, breakdown jsonb, scoped_ranks jsonb)
 LANGUAGE sql
 STABLE
AS $function$
    WITH dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet
        FROM player_stats ps
        CROSS JOIN LATERAL rating_datapoints(p_sport, ps.stats, p_rate_mode) d
        WHERE ps.sport = p_sport AND ps.season = p_season
          -- Eligibility gates from rating_thresholds: every row for the sport must pass;
          -- a sport with no rows rates nobody (bool_and over zero rows → NULL → FALSE).
          AND COALESCE((
                SELECT bool_and(COALESCE((ps.stats->>rt.stat_key)::numeric, 0) >= rt.min_value)
                FROM public.rating_thresholds rt
                WHERE rt.sport = p_sport
              ), FALSE)
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
$function$;

CREATE OR REPLACE FUNCTION public.compute_rating(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
    v_mode    TEXT;
    -- Alternate-mode list from rate_modes ('total' is always first; the rest are data).
    v_modes   TEXT[] := ARRAY['total'] || COALESCE(
        (SELECT array_agg(mode ORDER BY mode) FROM public.rate_modes WHERE sport = p_sport),
        ARRAY[]::TEXT[]);
BEGIN
    UPDATE player_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_breakdown = NULL, rating_scoped_ranks = NULL, rating_modes = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL OR rating_modes IS NOT NULL);

    FOREACH v_mode IN ARRAY v_modes LOOP
        -- MATERIALIZED: force the bundle to run exactly once. Inlined as a plain
        -- subquery the planner may park it on the inner side of a nestloop and
        -- re-execute the whole z-pipeline per player row (hours instead of
        -- seconds) whenever estimates are off — e.g. a freshly loaded
        -- player_stats that autoanalyze hasn't reached yet.
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
                rating_breakdown       = b.breakdown,
                rating_scoped_ranks    = b.scoped_ranks
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
                    v_mode, jsonb_build_object(
                        'composite',       b.composite,
                        'composite_rank',  b.composite_rank,
                        'specialist',      b.specialist,
                        'specialist_rank', b.specialist_rank,
                        'specialty',       b.specialty,
                        'breakdown',       b.breakdown,
                        'scoped_ranks',    b.scoped_ranks
                    ))
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
        END IF;
    END LOOP;

    RETURN v_updated;
END;
$function$;

-- ----------------------------------------------------------------------------
-- Gate 2 — recompute every (sport, season) with the new engine, assert all rating
-- columns + rating_modes are byte-identical to the baseline.
-- ----------------------------------------------------------------------------
-- Gate 1 just rewrote every player_stats row in this transaction; refresh stats
-- so the recompute below plans against reality, not pre-churn estimates.
ANALYZE public.player_stats;

DO $$
DECLARE r RECORD; n INT;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats ORDER BY sport, season LOOP
        PERFORM public.compute_rating(r.sport, r.season);
    END LOOP;

    SELECT count(*) INTO n
    FROM player_stats ps
    JOIN _054_before b
      ON b.player_id = ps.player_id AND b.sport = ps.sport
     AND b.season = ps.season AND b.lid = COALESCE(ps.league_id, -1)
    WHERE md5(ROW(ps.rating_composite, ps.rating_specialist, ps.rating_specialty,
                  ps.rating_composite_rank, ps.rating_specialist_rank)::text) <> b.rating_md5
       OR md5(COALESCE(ps.rating_breakdown::text,    '')) <> b.breakdown_md5
       OR md5(COALESCE(ps.rating_scoped_ranks::text, '')) <> b.scoped_md5
       OR md5(COALESCE(ps.rating_modes::text,        '')) <> b.modes_md5;
    IF n <> 0 THEN
        RAISE EXCEPTION '054 gate 2 FAILED: % rows changed rating output under the metadata-driven engine', n;
    END IF;
    RAISE NOTICE '054 gate 2 OK: rating output byte-identical across all (sport, season)';
END $$;

-- ----------------------------------------------------------------------------
-- 5. Payload blocks: mode lists from rate_modes. fantasy_block gains per_90
--    (inert until Football fantasy exists, then lights up automatically).
--    template_block reads the alias from stat_definitions.rate_base —
--    stat_templates.rate_base is dropped (single-sourced).
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.fantasy_block(p_stats jsonb, p_pct jsonb, p_scoped jsonb)
 RETURNS jsonb
 LANGUAGE sql
 STABLE
AS $function$
    SELECT CASE WHEN p_stats ? 'fantasy_points' THEN
        jsonb_object_agg(m.mode, m.blk)
    END
    FROM (
        SELECT v.mode,
            CASE WHEN p_stats ? v.key THEN jsonb_build_object(
                'points', (p_stats->>v.key)::numeric,
                'rank',   (p_pct->>v.key)::numeric,
                'scoped_ranks', CASE WHEN p_scoped ? v.key
                                     THEN jsonb_build_object('position', (p_scoped->>v.key)::numeric) END
            ) END AS blk
        FROM (
            SELECT 'default'::text AS mode, 'fantasy_points'::text AS key
            UNION
            SELECT DISTINCT rm.mode, 'fantasy_points' || rm.suffix FROM public.rate_modes rm
        ) v
    ) m
    WHERE m.blk IS NOT NULL;
$function$;

CREATE OR REPLACE FUNCTION public.template_block(p_sport text, p_position text, p_stats jsonb, p_pct jsonb)
 RETURNS jsonb
 LANGUAGE sql
 STABLE
AS $function$
    WITH tmpl AS (
        SELECT t.stat_key,
               -- rate modes read the sibling; the sibling base is the legacy alias from
               -- stat_definitions.rate_base when it differs (e.g. turnover → tov).
               COALESCE(sd.rate_base, t.stat_key) AS rate_base,
               COALESCE(sd.display_name, t.stat_key) AS label,
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
            (SELECT jsonb_agg(jsonb_build_object(
                'key',   t.stat_key,
                'label', t.label,
                'value', COALESCE((p_stats->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE t.rate_base || md.suffix END))::numeric, 0),
                'pct',   COALESCE((p_pct  ->>(CASE WHEN md.suffix = '' THEN t.stat_key
                                                   ELSE t.rate_base || md.suffix END))::numeric, 0),
                'sort',  t.sort_order
            ) ORDER BY t.sort_order) FROM tmpl t) AS items
        FROM modes md
        -- emit a mode only when its rate sibling actually exists for this sport
        WHERE EXISTS (SELECT 1 FROM tmpl t
                      WHERE p_stats ? (CASE WHEN md.suffix = '' THEN t.stat_key
                                            ELSE t.rate_base || md.suffix END))
    ) m
    WHERE m.items IS NOT NULL;
$function$;

ALTER TABLE public.stat_templates DROP COLUMN rate_base;

-- ----------------------------------------------------------------------------
-- Gate 3 — payload blocks byte-identical for every row.
-- ----------------------------------------------------------------------------
DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n
    FROM player_stats ps
    JOIN _054_before b
      ON b.player_id = ps.player_id AND b.sport = ps.sport
     AND b.season = ps.season AND b.lid = COALESCE(ps.league_id, -1)
    WHERE public.fantasy_block(ps.stats, ps.percentiles, ps.scoped_percentiles)
            IS DISTINCT FROM b.fantasy_before
       OR public.template_block(ps.sport, ps.position, ps.stats, ps.percentiles)
            IS DISTINCT FROM b.template_before;
    IF n <> 0 THEN
        RAISE EXCEPTION '054 gate 3 FAILED: % rows changed fantasy_block/template_block output', n;
    END IF;
    RAISE NOTICE '054 gate 3 OK: payload blocks byte-identical';
END $$;

-- ----------------------------------------------------------------------------
-- 6. notify_percentile_changed: sibling exclusion from rate_modes (was a literal
--    regex). Same key set today; a future mode is excluded automatically.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.notify_percentile_changed()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE
    stat_key TEXT;
    new_val NUMERIC;
    old_val NUMERIC;
    entity_type TEXT;
    entity_id INTEGER;
BEGIN
    IF TG_TABLE_NAME = 'player_stats' THEN
        entity_type := 'player';
        entity_id := NEW.player_id;
    ELSIF TG_TABLE_NAME = 'team_stats' THEN
        entity_type := 'team';
        entity_id := NEW.team_id;
    ELSE
        RETURN NEW;
    END IF;

    IF OLD.percentiles IS NOT DISTINCT FROM NEW.percentiles THEN
        RETURN NEW;
    END IF;

    FOR stat_key IN
        SELECT key FROM jsonb_each(NEW.percentiles)
        WHERE key NOT LIKE '\_%'
          -- skip per-rate siblings; the suffix family lives in rate_modes
          AND NOT EXISTS (SELECT 1 FROM public.rate_modes rm WHERE key ~ (rm.suffix || '$'))
          AND jsonb_typeof(value) = 'number'
    LOOP
        new_val := (NEW.percentiles ->> stat_key)::numeric;
        old_val := COALESCE((OLD.percentiles ->> stat_key)::numeric, 0);

        IF (new_val >= 90 AND old_val < 90)
        OR (new_val >= 95 AND old_val < 95)
        OR (new_val >= 99 AND old_val < 99)
        OR (new_val - old_val >= 10) THEN
            PERFORM pg_notify('percentile_changed', json_build_object(
                'entity_type', entity_type,
                'entity_id', entity_id,
                'sport', NEW.sport,
                'season', NEW.season,
                'stat_key', stat_key,
                'old_percentile', old_val,
                'new_percentile', new_val,
                'ts', extract(epoch from now())::bigint
            )::text);
        END IF;
    END LOOP;

    RETURN NEW;
END;
$function$;

INSERT INTO public.schema_migrations (version) VALUES ('054_engine_metadata')
ON CONFLICT (version) DO NOTHING;

COMMIT;

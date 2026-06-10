-- ============================================================================
-- 057 — Football fantasy (FPL-style) — build-order Phase 4
--
-- Football joins the Regular | Fantasy model selector. Mechanically identical to
-- the NFL/NBA fantasy spine (migration 046): fantasy_points is a DERIVED STAT
-- (computed in football.compute_derived_player_stats, NOT the aggregator), so it
-- backfills by re-firing the trigger, picks up its per-90/per-game rate siblings
-- from public.apply_rate_siblings (rate_sibling=TRUE), and recalculate_percentiles
-- ranks it for free → the fantasy headline percentile, no z-engine change.
--
-- Scoring preset: FPL-style, POSITION-DEPENDENT, on SEASON TOTALS (default mode;
-- per_game = ÷ appearances, per_90 = ÷ minutes × 90 fall out of the rate loop).
-- Goal value by position group (GK/DEF 6, MID 5, FWD 4), assists 3, GK save/penalty
-- credit, GK/DEF goals-conceded penalty, discipline deductions.
--
-- Documented approximations (FPL components NOT reconstructable from season
-- aggregates — same spirit as the NBA DraftKings double/triple-double omission):
--   * CLEAN SHEETS (per-match: team concedes 0 while player on pitch 60'+) — omitted.
--     A season goals_conceded total cannot reconstruct per-match shutouts.
--   * BONUS / BPS points (per-match judgement) — omitted.
--   * Appearance points approximated as 2 × appearances (the FPL 1-vs-2 split for
--     <60' vs 60'+ is per-match; most appearances in our data are starts).
--   Penalty goals are already inside `goals` (provider counts them as goals; FPL
--   scores them as ordinary goals) → no separate term, no double count.
--
-- Notifications: the milestone NOTIFY trigger is disabled during the bulk recalc to
-- avoid a one-time storm; per-rate siblings are already excluded by
-- notify_percentile_changed's `_per_(36|90|game|season)$` filter (migration 046).
--
-- Frontend: flip fantasySupported('football') → true (ContentShell). The payload
-- (public.fantasy_block, metadata-driven since 054), Model selector, and the
-- CompositeCard fantasy branch already handle football once the data exists.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/057_football_fantasy.sql
-- ============================================================================

BEGIN;

-- ── 1. Football fantasy scoring (FPL-style, position-dependent) ─────────────
CREATE OR REPLACE FUNCTION football.fantasy_points(p jsonb, p_position text) RETURNS numeric
LANGUAGE sql IMMUTABLE AS $$
    -- FPL-style on season totals. Goal value by position group; GK save/penalty
    -- credit + GK/DEF goals-conceded penalty; discipline deductions. Clean-sheet
    -- and bonus points omitted by design (per-match, unreconstructable from totals);
    -- appearance points approximated as 2/appearance. Unknown position group → ELSE
    -- branch (attacker-style goals, no GK/DEF credit).
    SELECT ROUND(
          2.0 * COALESCE((p->>'appearances')::numeric, 0)
        + (CASE public.position_group('FOOTBALL', p_position)
             WHEN 'goalkeeper' THEN 6.0
             WHEN 'defender'   THEN 6.0
             WHEN 'midfielder' THEN 5.0
             ELSE 4.0 END) * COALESCE((p->>'goals')::numeric, 0)
        + 3.0 * COALESCE((p->>'assists')::numeric, 0)
        - 2.0 * COALESCE((p->>'penalties_missed')::numeric, 0)
        - 1.0 * COALESCE((p->>'yellow_cards')::numeric, 0)
        - 3.0 * COALESCE((p->>'red_cards')::numeric, 0)
        - 2.0 * COALESCE((p->>'own_goals')::numeric, 0)
        + CASE WHEN public.position_group('FOOTBALL', p_position) IN ('goalkeeper','defender')
               THEN -1.0 * FLOOR(COALESCE((p->>'goals_conceded')::numeric, 0) / 2.0)
               ELSE 0 END
        + CASE WHEN public.position_group('FOOTBALL', p_position) = 'goalkeeper'
               THEN FLOOR(COALESCE((p->>'saves')::numeric, 0) / 3.0)
                  + 5.0 * COALESCE((p->>'penalties_saved')::numeric, 0)
               ELSE 0 END
    , 2);
$$;

-- ── 2. stat_definitions — football fantasy keys (idempotent) ────────────────
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'fantasy_points',          'Fantasy Points',          'player', 'fantasy', false, true, true, 90),
    ('FOOTBALL', 'fantasy_points_per_90',   'Fantasy Points Per 90',   'player', 'fantasy', false, true, true, 91),
    ('FOOTBALL', 'fantasy_points_per_game', 'Fantasy Points Per Game', 'player', 'fantasy', false, true, true, 92)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Rate-sibling registry: fantasy_points gets _per_90 / _per_game emitted by
-- public.apply_rate_siblings for every FOOTBALL rate_modes row.
UPDATE public.stat_definitions SET rate_sibling = TRUE
WHERE sport = 'FOOTBALL' AND entity_type = 'player' AND key_name = 'fantasy_points';

-- ── 3. Derived trigger — compute fantasy_points before the rate siblings ────
-- Identical to the canonical football.compute_derived_player_stats except the new
-- fantasy_points line, placed BEFORE apply_rate_siblings so its siblings emit.
CREATE OR REPLACE FUNCTION football.compute_derived_player_stats()
RETURNS TRIGGER AS $$
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
    -- Fantasy points (FPL-style, season total) — before apply_rate_siblings so its
    -- per_90 / per_game siblings emit (fantasy_points is rate_sibling=TRUE).
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', football.fantasy_points(NEW.stats, NEW.position));

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
$$ LANGUAGE plpgsql;

-- ── 4. Backfill: materialize fantasy_points + siblings, then rank ───────────
ALTER TABLE player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;

UPDATE player_stats SET stats = stats WHERE sport = 'FOOTBALL';

DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats
             WHERE sport = 'FOOTBALL' ORDER BY season LOOP
        PERFORM recalculate_percentiles(r.sport, r.season);
    END LOOP;
END $$;

ALTER TABLE player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;

-- ── 5. Gates ────────────────────────────────────────────────────────────────
DO $$
DECLARE
    v_pts      BIGINT;
    v_ranked   BIGINT;
    v_sib90    BIGINT;
    v_sibgame  BIGINT;
    v_drift    BIGINT;
BEGIN
    -- (1) populated + nonzero
    SELECT count(*) INTO v_pts FROM player_stats
        WHERE sport='FOOTBALL' AND stats ? 'fantasy_points' AND (stats->>'fantasy_points')::numeric <> 0;
    IF v_pts = 0 THEN RAISE EXCEPTION '057 gate 1 FAIL: no nonzero FOOTBALL fantasy_points'; END IF;

    -- (2) ranked by recalculate_percentiles
    SELECT count(*) INTO v_ranked FROM player_stats
        WHERE sport='FOOTBALL' AND percentiles ? 'fantasy_points';
    IF v_ranked = 0 THEN RAISE EXCEPTION '057 gate 2 FAIL: fantasy_points not ranked'; END IF;

    -- (3) rate siblings emitted (per_90 needs minutes_played, per_game needs appearances)
    SELECT count(*) INTO v_sib90   FROM player_stats WHERE sport='FOOTBALL' AND stats ? 'fantasy_points_per_90';
    SELECT count(*) INTO v_sibgame FROM player_stats WHERE sport='FOOTBALL' AND stats ? 'fantasy_points_per_game';
    IF v_sib90 = 0 OR v_sibgame = 0 THEN
        RAISE EXCEPTION '057 gate 3 FAIL: missing fantasy siblings (per_90=%, per_game=%)', v_sib90, v_sibgame;
    END IF;

    -- (4) consistency: every stored fantasy_points equals the function recomputation
    --     (validates the trigger wired the position-dependent formula correctly).
    SELECT count(*) INTO v_drift FROM player_stats
        WHERE sport='FOOTBALL'
          AND (stats->>'fantasy_points')::numeric
              IS DISTINCT FROM football.fantasy_points(stats, position);
    IF v_drift > 0 THEN
        RAISE EXCEPTION '057 gate 4 FAIL: % rows where stored fantasy_points != football.fantasy_points()', v_drift;
    END IF;

    RAISE NOTICE '057 OK: % nonzero fantasy_points (% ranked, per_90=%, per_game=%), 0 formula drift',
        v_pts, v_ranked, v_sib90, v_sibgame;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('057_football_fantasy')
ON CONFLICT (version) DO NOTHING;

COMMIT;

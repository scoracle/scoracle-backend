-- ============================================================================
-- 046_fantasy_points.sql
-- FANTASY scope (NFL + NBA). Adds box-score-derived fantasy points as a first-class
-- derived stat, with a per-X rate family (mirroring migration 045's siblings) so the
-- Per Season / Per Game / Per-X selector cross-applies to fantasy. This is the data
-- spine for the Regular | Fantasy model selector + the fantasy leaderboard/roster.
--
-- Scoring presets (one per sport):
--   NFL → standard PPR     (on season totals → season-total fantasy points)
--   NBA → DraftKings        (on per-game averages → per-game fantasy points;
--                            the per_season sibling = × games_played = season total.
--                            DK double/triple-double bonus is per-game and cannot be
--                            reconstructed from season averages → omitted by design.)
--
-- Architecture: fantasy_points is computed in the DERIVED-STATS TRIGGER (not the
-- aggregator), so (a) it is a normal derived stat in the right home, (b) it backfills
-- on existing rows by simply re-firing the trigger (no event re-aggregation), and (c)
-- adding 'fantasy_points' to each sport's per-X key array gives the rate siblings for
-- free. recalculate_percentiles then ranks it → the headline percentile, no z-engine
-- change (fantasy is a points headline, a different axis from the z-composite).
--
-- Notifications: per-rate SIBLING keys (…_per_36/_per_90/_per_game/_per_season) are
-- excluded from notify_percentile_changed (and the Go catch-up sweep) — a player who
-- is elite in "Points" shouldn't also fire "Points Per 36"; this also silences the
-- fantasy siblings while keeping the base fantasy_points notifiable. The milestone
-- trigger is disabled during the in-migration bulk recalc to avoid a one-time storm.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/046_fantasy_points.sql
-- ============================================================================

BEGIN;

-- ── 1. Fantasy scoring functions (per-sport presets + dispatcher) ───────────
CREATE OR REPLACE FUNCTION nba.fantasy_points(p jsonb) RETURNS numeric
LANGUAGE sql IMMUTABLE AS $$
    -- DraftKings NBA on per-game averages → per-game fantasy points. DD/TD bonus
    -- omitted (per-game, unreconstructable from season averages).
    SELECT ROUND(
          1.0  * COALESCE((p->>'pts')::numeric, 0)
        + 0.5  * COALESCE((p->>'fg3m')::numeric, 0)
        + 1.25 * COALESCE((p->>'reb')::numeric, 0)
        + 1.5  * COALESCE((p->>'ast')::numeric, 0)
        + 2.0  * COALESCE((p->>'stl')::numeric, 0)
        + 2.0  * COALESCE((p->>'blk')::numeric, 0)
        - 0.5  * COALESCE((p->>'turnover')::numeric, 0)
    , 2);
$$;

CREATE OR REPLACE FUNCTION nfl.fantasy_points(p jsonb) RETURNS numeric
LANGUAGE sql IMMUTABLE AS $$
    -- Standard PPR on season totals → season-total fantasy points.
    SELECT ROUND(
          0.04 * COALESCE((p->>'passing_yards')::numeric, 0)
        + 4.0  * COALESCE((p->>'passing_touchdowns')::numeric, 0)
        - 2.0  * COALESCE((p->>'passing_interceptions')::numeric, 0)
        + 0.1  * COALESCE((p->>'rushing_yards')::numeric, 0)
        + 6.0  * COALESCE((p->>'rushing_touchdowns')::numeric, 0)
        + 1.0  * COALESCE((p->>'receptions')::numeric, 0)
        + 0.1  * COALESCE((p->>'receiving_yards')::numeric, 0)
        + 6.0  * COALESCE((p->>'receiving_touchdowns')::numeric, 0)
        - 2.0  * COALESCE((p->>'fumbles_lost')::numeric, 0)
    , 2);
$$;

-- ── 2. fantasy_block — the sparkline payload helper (pure passthrough) ───────
-- Pre-expands fantasy points + percentile rank by rate mode so the client switch is
-- free (mirrors rating_modes). NULL when the entity has no fantasy_points at all.
CREATE OR REPLACE FUNCTION public.fantasy_block(p_stats jsonb, p_pct jsonb, p_scoped jsonb)
RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$
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
        FROM (VALUES
            ('default',    'fantasy_points'),
            ('per_game',   'fantasy_points_per_game'),
            ('per_season', 'fantasy_points_per_season'),
            ('per_36',     'fantasy_points_per_36')
        ) v(mode, key)
    ) m
    WHERE m.blk IS NOT NULL;
$$;

-- ── 3. stat_definitions — fantasy keys per sport (idempotent) ───────────────
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'fantasy_points',            'Fantasy Points',           'player', 'fantasy', false, true, true, 90),
    ('NBA', 'fantasy_points_per_36',     'Fantasy Points Per 36',    'player', 'fantasy', false, true, true, 91),
    ('NBA', 'fantasy_points_per_season', 'Total Fantasy Points',     'player', 'fantasy', false, true, true, 92),
    ('NFL', 'fantasy_points',            'Fantasy Points',           'player', 'fantasy', false, true, true, 90),
    ('NFL', 'fantasy_points_per_game',   'Fantasy Points Per Game',  'player', 'fantasy', false, true, true, 91)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ── 4. Derived BEFORE-triggers — compute fantasy_points + rate siblings ─────
-- NBA: cumulative of migration 045 (per_36 + per_season) PLUS fantasy_points. The
-- base is computed before the loops; 'fantasy_points' joins per_36_keys so both the
-- per_36 and per_season loops emit its siblings.
CREATE OR REPLACE FUNCTION nba.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC;
    s TEXT;
    v NUMERIC;
    pts NUMERIC; reb NUMERIC; ast NUMERIC; stl NUMERIC; blk NUMERIC;
    fga NUMERIC; fgm NUMERIC; fta NUMERIC; ftm NUMERIC; turnover NUMERIC;
    tsa NUMERIC; gp NUMERIC;
    per_36_keys TEXT[] := ARRAY[
        'pts','reb','ast','stl','blk','pf',
        'oreb','dreb','fgm','fga','fg3m','fg3a','ftm','fta',
        'fantasy_points'
    ];
BEGIN
    minutes  := (NEW.stats->>'minutes')::NUMERIC;
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

    -- Fantasy points (DraftKings, per-game) — computed before the rate loops so the
    -- per_36 / per_season loops pick up its siblings.
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', nba.fantasy_points(NEW.stats));

    IF minutes IS NOT NULL AND minutes > 0 THEN
        FOREACH s IN ARRAY per_36_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_36', ROUND(v / minutes * 36, 1));
                END IF;
            END IF;
        END LOOP;
        IF turnover IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('tov_per_36', ROUND(turnover / minutes * 36, 1));
        END IF;
    END IF;

    -- Per-SEASON cumulative totals (avg × games_played).
    gp := (NEW.stats->>'games_played')::NUMERIC;
    IF gp IS NOT NULL AND gp > 0 THEN
        FOREACH s IN ARRAY per_36_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_season', ROUND(v * gp, 0));
                END IF;
            END IF;
        END LOOP;
        IF turnover IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('tov_per_season', ROUND(turnover * gp, 0));
        END IF;
    END IF;

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
$$ LANGUAGE plpgsql;

-- NFL: fantasy_points (PPR season total) before the per_game loop; 'fantasy_points'
-- joins per_game_keys so the loop emits fantasy_points_per_game.
CREATE OR REPLACE FUNCTION nfl.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    gp NUMERIC;
    s TEXT;
    v NUMERIC;
    pass_td NUMERIC; pass_int NUMERIC; rec NUMERIC; targets NUMERIC;
    per_game_keys TEXT[] := ARRAY[
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
        'fantasy_points'
    ];
BEGIN
    gp       := (NEW.stats->>'games_played')::NUMERIC;
    pass_td  := (NEW.stats->>'passing_touchdowns')::NUMERIC;
    pass_int := (NEW.stats->>'passing_interceptions')::NUMERIC;
    rec      := (NEW.stats->>'receptions')::NUMERIC;
    targets  := (NEW.stats->>'receiving_targets')::NUMERIC;

    -- Fantasy points (PPR, season total) — before the per_game loop so its sibling emits.
    NEW.stats := NEW.stats || jsonb_build_object('fantasy_points', nfl.fantasy_points(NEW.stats));

    IF gp IS NOT NULL AND gp > 0 THEN
        FOREACH s IN ARRAY per_game_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_game', ROUND(v / gp, 2));
                END IF;
            END IF;
        END LOOP;
    END IF;

    IF pass_td IS NOT NULL AND pass_int IS NOT NULL AND pass_int > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('td_int_ratio', ROUND(pass_td / pass_int, 2));
    END IF;
    IF rec IS NOT NULL AND targets IS NOT NULL AND targets > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('catch_pct', ROUND(rec / targets * 100, 1));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── 5. notify_percentile_changed — skip per-rate SIBLING keys ───────────────
-- A player elite in "Points" shouldn't also fire "Points Per 36"; this also silences
-- the fantasy_points_per_* siblings while keeping base fantasy_points notifiable.
CREATE OR REPLACE FUNCTION notify_percentile_changed()
RETURNS TRIGGER AS $$
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
          AND key !~ '_per_(36|90|game|season)$'
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
$$ LANGUAGE plpgsql;

-- ── 6. Backfill: populate fantasy_points + siblings on existing rows, then rank.
-- The milestone NOTIFY trigger is disabled during the bulk recalc to avoid a one-time
-- notification storm; live seeds resume normal notification behavior afterward.
ALTER TABLE player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;

UPDATE player_stats SET stats = stats WHERE sport IN ('NBA', 'NFL');

DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats
             WHERE sport IN ('NBA', 'NFL') ORDER BY sport, season LOOP
        PERFORM recalculate_percentiles(r.sport, r.season);
    END LOOP;
END $$;

ALTER TABLE player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;

-- ── 7. Smoke gate — fantasy_points populated + ranked for both sports ───────
DO $$
DECLARE
    v_nba_pts BIGINT;
    v_nfl_pts BIGINT;
    v_ranked  BIGINT;
BEGIN
    SELECT count(*) INTO v_nba_pts FROM player_stats WHERE sport='NBA' AND stats ? 'fantasy_points' AND (stats->>'fantasy_points')::numeric <> 0;
    SELECT count(*) INTO v_nfl_pts FROM player_stats WHERE sport='NFL' AND stats ? 'fantasy_points' AND (stats->>'fantasy_points')::numeric <> 0;
    SELECT count(*) INTO v_ranked  FROM player_stats WHERE sport IN ('NBA','NFL') AND percentiles ? 'fantasy_points';
    IF v_nba_pts = 0 OR v_nfl_pts = 0 THEN
        RAISE EXCEPTION 'SMOKE FAILURE: fantasy_points not populated (NBA=%, NFL=%)', v_nba_pts, v_nfl_pts;
    END IF;
    IF v_ranked = 0 THEN
        RAISE EXCEPTION 'SMOKE FAILURE: fantasy_points has no percentile rank — recalculate_percentiles did not pick it up';
    END IF;
    RAISE NOTICE 'FANTASY OK: fantasy_points populated (NBA % / NFL % nonzero) and ranked for % rows', v_nba_pts, v_nfl_pts, v_ranked;
END $$;

COMMIT;

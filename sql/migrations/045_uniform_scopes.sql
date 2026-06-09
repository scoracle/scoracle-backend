-- ============================================================================
-- 045_uniform_scopes.sql
-- Make the PLAYER per-X rate vocabulary UNIFORM across sports: every sport now
-- speaks Per Season / Per Game / Per-X. Adds the missing tier per sport:
--   NBA      → NEW 'per_season'  (avg × games_played — total seasonal volume)
--   FOOTBALL → NEW 'per_game'    (season total ÷ appearances)
--   NFL      → unchanged (its 'total' already = season totals; 'per_game' exists;
--              no per-x derivable from a box-score-only feed — no snap counts)
--
-- Why: NBA box scores are stored as per-game AVERAGES, so until now there was no
-- way to express total seasonal value (the durable 70-game season outranking a
-- higher per-game rate from a player who missed time). Football had season totals
-- + per-90 but no per-game middle tier. This closes both gaps with the SAME
-- machinery migration 042 introduced — a per-row rate sibling + a v_modes entry.
--
-- STRICTLY ADDITIVE. Default ('total') columns AND the existing alternate blocks
-- (NBA per_36 / FB per_90 / NFL per_game) stay BYTE-IDENTICAL — an in-transaction
-- PARITY GATE aborts on any drift. The new modes live only as new keys inside the
-- existing rating_modes JSONB.
--
-- Mechanics mirror 042/012/014:
--   * The derived BEFORE-trigger writes <base>_per_season (NBA) / <base>_per_game
--     (FB) siblings — same loop shape as the per_36 / per_90 families.
--   * rating_datapoints gains a per-row rate_key2 literal (the new mode's sibling).
--     The wrapping CASE routes total → raw, per_36/per_90 → rate_key, the NEW mode
--     → rate_key2. NFL branch is untouched (no new mode).
--   * compute_rating's v_modes appends the new mode per sport.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/045_uniform_scopes.sql
-- ============================================================================

BEGIN;

-- ── 0. Parity snapshot (BEFORE any redefinition / recompute) ────────────────
-- Captures default-mode columns AND the existing alternate-mode blocks so we can
-- prove zero drift after the recompute. ON COMMIT DROP — gone when the txn ends.
CREATE TEMP TABLE _parity_before ON COMMIT DROP AS
SELECT player_id, sport, season, COALESCE(league_id, 0) AS league_id,
       rating_composite, rating_specialist, rating_specialty,
       rating_composite_rank, rating_specialist_rank,
       rating_breakdown, rating_scoped_ranks, rating_modes
FROM player_stats
WHERE sport IN ('FOOTBALL', 'NBA', 'NFL');

-- ── 1. stat_definitions — new derived sibling keys (idempotent) ─────────────
-- NBA per-season totals (avg × games_played).
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'pts_per_season',  'Total Points',        'player', 'advanced', false, true, true,  50),
    ('NBA', 'reb_per_season',  'Total Rebounds',      'player', 'advanced', false, true, true,  51),
    ('NBA', 'ast_per_season',  'Total Assists',       'player', 'advanced', false, true, true,  52),
    ('NBA', 'stl_per_season',  'Total Steals',        'player', 'advanced', false, true, true,  53),
    ('NBA', 'blk_per_season',  'Total Blocks',        'player', 'advanced', false, true, true,  54),
    ('NBA', 'tov_per_season',  'Total Turnovers',     'player', 'advanced', true,  true, true,  55),
    ('NBA', 'pf_per_season',   'Total Fouls',         'player', 'advanced', true,  true, false, 56),
    ('NBA', 'oreb_per_season', 'Total Off Rebounds',  'player', 'advanced', false, true, true,  57),
    ('NBA', 'dreb_per_season', 'Total Def Rebounds',  'player', 'advanced', false, true, true,  58),
    ('NBA', 'fgm_per_season',  'Total FG Made',       'player', 'advanced', false, true, true,  59),
    ('NBA', 'fga_per_season',  'Total FG Attempted',  'player', 'advanced', false, true, true,  60),
    ('NBA', 'fg3m_per_season', 'Total 3PT Made',      'player', 'advanced', false, true, true,  61),
    ('NBA', 'fg3a_per_season', 'Total 3PT Attempted', 'player', 'advanced', false, true, true,  62),
    ('NBA', 'ftm_per_season',  'Total FT Made',       'player', 'advanced', false, true, true,  63),
    ('NBA', 'fta_per_season',  'Total FT Attempted',  'player', 'advanced', false, true, true,  64)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- FOOTBALL per-game averages (season total ÷ appearances). Mirrors the per-90 set.
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'goals_per_game',             'Goals Per Game',           'player', 'scoring',    false, true, true, 300),
    ('FOOTBALL', 'assists_per_game',           'Assists Per Game',         'player', 'scoring',    false, true, true, 301),
    ('FOOTBALL', 'expected_goals_per_game',    'xG Per Game',              'player', 'scoring',    false, true, true, 302),
    ('FOOTBALL', 'shots_per_game',             'Shots Per Game',           'player', 'shooting',   false, true, true, 303),
    ('FOOTBALL', 'shots_on_target_per_game',   'Shots on Target/Game',     'player', 'shooting',   false, true, true, 304),
    ('FOOTBALL', 'key_passes_per_game',        'Key Passes Per Game',      'player', 'passing',    false, true, true, 305),
    ('FOOTBALL', 'passes_total_per_game',      'Passes Per Game',          'player', 'passing',    false, true, true, 306),
    ('FOOTBALL', 'passes_accurate_per_game',   'Accurate Passes/Game',     'player', 'passing',    false, true, true, 307),
    ('FOOTBALL', 'crosses_total_per_game',     'Crosses Per Game',         'player', 'passing',    false, true, true, 308),
    ('FOOTBALL', 'crosses_accurate_per_game',  'Accurate Crosses/Game',    'player', 'passing',    false, true, true, 309),
    ('FOOTBALL', 'clearances_per_game',        'Clearances Per Game',      'player', 'defensive',  false, true, true, 310),
    ('FOOTBALL', 'blocks_per_game',            'Blocks Per Game',          'player', 'defensive',  false, true, true, 311),
    ('FOOTBALL', 'duels_total_per_game',       'Duels Per Game',           'player', 'duels',      false, true, true, 312),
    ('FOOTBALL', 'duels_won_per_game',         'Duels Won Per Game',       'player', 'duels',      false, true, true, 313),
    ('FOOTBALL', 'dribbles_attempts_per_game', 'Dribble Attempts/Game',    'player', 'dribbling',  false, true, true, 314),
    ('FOOTBALL', 'dribbles_success_per_game',  'Successful Dribbles/Game', 'player', 'dribbling',  false, true, true, 315),
    ('FOOTBALL', 'saves_per_game',             'Saves Per Game',           'player', 'goalkeeper', false, true, true, 316),
    ('FOOTBALL', 'goals_conceded_per_game',    'Goals Conceded/Game',      'player', 'goalkeeper', true,  true, true, 317),
    ('FOOTBALL', 'saves_insidebox_per_game',   'Saves Inside Box/Game',    'player', 'goalkeeper', false, true, true, 318),
    ('FOOTBALL', 'chances_created_per_game',   'Chances Created/Game',     'player', 'passing',    false, true, true, 319),
    ('FOOTBALL', 'big_chances_created_per_game','Big Chances Created/Game','player', 'passing',    false, true, true, 320),
    ('FOOTBALL', 'long_balls_per_game',        'Long Balls Per Game',      'player', 'passing',    false, true, true, 321),
    ('FOOTBALL', 'long_balls_won_per_game',    'Long Balls Won/Game',      'player', 'passing',    false, true, true, 322),
    ('FOOTBALL', 'through_balls_per_game',     'Through Balls Per Game',   'player', 'passing',    false, true, true, 323),
    ('FOOTBALL', 'through_balls_won_per_game', 'Through Balls Won/Game',   'player', 'passing',    false, true, true, 324),
    ('FOOTBALL', 'passes_in_final_third_per_game','Final-Third Passes/Game','player','passing',    false, true, true, 325),
    ('FOOTBALL', 'tackles_per_game',           'Tackles Per Game',         'player', 'defensive',  false, true, true, 326),
    ('FOOTBALL', 'tackles_won_per_game',       'Tackles Won Per Game',     'player', 'defensive',  false, true, true, 327),
    ('FOOTBALL', 'interceptions_per_game',     'Interceptions/Game',       'player', 'defensive',  false, true, true, 328),
    ('FOOTBALL', 'dribbled_past_per_game',     'Dribbled Past Per Game',   'player', 'defensive',  true,  true, true, 329),
    ('FOOTBALL', 'dispossessed_per_game',      'Dispossessed Per Game',    'player', 'possession', true,  true, true, 330),
    ('FOOTBALL', 'possession_lost_per_game',   'Possession Lost/Game',     'player', 'possession', true,  true, true, 331),
    ('FOOTBALL', 'turnovers_per_game',         'Turnovers Per Game',       'player', 'possession', true,  true, true, 332),
    ('FOOTBALL', 'ball_recovery_per_game',     'Ball Recoveries/Game',     'player', 'defensive',  false, true, true, 333),
    ('FOOTBALL', 'aerials_per_game',           'Aerials Per Game',         'player', 'duels',      false, true, true, 334),
    ('FOOTBALL', 'aeriels_won_per_game',       'Aerials Won Per Game',     'player', 'duels',      false, true, true, 335),
    ('FOOTBALL', 'fouls_committed_per_game',   'Fouls Committed/Game',     'player', 'discipline', true,  true, true, 336),
    ('FOOTBALL', 'fouls_drawn_per_game',       'Fouls Drawn Per Game',     'player', 'discipline', false, true, true, 337)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ── 2. Derived BEFORE-triggers — add the new sibling family per sport ───────
-- (Identical to the canonical sql/nba.sql + sql/football.sql edits; carried here
-- so applying this migration updates the live functions too.)

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
        'oreb','dreb','fgm','fga','fg3m','fg3a','ftm','fta'
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

    -- Per-SEASON cumulative totals (Per Season rate mode): per-game average × games
    -- played. The only mode that captures total seasonal volume.
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

CREATE OR REPLACE FUNCTION football.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC;
    apps NUMERIC;
    s TEXT;
    v NUMERIC;
    per_90_keys TEXT[] := ARRAY[
        'goals','assists','expected_goals','shots_on_target','key_passes',
        'passes_total','passes_accurate','crosses_total','crosses_accurate',
        'clearances','blocks','duels_total','duels_won','dribbles_attempts',
        'dribbles_success','saves','goals_conceded','saves_insidebox',
        'chances_created','big_chances_created','long_balls','long_balls_won',
        'through_balls','through_balls_won','passes_in_final_third','tackles',
        'tackles_won','interceptions','dribbled_past','dispossessed',
        'possession_lost','turnovers','ball_recovery','aerials','aeriels_won',
        'fouls_committed','fouls_drawn'
    ];
    shots_t NUMERIC; shots_on NUMERIC; passes_t NUMERIC; passes_a NUMERIC;
    duels_t NUMERIC; duels_w NUMERIC;
    dribbles_a NUMERIC; dribbles_s NUMERIC;
    tackles NUMERIC; tackles_w NUMERIC;
    crosses_t NUMERIC; crosses_a NUMERIC;
    long_balls_t NUMERIC; long_balls_w NUMERIC;
    aerials_t NUMERIC; aerials_w NUMERIC;
    saves NUMERIC; conceded NUMERIC;
BEGIN
    minutes := (NEW.stats->>'minutes_played')::NUMERIC;

    IF minutes IS NOT NULL AND minutes > 0 THEN
        FOREACH s IN ARRAY per_90_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_90', ROUND(v * 90 / minutes, 3));
                END IF;
            END IF;
        END LOOP;
        IF NEW.stats ? 'shots_total' THEN
            v := (NEW.stats->>'shots_total')::NUMERIC;
            IF v IS NOT NULL THEN
                NEW.stats := NEW.stats || jsonb_build_object('shots_per_90', ROUND(v * 90 / minutes, 3));
            END IF;
        END IF;
    END IF;

    -- Per-GAME averages (Per Game rate mode): season total ÷ appearances.
    apps := (NEW.stats->>'appearances')::NUMERIC;
    IF apps IS NOT NULL AND apps > 0 THEN
        FOREACH s IN ARRAY per_90_keys LOOP
            IF NEW.stats ? s THEN
                v := (NEW.stats->>s)::NUMERIC;
                IF v IS NOT NULL THEN
                    NEW.stats := NEW.stats || jsonb_build_object(s || '_per_game', ROUND(v / apps, 3));
                END IF;
            END IF;
        END LOOP;
        IF NEW.stats ? 'shots_total' THEN
            v := (NEW.stats->>'shots_total')::NUMERIC;
            IF v IS NOT NULL THEN
                NEW.stats := NEW.stats || jsonb_build_object('shots_per_game', ROUND(v / apps, 3));
            END IF;
        END IF;
    END IF;

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

-- ── 3. rating_datapoints — gains a per-row rate_key2 (the NEW mode's sibling) ─
-- The wrapping CASE: total/mode-invariant → raw; the NEW mode (per_season / per_game)
-- → rate_key2; otherwise (per_36 / per_90) → rate_key (verbatim 042 behavior).
-- NFL branch is unchanged from 042 (no new mode; 'total' already = season totals).
CREATE OR REPLACE FUNCTION rating_datapoints(p_sport TEXT, p_stats JSONB, p_rate_mode TEXT DEFAULT 'total')
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA (per_36 + NEW per_season; turnover → tov_* special sibling names;
    -- plus_minus is a margin → mode-invariant, both rate keys NULL).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_key IS NULL THEN v.raw_value
                WHEN p_rate_mode = 'per_season' THEN COALESCE(NULLIF(p_stats->>v.rate_key2, '')::numeric, v.raw_value)
                ELSE COALESCE(NULLIF(p_stats->>v.rate_key, '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all', 'pts_per_36',  'pts_per_season'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all', 'reb_per_36',  'reb_per_season'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all', 'ast_per_36',  'ast_per_season'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all', 'stl_per_36',  'stl_per_season'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all', 'blk_per_36',  'blk_per_season'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all', 'fg3m_per_36', 'fg3m_per_season'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all', NULL,          NULL),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all', 'tov_per_36',  'tov_per_season'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all', 'pf_per_36',   'pf_per_season'),
        ('Foul Drawing',    NULLIF(p_stats->>'fta','')::numeric,        FALSE, TRUE,  1, 'all', 'fta_per_36',  'fta_per_season')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_key, rate_key2)
    WHERE p_sport = 'NBA'

    UNION ALL
    -- FOOTBALL (per_90 + NEW per_game; shots_total → shots_* special name; the four
    -- sparse GK/penalty terms have no rate sibling → both keys NULL → read raw).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_key IS NULL THEN v.raw_value
                WHEN p_rate_mode = 'per_game' THEN COALESCE(NULLIF(p_stats->>v.rate_key2, '')::numeric, v.raw_value)
                ELSE COALESCE(NULLIF(p_stats->>v.rate_key, '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all', 'goals_per_90',            'goals_per_game'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all', 'assists_per_90',          'assists_per_game'),
        ('Shooting',        NULLIF(p_stats->>'shots_total','')::numeric,      TRUE, TRUE,   1, 'all', 'shots_per_90',            'shots_per_game'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate_per_90',  'passes_accurate_per_game'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all', 'key_passes_per_90',       'key_passes_per_game'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success_per_90', 'dribbles_success_per_game'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        TRUE, TRUE,   1, 'all', 'duels_won_per_90',        'duels_won_per_game'),
        ('Tackling',        NULLIF(p_stats->>'tackles','')::numeric,          TRUE, TRUE,   1, 'all', 'tackles_per_90',          'tackles_per_game'),
        ('Interceptions',   NULLIF(p_stats->>'interceptions','')::numeric,    TRUE, TRUE,   1, 'all', 'interceptions_per_90',    'interceptions_per_game'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       TRUE, TRUE,   1, 'all', 'clearances_per_90',       'clearances_per_game'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           TRUE, TRUE,   1, 'all', 'blocks_per_90',           'blocks_per_game'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    TRUE, TRUE,   1, 'all', 'ball_recovery_per_90',    'ball_recovery_per_game'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      TRUE, TRUE,   1, 'all', 'fouls_drawn_per_90',      'fouls_drawn_per_game'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all', NULL,                      NULL),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all', 'possession_lost_per_90', 'possession_lost_per_game'),
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all', 'saves_per_90',            'saves_per_game'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all', NULL,                      NULL),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all', NULL,                      NULL),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all', NULL,                      NULL)
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_key, rate_key2)
    WHERE p_sport = 'FOOTBALL'

    UNION ALL
    -- NFL (per_game only — UNCHANGED from 042). 'total' = season totals = Per Season.
    -- The three SUM rows branch inline on p_rate_mode (summing _per_game siblings).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_key IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>v.rate_key, '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (VALUES
        ('Total Yards',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>'rushing_yards')::numeric,0)
                + COALESCE((p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_returner_return_yards')::numeric,0)
            ELSE
                  COALESCE((p_stats->>'passing_yards_per_game')::numeric,(p_stats->>'passing_yards')::numeric,0)
                + COALESCE((p_stats->>'rushing_yards_per_game')::numeric,(p_stats->>'rushing_yards')::numeric,0)
                + COALESCE((p_stats->>'receiving_yards_per_game')::numeric,(p_stats->>'receiving_yards')::numeric,0)
                + COALESCE((p_stats->>'kick_return_yards_per_game')::numeric,(p_stats->>'kick_return_yards')::numeric,0)
                + COALESCE((p_stats->>'punt_returner_return_yards_per_game')::numeric,(p_stats->>'punt_returner_return_yards')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Touchdowns',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'rushing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'receiving_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'kick_return_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'punt_return_touchdowns')::numeric,0)
            ELSE
                  COALESCE((p_stats->>'passing_touchdowns_per_game')::numeric,(p_stats->>'passing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'rushing_touchdowns_per_game')::numeric,(p_stats->>'rushing_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'receiving_touchdowns_per_game')::numeric,(p_stats->>'receiving_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'kick_return_touchdowns_per_game')::numeric,(p_stats->>'kick_return_touchdowns')::numeric,0)
                + COALESCE((p_stats->>'punt_return_touchdowns_per_game')::numeric,(p_stats->>'punt_return_touchdowns')::numeric,0)
            END,                                                                  TRUE, TRUE,   1, 'offense', NULL),
        ('Receiving',        NULLIF(p_stats->>'receptions','')::numeric,          TRUE, TRUE,   1, 'offense', 'receptions_per_game'),
        ('Giveaways',
            CASE WHEN p_rate_mode = 'total' THEN
                  COALESCE((p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost')::numeric,0)
            ELSE
                  COALESCE((p_stats->>'passing_interceptions_per_game')::numeric,(p_stats->>'passing_interceptions')::numeric,0)
                + COALESCE((p_stats->>'fumbles_lost_per_game')::numeric,(p_stats->>'fumbles_lost')::numeric,0)
            END,                                                                  TRUE, FALSE, -1, 'offense', NULL),
        ('Tackling',         NULLIF(p_stats->>'total_tackles','')::numeric,       TRUE, TRUE,   1, 'defense', 'total_tackles_per_game'),
        ('Tackles For Loss', NULLIF(p_stats->>'tackles_for_loss','')::numeric,    TRUE, TRUE,   1, 'defense', 'tackles_for_loss_per_game'),
        ('Sacks',            NULLIF(p_stats->>'defensive_sacks','')::numeric,     TRUE, TRUE,   1, 'defense', 'defensive_sacks_per_game'),
        ('Pass Defense',     NULLIF(p_stats->>'passes_defended','')::numeric,     TRUE, TRUE,   1, 'defense', 'passes_defended_per_game'),
        ('Interceptions',    NULLIF(p_stats->>'defensive_interceptions','')::numeric, TRUE, TRUE, 1, 'defense', 'defensive_interceptions_per_game'),
        ('Fumble Recovery',  NULLIF(p_stats->>'fumbles_recovered','')::numeric,   TRUE, TRUE,   1, 'defense', 'fumbles_recovered_per_game'),
        ('Field Goals',      NULLIF(p_stats->>'field_goals_made','')::numeric,    TRUE, TRUE,   1, 'special', 'field_goals_made_per_game'),
        ('Punting',          NULLIF(p_stats->>'punts_inside_20','')::numeric,     TRUE, TRUE,   1, 'special', 'punts_inside_20_per_game')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_key)
    WHERE p_sport = 'NFL';
$$;

-- ── 4. compute_rating — append the new mode per sport to v_modes ─────────────
CREATE OR REPLACE FUNCTION compute_rating(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated INTEGER := 0;
    v_mode    TEXT;
    v_modes   TEXT[] := ARRAY['total'] || CASE p_sport
        WHEN 'NBA'      THEN ARRAY['per_36', 'per_season']
        WHEN 'FOOTBALL' THEN ARRAY['per_90', 'per_game']
        WHEN 'NFL'      THEN ARRAY['per_game']
        ELSE ARRAY[]::TEXT[]
    END;
BEGIN
    UPDATE player_stats
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL,
           rating_composite_rank = NULL, rating_specialist_rank = NULL,
           rating_breakdown = NULL, rating_scoped_ranks = NULL, rating_modes = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL
            OR rating_composite_rank IS NOT NULL OR rating_modes IS NOT NULL);

    FOREACH v_mode IN ARRAY v_modes LOOP
        IF v_mode = 'total' THEN
            UPDATE player_stats ps SET
                rating_composite       = b.composite,
                rating_specialist      = b.specialist,
                rating_specialty       = b.specialty,
                rating_composite_rank  = b.composite_rank,
                rating_specialist_rank = b.specialist_rank,
                rating_breakdown       = b.breakdown,
                rating_scoped_ranks    = b.scoped_ranks
            FROM _compute_rating_bundle(p_sport, p_season, 'total') b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
            GET DIAGNOSTICS v_updated = ROW_COUNT;
        ELSE
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
            FROM _compute_rating_bundle(p_sport, p_season, v_mode) b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
        END IF;
    END LOOP;

    RETURN v_updated;
END;
$$;

-- ── 5. Backfill: re-fire derived triggers so the new siblings materialize on
-- existing rows, THEN recompute every (sport, season). NFL has no new sibling,
-- but is recomputed too so the parity gate proves its output is unchanged. ────
UPDATE player_stats SET stats = stats WHERE sport IN ('NBA', 'FOOTBALL');

DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats
             WHERE sport IN ('FOOTBALL', 'NBA', 'NFL') ORDER BY sport, season LOOP
        PERFORM compute_rating(r.sport, r.season);
    END LOOP;
END $$;

-- ── 6. Parity gate — default columns AND existing alternate blocks must be
-- byte-identical; the NEW mode must actually move some composites. ───────────
CREATE OR REPLACE FUNCTION _bd_sans_spec(p JSONB) RETURNS JSONB LANGUAGE sql IMMUTABLE AS $fn$
    SELECT CASE WHEN p IS NULL THEN NULL ELSE
        (SELECT jsonb_agg(e - 'is_specialty' ORDER BY ord)
         FROM jsonb_array_elements(p) WITH ORDINALITY t(e, ord)) END;
$fn$;

DO $$
DECLARE
    v_drift     BIGINT;
    v_spec_real BIGINT;
    v_mode_drift BIGINT;
    v_new_diff  BIGINT;
    v_rows      BIGINT;
BEGIN
    -- Default-mode deterministic columns: byte-identical (specialty handled below).
    SELECT count(*) INTO v_drift
    FROM _parity_before b
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
        RAISE EXCEPTION 'PARITY FAILURE: % default-mode rows drifted — aborting', v_drift;
    END IF;

    -- A specialty change is benign only if the specialist VALUE is unchanged.
    SELECT count(*) INTO v_spec_real
    FROM _parity_before b JOIN player_stats a
      ON a.player_id=b.player_id AND a.sport=b.sport AND a.season=b.season AND COALESCE(a.league_id,0)=b.league_id
    WHERE a.rating_specialty IS DISTINCT FROM b.rating_specialty
      AND a.rating_specialist IS DISTINCT FROM b.rating_specialist;
    IF v_spec_real > 0 THEN
        RAISE EXCEPTION 'PARITY FAILURE: % rows changed specialty with a different specialist value — aborting', v_spec_real;
    END IF;

    -- The EXISTING alternate blocks (NBA per_36 / FB per_90 / NFL per_game) must be
    -- byte-identical — proves the rate_key2 addition didn't perturb the prior modes.
    SELECT count(*) INTO v_mode_drift
    FROM _parity_before b JOIN player_stats a
      ON a.player_id=b.player_id AND a.sport=b.sport AND a.season=b.season AND COALESCE(a.league_id,0)=b.league_id
    WHERE b.rating_modes IS NOT NULL
      AND b.rating_modes -> (CASE b.sport WHEN 'NBA' THEN 'per_36' WHEN 'FOOTBALL' THEN 'per_90' ELSE 'per_game' END)
          IS DISTINCT FROM
          a.rating_modes -> (CASE a.sport WHEN 'NBA' THEN 'per_36' WHEN 'FOOTBALL' THEN 'per_90' ELSE 'per_game' END);
    IF v_mode_drift > 0 THEN
        RAISE EXCEPTION 'PARITY FAILURE: % rows drifted in an EXISTING alternate-mode block — aborting', v_mode_drift;
    END IF;

    -- Smoke: the NEW modes must actually differ from total for some rows (proves the
    -- new per_season / per_game siblings were read, not silently falling back to raw).
    SELECT count(*) INTO v_new_diff FROM player_stats
    WHERE (sport='NBA'      AND rating_modes ? 'per_season'
           AND (rating_modes->'per_season'->>'composite')::numeric IS DISTINCT FROM rating_composite)
       OR (sport='FOOTBALL' AND rating_modes ? 'per_game'
           AND (rating_modes->'per_game'->>'composite')::numeric IS DISTINCT FROM rating_composite);
    IF v_new_diff = 0 THEN
        RAISE EXCEPTION 'SMOKE FAILURE: new modes never differ from total — per-rate siblings not applied';
    END IF;

    SELECT count(*) INTO v_rows FROM _parity_before;
    RAISE NOTICE 'PARITY OK: default + existing alternate blocks byte-identical across % rows', v_rows;
    RAISE NOTICE 'new modes (NBA per_season / FB per_game) differ from total for % rows', v_new_diff;
END $$;

DROP FUNCTION _bd_sans_spec(JSONB);

COMMIT;

-- ============================================================================
-- 042_rating_modes.sql
-- Selectable per-X RATE MODE for the PLAYER rating engine. The frontend Per-X
-- dropdown switches Composite/Specialist/ranks/breakdown/scoped_ranks between
-- the default ("total") mode and ONE alternate rate mode per sport:
--   NBA → per_36   FOOTBALL → per_90   NFL → per_game
-- (Players only — teams have no per-rate derived keys.)
--
-- STRICTLY ADDITIVE. Default-mode output is BYTE-IDENTICAL to today: the existing
-- rating_* columns are written by the 'total' pass, and rating_datapoints' new
-- 3rd arg defaults to 'total'. The alternate mode lives ONLY in a new column
-- rating_modes JSONB, keyed by mode name. An in-transaction PARITY GATE aborts
-- the whole migration if any default-mode column drifts.
--
-- Design (DRY): the per-mode pipeline is extracted into ONE helper,
-- _compute_rating_bundle(sport, season, mode), which returns the per-entity
-- bundle computed within that mode's population. compute_rating just loops the
-- modes and ROUTES the bundle — 'total' → columns, alternates → rating_modes.
-- Single source of truth; the parity gate guarantees the 'total' route still
-- reproduces 039's output exactly.
--
-- rate_key invariant (see rating_datapoints): a per-row literal naming the
-- per-rate sibling key. NULL ⟺ mode-invariant (percentages, plus_minus,
-- inline-summed NFL rows, and sparse FB GK/penalty terms with no _per_90
-- sibling). All non-NULL siblings were verified against migrations 012 / 014.
--
-- compute_team_rating / rating_datapoints_team are UNTOUCHED (no team recompute).
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/042_rating_modes.sql
-- ============================================================================

BEGIN;

-- ── 0. Parity snapshot (BEFORE any redefinition / recompute) ────────────────
-- Captures the live default-mode columns so we can prove zero drift after the
-- recompute. ON COMMIT DROP — gone when the txn ends.
CREATE TEMP TABLE _parity_before ON COMMIT DROP AS
SELECT player_id, sport, season, COALESCE(league_id, 0) AS league_id,
       rating_composite, rating_specialist, rating_specialty,
       rating_composite_rank, rating_specialist_rank,
       rating_breakdown, rating_scoped_ranks
FROM player_stats
WHERE sport IN ('FOOTBALL', 'NBA', 'NFL');

-- ── 1. Schema ───────────────────────────────────────────────────────────────
ALTER TABLE player_stats ADD COLUMN IF NOT EXISTS rating_modes JSONB;

-- ── 2. rating_datapoints — gains p_rate_mode (DROP+CREATE: arity change) ─────
-- Each row carries the original raw_value expression (verbatim from 041) PLUS a
-- rate_key literal. The wrapping CASE selects raw_value in 'total' mode (→
-- byte-identical) or COALESCE(per-rate sibling, raw_value) otherwise.
DROP FUNCTION IF EXISTS rating_datapoints(TEXT, JSONB);

CREATE FUNCTION rating_datapoints(p_sport TEXT, p_stats JSONB, p_rate_mode TEXT DEFAULT 'total')
RETURNS TABLE (label TEXT, value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT)
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    -- NBA (per_36; turnover → tov_per_36 is the special sibling name; plus_minus
    -- has no per-36 sibling and is a margin → mode-invariant).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_key IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>v.rate_key, '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (VALUES
        ('Scoring',         NULLIF(p_stats->>'pts','')::numeric,        TRUE, TRUE,   1, 'all', 'pts_per_36'),
        ('Rebounding',      NULLIF(p_stats->>'reb','')::numeric,        TRUE, TRUE,   1, 'all', 'reb_per_36'),
        ('Playmaking',      NULLIF(p_stats->>'ast','')::numeric,        TRUE, TRUE,   1, 'all', 'ast_per_36'),
        ('Steals',          NULLIF(p_stats->>'stl','')::numeric,        TRUE, TRUE,   1, 'all', 'stl_per_36'),
        ('Rim Protection',  NULLIF(p_stats->>'blk','')::numeric,        TRUE, TRUE,   1, 'all', 'blk_per_36'),
        ('3PT Shooting',    NULLIF(p_stats->>'fg3m','')::numeric,       TRUE, TRUE,   1, 'all', 'fg3m_per_36'),
        ('On-Court Impact', NULLIF(p_stats->>'plus_minus','')::numeric, TRUE, FALSE,  1, 'all', NULL),
        ('Ball Security',   NULLIF(p_stats->>'turnover','')::numeric,   TRUE, FALSE, -1, 'all', 'tov_per_36'),
        ('Discipline',      NULLIF(p_stats->>'pf','')::numeric,         TRUE, FALSE, -1, 'all', 'pf_per_36'),
        ('Foul Drawing',    NULLIF(p_stats->>'fta','')::numeric,        FALSE, TRUE,  1, 'all', 'fta_per_36')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_key)
    WHERE p_sport = 'NBA'

    UNION ALL
    -- FOOTBALL (per_90; shots_total → shots_per_90 special name; the four sparse
    -- GK/penalty terms have no _per_90 sibling → rate_key NULL → read raw).
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_key IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>v.rate_key, '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all', 'goals_per_90'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all', 'assists_per_90'),
        ('Shooting',        NULLIF(p_stats->>'shots_total','')::numeric,      TRUE, TRUE,   1, 'all', 'shots_per_90'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate_per_90'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all', 'key_passes_per_90'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success_per_90'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        TRUE, TRUE,   1, 'all', 'duels_won_per_90'),
        ('Tackling',        NULLIF(p_stats->>'tackles','')::numeric,          TRUE, TRUE,   1, 'all', 'tackles_per_90'),
        ('Interceptions',   NULLIF(p_stats->>'interceptions','')::numeric,    TRUE, TRUE,   1, 'all', 'interceptions_per_90'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       TRUE, TRUE,   1, 'all', 'clearances_per_90'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           TRUE, TRUE,   1, 'all', 'blocks_per_90'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    TRUE, TRUE,   1, 'all', 'ball_recovery_per_90'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      TRUE, TRUE,   1, 'all', 'fouls_drawn_per_90'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all', NULL),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all', 'possession_lost_per_90'),
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all', 'saves_per_90'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all', NULL),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all', NULL),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all', NULL)
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_key)
    WHERE p_sport = 'FOOTBALL'

    UNION ALL
    -- NFL (per_game; category-balanced). The three SUM rows branch their value
    -- expression inline on p_rate_mode (summing per_game siblings, each
    -- COALESCE'd to its raw key then 0) and carry rate_key NULL so the wrapper
    -- passes them through unchanged. In 'total' mode every row returns its
    -- original 041 expression verbatim.
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

-- ── 3. _compute_rating_bundle — the per-mode pipeline, returned not written ──
-- 039's compute_rating body, parameterized by p_rate_mode and emitting the
-- per-entity bundle for the (sport, season, mode) population. Reads player_stats
-- (+ its owned position, like 039) but writes nothing.
CREATE OR REPLACE FUNCTION _compute_rating_bundle(p_sport TEXT, p_season INTEGER, p_rate_mode TEXT)
RETURNS TABLE (
    player_id INTEGER, league_id INTEGER,
    composite NUMERIC, composite_rank NUMERIC,
    specialist NUMERIC, specialist_rank NUMERIC, specialty TEXT,
    breakdown JSONB, scoped_ranks JSONB
)
LANGUAGE sql STABLE AS $$
    WITH dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet
        FROM player_stats ps
        CROSS JOIN LATERAL rating_datapoints(p_sport, ps.stats, p_rate_mode) d
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND CASE p_sport
                WHEN 'NBA'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0) >= 30
                                 AND COALESCE((ps.stats->>'minutes')::numeric, 0)      >= 20
                WHEN 'FOOTBALL' THEN COALESCE((ps.stats->>'appearances')::numeric, 0)  >= 15
                WHEN 'NFL'      THEN COALESCE((ps.stats->>'games_played')::numeric, 0)  >= 8
                ELSE FALSE
              END
    ),
    pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
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
        -- Deterministic tiebreaker (label) so the specialty is STABLE across
        -- recomputes. 039 had none → the specialty label flickered among
        -- z-score-tied in_spec datapoints (sparse players) on every re-rate. The
        -- peak zr (specialist VALUE) is identical regardless; only the label is
        -- pinned. is_specialty (in bd) derives from this, so they stay consistent.
        SELECT DISTINCT ON (player_id, league_id)
               player_id, league_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY player_id, league_id, zr DESC, label
    ),
    scored AS (
        SELECT player_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct
        FROM z
    ),
    bd AS (
        -- is_specialty is derived from sp (the SAME selection that sets
        -- rating_specialty) so the flagged breakdown row always equals the
        -- specialty. 039 used a separate non-deterministic peak that, on z-score
        -- ties (sparse players, many z=0 in_spec terms), could flag a different
        -- row than rating_specialty — a latent inconsistency this corrects.
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
                   'is_specialty', (sp.specialty IS NOT DISTINCT FROM s.label)
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
           -- NULL (not '{}') when the player has no position, matching 039 which
           -- simply doesn't touch null-position rows in its scoped pass.
           CASE WHEN sc.pos_pct IS NOT NULL
                THEN jsonb_build_object('position', sc.pos_pct) END AS scoped_ranks
    FROM base b
    JOIN ranks r USING (player_id, league_id)
    LEFT JOIN scoped sc USING (player_id, league_id);
$$;

-- ── 4. compute_rating — thin: reset, loop modes, route the bundle ───────────
CREATE OR REPLACE FUNCTION compute_rating(p_sport TEXT, p_season INTEGER)
RETURNS INTEGER
LANGUAGE plpgsql AS $$
DECLARE
    v_updated INTEGER := 0;
    v_mode    TEXT;
    v_modes   TEXT[] := ARRAY['total'] || CASE p_sport
        WHEN 'NBA'      THEN ARRAY['per_36']
        WHEN 'FOOTBALL' THEN ARRAY['per_90']
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

-- ── 5. Recompute every player (sport, season) — populates rating_modes ──────
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT sport, season FROM player_stats
             WHERE sport IN ('FOOTBALL', 'NBA', 'NFL') ORDER BY sport, season LOOP
        PERFORM compute_rating(r.sport, r.season);
    END LOOP;
END $$;

-- ── 6. Parity gate + populate smoke (aborts the migration on any drift) ─────
-- Default-mode columns must be byte-identical to the pre-migration values, with
-- ONE documented exception: rating_breakdown's is_specialty flag (compared
-- modulo is_specialty). 039 set it non-deterministically on z-score ties; 042
-- makes it consistent with rating_specialty, so it legitimately changes on tied
-- rows while value/pct/z/facet/order stay identical.
CREATE OR REPLACE FUNCTION _bd_sans_spec(p JSONB) RETURNS JSONB LANGUAGE sql IMMUTABLE AS $fn$
    SELECT CASE WHEN p IS NULL THEN NULL ELSE
        (SELECT jsonb_agg(e - 'is_specialty' ORDER BY ord)
         FROM jsonb_array_elements(p) WITH ORDINALITY t(e, ord)) END;
$fn$;

DO $$
DECLARE
    v_drift     BIGINT;
    v_spec_real BIGINT;
    v_spec_tie  BIGINT;
    v_rows      BIGINT;
    v_modes     BIGINT;
    v_diff      BIGINT;
BEGIN
    -- Strict: the deterministic columns must be byte-identical. (rating_specialty
    -- and is_specialty are tie-broken LABELS, handled separately below.)
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
        RAISE EXCEPTION 'PARITY FAILURE: % player rows drifted in default-mode columns — aborting', v_drift;
    END IF;

    -- rating_specialty / is_specialty are labels chosen among z-score ties (039
    -- resolved them non-deterministically; 042 pins them). A specialty change is
    -- benign IFF the specialist VALUE (peak zr) is unchanged — same peak, just a
    -- different tied label. A specialty change WITH a different specialist value
    -- is a real regression → abort.
    SELECT count(*) INTO v_spec_real
    FROM _parity_before b JOIN player_stats a
      ON a.player_id=b.player_id AND a.sport=b.sport AND a.season=b.season AND COALESCE(a.league_id,0)=b.league_id
    WHERE a.rating_specialty IS DISTINCT FROM b.rating_specialty
      AND a.rating_specialist IS DISTINCT FROM b.rating_specialist;
    IF v_spec_real > 0 THEN
        RAISE EXCEPTION 'PARITY FAILURE: % rows changed specialty with a different specialist value — aborting', v_spec_real;
    END IF;

    SELECT count(*) INTO v_spec_tie
    FROM _parity_before b JOIN player_stats a
      ON a.player_id=b.player_id AND a.sport=b.sport AND a.season=b.season AND COALESCE(a.league_id,0)=b.league_id
    WHERE a.rating_specialty IS DISTINCT FROM b.rating_specialty;
    RAISE NOTICE 'specialty tie-reshuffles (benign — same peak zr, now deterministic): % rows', v_spec_tie;

    SELECT count(*) INTO v_rows  FROM _parity_before;
    SELECT count(*) INTO v_modes FROM player_stats
        WHERE sport IN ('FOOTBALL','NBA','NFL') AND rating_modes IS NOT NULL;
    -- Smoke: the alternate mode must actually change SOME composites (proves the
    -- per-rate siblings were read, not silently falling back to raw everywhere).
    SELECT count(*) INTO v_diff FROM player_stats
        WHERE sport IN ('FOOTBALL','NBA','NFL') AND rating_modes IS NOT NULL
          AND (rating_modes -> (CASE sport WHEN 'NBA' THEN 'per_36' WHEN 'FOOTBALL' THEN 'per_90' ELSE 'per_game' END) ->> 'composite')::numeric
              IS DISTINCT FROM rating_composite;

    RAISE NOTICE 'PARITY OK: default-mode columns byte-identical across % rows', v_rows;
    RAISE NOTICE 'rating_modes populated for % rows; % differ from total-mode composite', v_modes, v_diff;
    IF v_modes > 0 AND v_diff = 0 THEN
        RAISE EXCEPTION 'SMOKE FAILURE: alternate mode never differs from total — per-rate siblings not applied';
    END IF;
END $$;

DROP FUNCTION _bd_sans_spec(JSONB);

COMMIT;

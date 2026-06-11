-- ============================================================================
-- 062 — Football players: Blocks + Clearances -> display (out of the composite)
--
-- Blocks and Clearances reward REACTIVE, last-ditch volume, not defender quality:
-- among PL defenders they are the LEAST-aligned-with-winning signals — perverse vs
-- the team's goals conceded (blocks +0.24, clearances +0.19) — yet they're highly
-- repeatable (0.66 / 0.83), so a high-volume blocker/clearer posts extreme z-scores
-- that dominate the composite (a deep-block CB jumping from rank 33 to 99 on one
-- season's block/clearance spike). Possession-adjustment can't fix it (the volume is
-- individual, ~5% explained by team possession), and we don't de-weight or cap. So,
-- consistent with how TEAMS already treat them (display-only since migration 060),
-- Blocks + Clearances move to the display tier for football players too: in_comp=FALSE,
-- in_spec=FALSE. Tackles + Interceptions (proactive ball-winning, neutral vs outcome)
-- stay. FOOTBALL player branch of rating_datapoints only; NBA/NFL untouched.
--
-- No API restart (rating_datapoints isn't a prepared statement); the frontend z-pizza
-- reflects the change automatically. compute_rating updates rating columns only.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/062_football_player_blocks_clearances.sql
-- ============================================================================

BEGIN;

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
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       FALSE, FALSE,   1, 'all', 'clearances'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           FALSE, FALSE,   1, 'all', 'blocks'),
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


-- Recompute every football (player) season.
DO $$
DECLARE r RECORD;
BEGIN
    FOR r IN SELECT DISTINCT season FROM player_stats WHERE sport='FOOTBALL' ORDER BY season LOOP
        PERFORM compute_rating('FOOTBALL', r.season);
    END LOOP;
END $$;

-- Gates
DO $$
DECLARE v_clr_comp BIGINT; v_blk_comp BIGINT; v_present BIGINT;
BEGIN
    SELECT count(*) INTO v_clr_comp FROM player_stats ps, jsonb_array_elements(ps.rating_breakdown) e
        WHERE ps.sport='FOOTBALL' AND e->>'label'='Clearances' AND (e->>'in_comp')::boolean;
    SELECT count(*) INTO v_blk_comp FROM player_stats ps, jsonb_array_elements(ps.rating_breakdown) e
        WHERE ps.sport='FOOTBALL' AND e->>'label'='Blocks' AND (e->>'in_comp')::boolean;
    IF v_clr_comp > 0 OR v_blk_comp > 0 THEN
        RAISE EXCEPTION '062 gate FAIL: Clearances/Blocks still in composite (clr=%, blk=%)', v_clr_comp, v_blk_comp;
    END IF;
    -- still present as display datapoints
    SELECT count(*) INTO v_present FROM player_stats ps, jsonb_array_elements(ps.rating_breakdown) e
        WHERE ps.sport='FOOTBALL' AND e->>'label' IN ('Clearances','Blocks') AND NOT (e->>'in_comp')::boolean;
    IF v_present = 0 THEN RAISE EXCEPTION '062 gate FAIL: Clearances/Blocks vanished entirely'; END IF;
    RAISE NOTICE '062 OK: Blocks + Clearances are display-only (0 in composite); % display rows', v_present;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('062_football_player_blocks_clearances')
ON CONFLICT (version) DO NOTHING;

COMMIT;

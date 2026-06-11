-- ============================================================================
-- 063 — Football: clean Goalkeeper / outfield datapoint split
--
-- Until now rating_datapoints emitted EVERY football datapoint for EVERY player,
-- regardless of position. Two consequences, both dishonest:
--   1. Keepers were rated on outfield play they never do. A GK posts near-zero
--      Goalscoring / Shooting / Dribbling / Tackling, and because those labels'
--      z-score population was dominated by outfielders, the keeper earned big
--      NEGATIVE z's on each — dragging an otherwise-fine keeper down toward the
--      middle of the pack (≈44th pct) for the crime of not scoring goals.
--   2. Outfielders carried four dead GK wedges (Shot-Stopping / Penalty Saves /
--      Punching / High Claims) sitting at z=0 — clutter in the breakdown.
--   And keeper GK stats were z-scored against a sea of outfield zeros, inflating
--      them (High Claims = +4.1) — the mirror image of the same population bug.
--
-- The fix is a clean position split, driven entirely by the datapoint definitions:
-- rating_datapoints now takes the player's position and emits ONLY the GK datapoints
-- for a Goalkeeper, ONLY the outfield datapoints for everyone else (Defender /
-- Midfielder / Attacker, and unknown/NULL → treated as outfield). Because each label
-- is now emitted by exactly one position class, its z-score population IS that cohort
-- for free — keepers are shot-stoppers measured against keepers; outfielders are
-- measured against outfielders. A keeper's composite is the sum of his keeping z's
-- only; an outfielder's the sum of his outfield z's only. Neither pizza shows the
-- other's stats. NBA / NFL branches are untouched (p_position is ignored there).
--
-- Both rating_datapoints callers pass position: _compute_rating_bundle (season
-- ratings) and compute_event_starline (per-game starline). Only FOOTBALL is
-- recomputed; NBA / NFL rating rows are not touched.
--
-- No API restart (rating_datapoints isn't a prepared statement); the frontend
-- z-pizza reflects the new breakdowns automatically.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/063_football_gk_outfield_split.sql
-- (runner: ./sql/migrate.sh)
-- ============================================================================

BEGIN;

-- ----------------------------------------------------------------------------
-- 1. rating_datapoints(p_sport, p_stats, p_rate_mode, p_position)
--    Adds p_position (DEFAULT NULL). The football branch tags each row with a
--    pos_class ('gk' / 'out') and gates on the player's position. DROP+CREATE
--    because a new parameter can't be added via CREATE OR REPLACE; the trailing
--    defaults keep the 2-/3-arg call shapes resolving.
-- ----------------------------------------------------------------------------
DROP FUNCTION IF EXISTS public.rating_datapoints(text, jsonb, text);

CREATE FUNCTION public.rating_datapoints(p_sport text, p_stats jsonb, p_rate_mode text DEFAULT 'total'::text, p_position text DEFAULT NULL)
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
    -- FOOTBALL. pos_class gates the row: 'gk' rows fire only for a Goalkeeper, 'out'
    -- rows for everyone else (outfield + unknown/NULL position). Each label is thus
    -- emitted by exactly one cohort, so its z-score population is that cohort.
    -- (shots_total keeps its legacy 'shots' sibling base; the four GK/penalty terms
    -- have no rate sibling → rate_base NULL → read raw.)
    SELECT v.label,
           CASE WHEN p_rate_mode = 'total' OR v.rate_base IS NULL THEN v.raw_value
                ELSE COALESCE(NULLIF(p_stats->>(v.rate_base || rs.suffix), '')::numeric, v.raw_value) END,
           v.in_comp, v.in_spec, v.sign, v.facet
    FROM (SELECT (SELECT rm.suffix FROM public.rate_modes rm
                  WHERE rm.sport = 'FOOTBALL' AND rm.mode = p_rate_mode) AS suffix) rs
    CROSS JOIN LATERAL (VALUES
        ('Goalscoring',     NULLIF(p_stats->>'goals','')::numeric,            TRUE, TRUE,   1, 'all', 'goals',           'out'),
        ('Creation',        NULLIF(p_stats->>'assists','')::numeric,          TRUE, TRUE,   1, 'all', 'assists',         'out'),
        ('Shooting',        NULLIF(p_stats->>'shots_total','')::numeric,      TRUE, TRUE,   1, 'all', 'shots',           'out'),
        ('Passing',         NULLIF(p_stats->>'passes_accurate','')::numeric,  TRUE, TRUE,   1, 'all', 'passes_accurate', 'out'),
        ('Key Passes',      NULLIF(p_stats->>'key_passes','')::numeric,       TRUE, TRUE,   1, 'all', 'key_passes',      'out'),
        ('Dribbling',       NULLIF(p_stats->>'dribbles_success','')::numeric, TRUE, TRUE,   1, 'all', 'dribbles_success','out'),
        ('Duels',           NULLIF(p_stats->>'duels_won','')::numeric,        TRUE, TRUE,   1, 'all', 'duels_won',       'out'),
        ('Tackling',        NULLIF(p_stats->>'tackles','')::numeric,          TRUE, TRUE,   1, 'all', 'tackles',         'out'),
        ('Interceptions',   NULLIF(p_stats->>'interceptions','')::numeric,    TRUE, TRUE,   1, 'all', 'interceptions',   'out'),
        ('Clearances',      NULLIF(p_stats->>'clearances','')::numeric,       FALSE, FALSE, 1, 'all', 'clearances',      'out'),
        ('Blocks',          NULLIF(p_stats->>'blocks','')::numeric,           FALSE, FALSE, 1, 'all', 'blocks',          'out'),
        ('Ball Recovery',   NULLIF(p_stats->>'ball_recovery','')::numeric,    TRUE, TRUE,   1, 'all', 'ball_recovery',   'out'),
        ('Drawing Fouls',   NULLIF(p_stats->>'fouls_drawn','')::numeric,      TRUE, TRUE,   1, 'all', 'fouls_drawn',     'out'),
        ('Penalties Won',   NULLIF(p_stats->>'penalties_won','')::numeric,    FALSE, TRUE,  1, 'all', NULL,              'out'),
        ('Possession Lost', NULLIF(p_stats->>'possession_lost','')::numeric,  TRUE, FALSE, -1, 'all', 'possession_lost','out'),
        ('Shot-Stopping',   NULLIF(p_stats->>'saves','')::numeric,            TRUE, TRUE,   1, 'all', 'saves',           'gk'),
        ('Penalty Saves',   NULLIF(p_stats->>'penalties_saved','')::numeric,  TRUE, TRUE,   1, 'all', NULL,              'gk'),
        ('Punching',        NULLIF(p_stats->>'punches','')::numeric,          TRUE, TRUE,   1, 'all', NULL,              'gk'),
        ('High Claims',     NULLIF(p_stats->>'good_high_claim','')::numeric,  TRUE, TRUE,   1, 'all', NULL,              'gk')
    ) v(label, raw_value, in_comp, in_spec, sign, facet, rate_base, pos_class)
    WHERE p_sport = 'FOOTBALL'
      AND (CASE WHEN p_position = 'Goalkeeper' THEN v.pos_class = 'gk'
                ELSE v.pos_class = 'out' END)

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

-- ----------------------------------------------------------------------------
-- 2. _compute_rating_bundle — pass ps.position into rating_datapoints.
--    Body is the live (058) definition; the ONLY change is the 4th argument on the
--    CROSS JOIN LATERAL call.
-- ----------------------------------------------------------------------------
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
        CROSS JOIN LATERAL rating_datapoints(p_sport, ps.stats, p_rate_mode, ps.position) d
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

-- ----------------------------------------------------------------------------
-- 3. compute_event_starline — pass e.position into rating_datapoints so the
--    per-game starline splits keepers from outfielders the same way. Body is the
--    live definition; the ONLY change is the 4th argument on the LATERAL call.
-- ----------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.compute_event_starline(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated  INTEGER := 0;
    v_balanced BOOLEAN := (p_sport = 'NFL');
BEGIN
    UPDATE event_box_scores
       SET rating_composite = NULL, rating_specialist = NULL, rating_specialty = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating_composite IS NOT NULL OR rating_specialist IS NOT NULL);

    DROP TABLE IF EXISTS _starline_dp;
    CREATE TEMP TABLE _starline_dp (
        event_id BIGINT, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    -- Every participated event × the shared datapoint definitions (position-gated).
    INSERT INTO _starline_dp
    SELECT e.id, dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM event_box_scores e
    CROSS JOIN LATERAL rating_datapoints(p_sport, e.stats, 'total', e.position) dp
    WHERE e.sport = p_sport AND e.season = p_season
      AND (e.minutes_played IS NULL OR e.minutes_played > 0);

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _starline_dp GROUP BY label
    ),
    z AS (
        SELECT d.event_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _starline_dp d JOIN pop p USING (label)
    ),
    comp_flat AS (
        SELECT event_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY event_id
    ),
    comp_facet AS (
        SELECT event_id, SUM(facet_mean) AS composite
        FROM (
            SELECT event_id, facet, AVG(sign * zr) AS facet_mean
            FROM z WHERE in_comp GROUP BY event_id, facet
        ) fm
        GROUP BY event_id
    ),
    composite AS (
        SELECT event_id, composite FROM comp_flat  WHERE NOT v_balanced
        UNION ALL
        SELECT event_id, composite FROM comp_facet WHERE     v_balanced
    ),
    spec AS (
        SELECT DISTINCT ON (event_id)
               event_id, zr AS specialist, label AS specialty
        FROM z WHERE in_spec
        ORDER BY event_id, zr DESC
    )
    UPDATE event_box_scores e SET
        rating_composite  = ROUND(c.composite,  4),
        rating_specialist = ROUND(s.specialist, 4),
        rating_specialty  = s.specialty
    FROM composite c
    JOIN spec s USING (event_id)
    WHERE e.id = c.event_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    RETURN v_updated;
END;
$function$;

-- ----------------------------------------------------------------------------
-- 4. Recompute FOOTBALL only (NBA / NFL datapoints are unchanged). compute_rating
--    rewrites the season rating columns + rating_modes; compute_event_starline
--    rewrites per-game starline. Neither touches `percentiles`, so the FCM notify
--    trigger (AFTER UPDATE OF percentiles) won't fire — but disable it for the
--    window anyway, matching house style.
-- ----------------------------------------------------------------------------
ALTER TABLE player_stats DISABLE TRIGGER trg_percentile_changed_player_stats;

DO $$
DECLARE s INTEGER;
BEGIN
    FOR s IN SELECT DISTINCT season FROM player_stats WHERE sport = 'FOOTBALL' ORDER BY 1 LOOP
        PERFORM compute_rating('FOOTBALL', s);
        PERFORM compute_event_starline('FOOTBALL', s);
    END LOOP;
END $$;

ALTER TABLE player_stats ENABLE TRIGGER trg_percentile_changed_player_stats;

-- ----------------------------------------------------------------------------
-- 5. Gate: every FOOTBALL season's breakdowns must be a clean split — a keeper's
--    breakdown holds ONLY the four GK labels; an outfielder's holds NONE of them.
-- ----------------------------------------------------------------------------
DO $$
DECLARE
    v_gk_viol  INTEGER;
    v_out_viol INTEGER;
BEGIN
    SELECT
        count(*) FILTER (WHERE ps.position = 'Goalkeeper'
                           AND el->>'label' NOT IN ('Shot-Stopping','Penalty Saves','Punching','High Claims')),
        count(*) FILTER (WHERE ps.position IS DISTINCT FROM 'Goalkeeper'
                           AND el->>'label' IN ('Shot-Stopping','Penalty Saves','Punching','High Claims'))
    INTO v_gk_viol, v_out_viol
    FROM player_stats ps
    CROSS JOIN LATERAL jsonb_array_elements(ps.rating_breakdown) el
    WHERE ps.sport = 'FOOTBALL' AND ps.rating_breakdown IS NOT NULL;

    IF v_gk_viol > 0 OR v_out_viol > 0 THEN
        RAISE EXCEPTION '063 FAIL: cross-position datapoints remain (gk_with_outfield=%, outfield_with_gk=%)',
            v_gk_viol, v_out_viol;
    END IF;
    RAISE NOTICE '063 OK: clean GK/outfield split — keepers rated on keeping only, outfielders on outfield only';
END $$;

COMMIT;

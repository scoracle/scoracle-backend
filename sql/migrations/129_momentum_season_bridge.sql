-- 129_momentum_season_bridge.sql
--
-- Momentum hardening: one season-boundary schedule for the whole product.
--
-- 1. season_bridge_window(sport) — THE canonical season-end/season-start
--    threshold window (the migration-025 cold-start schedule, ~10% of season
--    games). recalculate_event_percentiles and refresh_momentum_scores both
--    read it, so percentiles and Momentum agree on when last season stops
--    mattering.
-- 2. recalculate_event_percentiles re-emitted verbatim with its inline
--    v_window CASE swapped for season_bridge_window(p_sport). No behavior
--    change — same values, now sourced from the shared function.
-- 3. refresh_momentum_scores rating window: 21 days of CURRENT-season play,
--    bridged into the final 21 days of the PREVIOUS season while the entity
--    has played fewer than season_bridge_window(sport) current-season games.
--    A team that closed the season on a losing streak and opens with a win
--    carries that streak in its window until the bridge closes.
--    Vibe stays a plain 21 calendar days — news sentiment flows through the
--    offseason and must not pause with the fixture calendar.
-- 4. momentum_score is now SIGNED: the average of the present slopes with
--    sign preserved. Falls are as much a historic datapoint as rises; the
--    old GREATEST(x,0) clamp erased downside and made the stored history
--    dishonest. (No reader existed yet, so no read-path change.)
-- 5. Single-flight advisory lock in refresh_momentum_scores: concurrent
--    drains (NOTIFY listener vs catch-up ticker) can no longer double-append
--    a snapshot. The loser returns NULL and the Go drain keeps the dirty
--    marker for retry.
-- 6. Index swap: the three mig-128 indexes never matched the read (latest
--    row per entity for one sport). One composite index replaces them.

-- ---------------------------------------------------------------------------
-- 1. The canonical season-bridge window
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.season_bridge_window(p_sport TEXT)
RETURNS INTEGER
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE upper(p_sport)
        WHEN 'NBA'      THEN 8
        WHEN 'NFL'      THEN 2
        WHEN 'FOOTBALL' THEN 4
        ELSE 10
    END;
$$;

COMMENT ON FUNCTION public.season_bridge_window(TEXT) IS
    'THE season-end/season-start threshold window, in current-season games played '
    '(~10% of season length: NBA 8/82, NFL 2/17, FOOTBALL 4/38). Established by the '
    'migration-025 cold-start guard. Consumers: recalculate_event_percentiles blends '
    'season_composite_score with the prior-season anchor while games < window; '
    'refresh_momentum_scores includes the previous season''s final 21 days in an '
    'entity''s rating-slope window while games < window. Tune it here and every '
    'season-boundary consumer moves together — do not re-introduce inline copies.';

-- ---------------------------------------------------------------------------
-- 2. recalculate_event_percentiles — re-emitted from the live definition with
--    the inline v_window CASE replaced by season_bridge_window(p_sport).
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.recalculate_event_percentiles(p_sport text, p_season integer)
 RETURNS TABLE(player_events_updated integer, team_events_updated integer)
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_player_events INTEGER := 0;
    v_team_events INTEGER := 0;
    v_window INTEGER := public.season_bridge_window(p_sport);
BEGIN
    UPDATE event_box_scores SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;
    UPDATE event_team_stats SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;

    -- PLAYER EVENTS (Layer 1)
    WITH eligible AS (
        SELECT key_name, is_inverse, unit FROM stat_definitions
        WHERE sport = p_sport AND entity_type = 'player' AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT e.id AS event_id, COALESCE(e.position, ps.position, 'Unknown') AS position,
               ek.key_name AS stat_key, ek.is_inverse, (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_box_scores e CROSS JOIN eligible ek
        LEFT JOIN player_stats ps ON ps.player_id = e.player_id AND ps.sport = e.sport
              AND ps.season = e.season AND COALESCE(ps.league_id, 0) = COALESCE(e.league_id, 0)
        WHERE e.sport = p_sport AND e.season = p_season
          AND e.stats ? ek.key_name AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct' OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, position, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE ROUND((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id, MAX(position) AS position_group, jsonb_object_agg(stat_key, percentile) AS pct_only,
               MAX(sample_size) AS sample_size, ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked GROUP BY event_id
    ),
    normalized AS (
        SELECT event_id, position_group, pct_only, sample_size,
               ROUND((percent_rank() OVER (PARTITION BY position_group ORDER BY raw_composite ASC))::numeric * 100, 1) AS composite_score
        FROM per_event WHERE raw_composite IS NOT NULL
    )
    UPDATE event_box_scores ebs
        SET percentiles = nm.pct_only || jsonb_build_object('_position_group', nm.position_group, '_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm WHERE ebs.id = nm.event_id;
    GET DIAGNOSTICS v_player_events = ROW_COUNT;

    -- TEAM EVENTS (Layer 1)
    WITH eligible AS (
        SELECT key_name, is_inverse, unit FROM stat_definitions
        WHERE sport = p_sport AND entity_type = 'team' AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT e.id AS event_id, ek.key_name AS stat_key, ek.is_inverse, (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_team_stats e CROSS JOIN eligible ek
        WHERE e.sport = p_sport AND e.season = p_season
          AND e.stats ? ek.key_name AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct' OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE ROUND((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id, jsonb_object_agg(stat_key, percentile) AS pct_only,
               MAX(sample_size) AS sample_size, ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked GROUP BY event_id
    ),
    normalized AS (
        SELECT event_id, pct_only, sample_size,
               ROUND((percent_rank() OVER (ORDER BY raw_composite ASC))::numeric * 100, 1) AS composite_score
        FROM per_event WHERE raw_composite IS NOT NULL
    )
    UPDATE event_team_stats ets
        SET percentiles = nm.pct_only || jsonb_build_object('_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm WHERE ets.id = nm.event_id;
    GET DIAGNOSTICS v_team_events = ROW_COUNT;

    -- Layer 2: season_composite_score
    UPDATE player_stats SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL;
    UPDATE team_stats SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL;

    UPDATE player_stats ps SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ps2.player_id, ps2.league_id, ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM player_stats ps2
            CROSS JOIN LATERAL jsonb_each(ps2.percentiles) AS p(key, value)
            JOIN stat_definitions sd ON sd.sport = ps2.sport AND sd.entity_type = 'player' AND sd.key_name = p.key
            WHERE ps2.sport = p_sport AND ps2.season = p_season
              AND ps2.percentiles IS NOT NULL AND ps2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number' AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ps2.player_id, ps2.league_id HAVING COUNT(*) > 0
        ) sub
        WHERE ps.player_id = sub.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(sub.league_id, 0);

    UPDATE team_stats ts SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ts2.team_id, ts2.league_id, ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM team_stats ts2
            CROSS JOIN LATERAL jsonb_each(ts2.percentiles) AS p(key, value)
            JOIN stat_definitions sd ON sd.sport = ts2.sport AND sd.entity_type = 'team' AND sd.key_name = p.key
            WHERE ts2.sport = p_sport AND ts2.season = p_season
              AND ts2.percentiles IS NOT NULL AND ts2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number' AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ts2.team_id, ts2.league_id HAVING COUNT(*) > 0
        ) sub
        WHERE ts.team_id = sub.team_id AND ts.sport = p_sport AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(sub.league_id, 0);

    -- Layer 2.5: Cold-start guard (migration 025)
    WITH cold_start_players AS (
        SELECT ps.player_id, ps.league_id,
            (SELECT COUNT(*) FROM event_box_scores e
             WHERE e.player_id = ps.player_id AND e.sport = ps.sport AND e.season = ps.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                (SELECT prev.season_composite_score FROM player_stats prev
                 WHERE prev.player_id = ps.player_id AND prev.sport = ps.sport
                   AND prev.season = ps.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ps.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL LIMIT 1),
                (SELECT AVG(prev.season_composite_score) FROM player_stats prev
                 WHERE prev.sport = ps.sport AND prev.season = ps.season - 1
                   AND COALESCE(prev.position, 'Unknown') = COALESCE(ps.position, 'Unknown')
                   AND prev.season_composite_score IS NOT NULL),
                50.0) AS prior_anchor,
            ps.season_composite_score AS current_score
        FROM player_stats ps
        WHERE ps.sport = p_sport AND ps.season = p_season AND ps.season_composite_score IS NOT NULL
    )
    UPDATE player_stats ps SET season_composite_score = ROUND((
        (v_window - cs.games)::numeric / v_window * cs.prior_anchor
      + cs.games::numeric              / v_window * cs.current_score
    )::numeric, 1)
    FROM cold_start_players cs
    WHERE ps.player_id = cs.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = COALESCE(cs.league_id, 0) AND cs.games < v_window;

    WITH cold_start_teams AS (
        SELECT ts.team_id, ts.league_id,
            (SELECT COUNT(*) FROM event_team_stats e
             WHERE e.team_id = ts.team_id AND e.sport = ts.sport AND e.season = ts.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                (SELECT prev.season_composite_score FROM team_stats prev
                 WHERE prev.team_id = ts.team_id AND prev.sport = ts.sport
                   AND prev.season = ts.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ts.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL LIMIT 1),
                (SELECT AVG(prev.season_composite_score) FROM team_stats prev
                 WHERE prev.sport = ts.sport AND prev.season = ts.season - 1
                   AND prev.season_composite_score IS NOT NULL),
                50.0) AS prior_anchor,
            ts.season_composite_score AS current_score
        FROM team_stats ts
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.season_composite_score IS NOT NULL
    )
    UPDATE team_stats ts SET season_composite_score = ROUND((
        (v_window - cs.games)::numeric / v_window * cs.prior_anchor
      + cs.games::numeric              / v_window * cs.current_score
    )::numeric, 1)
    FROM cold_start_teams cs
    WHERE ts.team_id = cs.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = COALESCE(cs.league_id, 0) AND cs.games < v_window;

    -- Layer 3: season_composite_rank (within-position for players)
    UPDATE player_stats SET season_composite_rank = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank IS NOT NULL;
    UPDATE team_stats SET season_composite_rank = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank IS NOT NULL;

    UPDATE player_stats ps SET season_composite_rank = r.rnk
        FROM (
            SELECT player_id, league_id,
                   ROUND((percent_rank() OVER (PARTITION BY COALESCE(position, 'Unknown') ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0);

    UPDATE team_stats ts SET season_composite_rank = r.rnk
        FROM (
            SELECT team_id, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM team_stats WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(r.league_id, 0);

    -- ============================================================
    -- Layer 3 ABSOLUTE: cross-position rank for players (NEW, mig 026)
    -- No PARTITION BY position — ranks players across ALL positions in
    -- the (sport, season). Teams have no equivalent (already sport-wide).
    -- ============================================================
    UPDATE player_stats SET season_composite_rank_absolute = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank_absolute IS NOT NULL;

    UPDATE player_stats ps SET season_composite_rank_absolute = r.rnk
        FROM (
            SELECT player_id, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0);

    RETURN QUERY SELECT v_player_events, v_team_events;
END;
$function$

;

-- ---------------------------------------------------------------------------
-- 3-5. refresh_momentum_scores: season-bridged rating window, signed
--      momentum_score, single-flight advisory lock.
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.refresh_momentum_scores(p_sport TEXT DEFAULT NULL)
RETURNS INTEGER
LANGUAGE plpgsql
AS $$
DECLARE
    inserted_count INTEGER;
BEGIN
    -- Single-flight: the NOTIFY listener and the catch-up ticker can race a
    -- drain for the same sport. The loser returns NULL (NOT 0) — the Go drain
    -- leaves the dirty marker in place on NULL so the refresh is retried,
    -- never double-appended and never silently lost.
    IF NOT pg_try_advisory_xact_lock(hashtext('refresh_momentum_scores')) THEN
        RETURN NULL;
    END IF;

    WITH target_sports AS (
        SELECT id AS sport, current_season
        FROM public.sports
        WHERE p_sport IS NULL OR id = upper(p_sport)
    ),
    -- End of the previous season (last played fixture), the anchor for the
    -- season-bridge tail below. No row when the DB holds no prior season.
    prev_bounds AS (
        SELECT ts.sport, max(f.start_time) AS prev_end
        FROM target_sports ts
        JOIN public.fixtures f
          ON f.sport = ts.sport AND f.season = ts.current_season - 1
        WHERE f.start_time <= NOW()
        GROUP BY ts.sport
    ),
    -- Vibe window: a plain 21 calendar days. News sentiment flows through the
    -- offseason, so this clock never pauses with the fixture calendar.
    vibe AS (
        SELECT entity_type, entity_id, sport,
               ((array_agg(sentiment ORDER BY generated_at DESC))[1]
                - (array_agg(sentiment ORDER BY generated_at ASC))[1])::numeric AS vibe_slope,
               count(*)::int AS vibe_samples,
               min(generated_at) AS vibe_window_start,
               max(generated_at) AS vibe_window_end
        FROM public.vibe_scores
        WHERE sentiment IS NOT NULL
          AND generated_at > NOW() - INTERVAL '21 days'
          AND sport IN (SELECT sport FROM target_sports)
        GROUP BY entity_type, entity_id, sport
        HAVING count(*) >= 3
    ),
    -- Rating window: 21 days of CURRENT-season play, plus — while the entity
    -- has played fewer than season_bridge_window(sport) current-season games —
    -- the final 21 days of the previous season. Same threshold schedule as the
    -- percentile cold-start guard (migration 025), so Momentum and Rating
    -- agree on when last season stops mattering: an NFL team that closed the
    -- season on a losing streak and opens with a win carries that streak in
    -- its window until its 2nd game; NBA until the 8th; football the 4th.
    player_cur_games AS (
        SELECT e.player_id, e.sport, count(*) AS games
        FROM public.event_box_scores e
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.season = ts.current_season
          AND e.rating_composite_pct IS NOT NULL
        GROUP BY e.player_id, e.sport
    ),
    team_cur_games AS (
        SELECT e.team_id, e.sport, count(*) AS games
        FROM public.event_team_stats e
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.season = ts.current_season
          AND e.rating_composite_pct IS NOT NULL
        GROUP BY e.team_id, e.sport
    ),
    player_window_events AS (
        SELECT e.player_id AS entity_id, e.sport, e.season,
               e.rating_composite_pct, f.start_time
        FROM public.event_box_scores e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        LEFT JOIN prev_bounds pb ON pb.sport = e.sport
        WHERE e.rating_composite_pct IS NOT NULL
          AND (
                (e.season = ts.current_season
                 AND f.start_time > NOW() - INTERVAL '21 days')
             OR (e.season = ts.current_season - 1
                 AND pb.prev_end IS NOT NULL
                 AND f.start_time > pb.prev_end - INTERVAL '21 days')
          )
    ),
    team_window_events AS (
        SELECT e.team_id AS entity_id, e.sport, e.season,
               e.rating_composite_pct, f.start_time
        FROM public.event_team_stats e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        LEFT JOIN prev_bounds pb ON pb.sport = e.sport
        WHERE e.rating_composite_pct IS NOT NULL
          AND (
                (e.season = ts.current_season
                 AND f.start_time > NOW() - INTERVAL '21 days')
             OR (e.season = ts.current_season - 1
                 AND pb.prev_end IS NOT NULL
                 AND f.start_time > pb.prev_end - INTERVAL '21 days')
          )
    ),
    player_rating AS (
        SELECT 'player'::text AS entity_type, pe.entity_id, pe.sport,
               max(pe.season) AS season,
               ((array_agg(pe.rating_composite_pct ORDER BY pe.start_time DESC))[1]
                - (array_agg(pe.rating_composite_pct ORDER BY pe.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(pe.start_time) AS rating_window_start,
               max(pe.start_time) AS rating_window_end
        FROM player_window_events pe
        JOIN target_sports ts ON ts.sport = pe.sport
        LEFT JOIN player_cur_games pc
          ON pc.player_id = pe.entity_id AND pc.sport = pe.sport
        -- Prev-season rows count only while the bridge is open.
        WHERE pe.season = ts.current_season
           OR COALESCE(pc.games, 0) < public.season_bridge_window(pe.sport)
        GROUP BY pe.entity_id, pe.sport
        HAVING count(*) >= 3
    ),
    team_rating AS (
        SELECT 'team'::text AS entity_type, te.entity_id, te.sport,
               max(te.season) AS season,
               ((array_agg(te.rating_composite_pct ORDER BY te.start_time DESC))[1]
                - (array_agg(te.rating_composite_pct ORDER BY te.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(te.start_time) AS rating_window_start,
               max(te.start_time) AS rating_window_end
        FROM team_window_events te
        JOIN target_sports ts ON ts.sport = te.sport
        LEFT JOIN team_cur_games tc
          ON tc.team_id = te.entity_id AND tc.sport = te.sport
        WHERE te.season = ts.current_season
           OR COALESCE(tc.games, 0) < public.season_bridge_window(te.sport)
        GROUP BY te.entity_id, te.sport
        HAVING count(*) >= 3
    ),
    rating AS (
        SELECT * FROM player_rating
        UNION ALL
        SELECT * FROM team_rating
    ),
    entity_scores AS (
        SELECT COALESCE(v.entity_type, r.entity_type) AS entity_type,
               COALESCE(v.entity_id, r.entity_id) AS entity_id,
               COALESCE(v.sport, r.sport) AS sport,
               r.season,
               v.vibe_slope, COALESCE(v.vibe_samples, 0) AS vibe_samples,
               v.vibe_window_start, v.vibe_window_end,
               r.rating_slope, COALESCE(r.rating_samples, 0) AS rating_samples,
               r.rating_window_start, r.rating_window_end
        FROM vibe v
        FULL OUTER JOIN rating r
          ON r.entity_type = v.entity_type
         AND r.entity_id = v.entity_id
         AND r.sport = v.sport
    ),
    enriched AS (
        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               pci.league_id, pci.team_id, pci.position,
               COALESCE(pci.position_group, public.position_group(es.sport, pci.position)) AS position_group,
               t.conference, t.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        LEFT JOIN public.player_current_identity pci
          ON pci.player_id = es.entity_id AND pci.sport = es.sport AND es.entity_type = 'player'
        LEFT JOIN public.teams t
          ON t.id = pci.team_id AND t.sport = es.sport
        WHERE es.entity_type = 'player'

        UNION ALL

        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               tm.league_id, tm.id AS team_id, NULL::text AS position, NULL::text AS position_group,
               tm.conference, tm.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        JOIN public.teams tm
          ON tm.id = es.entity_id AND tm.sport = es.sport
        WHERE es.entity_type = 'team'
    )
    INSERT INTO public.momentum_scores (
        sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
        conference, division, vibe_slope, vibe_samples, vibe_window_start, vibe_window_end,
        rating_slope, rating_samples, rating_window_start, rating_window_end, momentum_score
    )
    SELECT sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
           conference, division,
           round(vibe_slope, 3), vibe_samples, vibe_window_start, vibe_window_end,
           round(rating_slope, 3), rating_samples, rating_window_start, rating_window_end,
           -- SIGNED: the average of the present slopes, sign preserved. Falls
           -- are as much a historic datapoint as rises — this number is the
           -- durable per-snapshot momentum record, so it must not clamp
           -- downside (the mig-128 GREATEST(x,0) clamp did, and is gone).
           round((
               COALESCE(vibe_slope, 0) + COALESCE(rating_slope, 0)
           ) / NULLIF(
               (CASE WHEN vibe_slope IS NULL THEN 0 ELSE 1 END)
               + (CASE WHEN rating_slope IS NULL THEN 0 ELSE 1 END),
               0
           ), 3) AS momentum_score
    FROM enriched
    WHERE vibe_slope IS NOT NULL OR rating_slope IS NOT NULL;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;

    RETURN inserted_count;
END;
$$;

-- ---------------------------------------------------------------------------
-- 6. Index swap: one index that matches the read (latest row per entity for
--    one sport). The mig-128 slope-ordered partials never served the
--    DISTINCT ON read, and the entity-first index could not be range-scanned
--    by sport. This index also carries the Go cleanup downsample self-join.
-- ---------------------------------------------------------------------------

DROP INDEX IF EXISTS public.idx_momentum_scores_entity_recent;
DROP INDEX IF EXISTS public.idx_momentum_scores_sport_vibe;
DROP INDEX IF EXISTS public.idx_momentum_scores_sport_rating;

CREATE INDEX IF NOT EXISTS idx_momentum_scores_read
    ON public.momentum_scores (sport, entity_type, entity_id, generated_at DESC);

COMMENT ON TABLE public.momentum_scores IS
    'Durable Momentum snapshots (append-only). vibe_slope: 21 calendar days. rating_slope: '
    '21 days of current-season play, bridged into the previous season''s final 21 days while '
    'the entity has played < season_bridge_window(sport) current-season games — the same '
    'threshold schedule as the percentile cold-start guard. momentum_score: SIGNED average '
    'of the present slopes (falls recorded, not clamped). Retention: full resolution 30 days, '
    'then thinned to the last snapshot per entity per day by the Go cleanup ticker.';

-- Re-snapshot every sport under the new semantics via the normal dirty-queue
-- path (mirrors the mig-128 backfill; the Go drain picks these up).
SELECT public.mark_momentum_refresh_needed(id, 'migration_129_season_bridge')
FROM public.sports
WHERE id IN ('NBA', 'NFL', 'FOOTBALL');

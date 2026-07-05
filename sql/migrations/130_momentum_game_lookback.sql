-- 130_momentum_game_lookback.sql
--
-- Simplify the Momentum rating window (supersedes the mig-129 calendar
-- bridge): a parallel game-count lookback on the SAME ~10%-of-season
-- schedule. Each entity's rating slope now reads its last
-- season_bridge_window(sport) rated games (NBA 8 / NFL 2 / FOOTBALL 4),
-- looking across (current, previous) seasons.
--
-- Why this beats the mig-129 bridge: the bridge was correct but paid two
-- prices for sharing the schedule — during the bridge the window
-- transiently spanned two 21-day tails, and NFL still needed >= 3 samples
-- from a sport that plays weekly (a bye week could starve it). A plain
-- last-N lookback keeps season_bridge_window as the one shared knob and
-- pays neither: it naturally closes at a season's end, resumes where it
-- left off at the next season's first game, and no calendar gap can
-- starve it. The rating sample floor drops to 2 — the minimum for a
-- slope, and the NFL window IS 2 games.
--
-- Vibe is untouched: 21 calendar days, >= 3 samples. News sentiment flows
-- through the offseason and has no game calendar to pause on.

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
    -- Rating lookback: the entity's last season_bridge_window(sport) rated
    -- games (~10% of the season — the shared mig-025 schedule), across
    -- (current, previous) seasons so the lookback closes at a season's end
    -- and resumes at the next season's first game. Game-count, not calendar:
    -- bye weeks and schedule gaps cannot starve it.
    player_ranked AS (
        SELECT e.player_id AS entity_id, e.sport, e.season,
               e.rating_composite_pct, f.start_time,
               row_number() OVER (
                   PARTITION BY e.player_id, e.sport
                   ORDER BY f.start_time DESC
               ) AS rn
        FROM public.event_box_scores e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.rating_composite_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
    ),
    team_ranked AS (
        SELECT e.team_id AS entity_id, e.sport, e.season,
               e.rating_composite_pct, f.start_time,
               row_number() OVER (
                   PARTITION BY e.team_id, e.sport
                   ORDER BY f.start_time DESC
               ) AS rn
        FROM public.event_team_stats e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.rating_composite_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
    ),
    player_rating AS (
        SELECT 'player'::text AS entity_type, pr.entity_id, pr.sport,
               max(pr.season) AS season,
               ((array_agg(pr.rating_composite_pct ORDER BY pr.start_time DESC))[1]
                - (array_agg(pr.rating_composite_pct ORDER BY pr.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(pr.start_time) AS rating_window_start,
               max(pr.start_time) AS rating_window_end
        FROM player_ranked pr
        WHERE pr.rn <= public.season_bridge_window(pr.sport)
        GROUP BY pr.entity_id, pr.sport
        HAVING count(*) >= 2
    ),
    team_rating AS (
        SELECT 'team'::text AS entity_type, tr.entity_id, tr.sport,
               max(tr.season) AS season,
               ((array_agg(tr.rating_composite_pct ORDER BY tr.start_time DESC))[1]
                - (array_agg(tr.rating_composite_pct ORDER BY tr.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(tr.start_time) AS rating_window_start,
               max(tr.start_time) AS rating_window_end
        FROM team_ranked tr
        WHERE tr.rn <= public.season_bridge_window(tr.sport)
        GROUP BY tr.entity_id, tr.sport
        HAVING count(*) >= 2
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
           -- downside.
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

COMMENT ON FUNCTION public.season_bridge_window(TEXT) IS
    'THE season-end/season-start threshold window, in games (~10% of season length: '
    'NBA 8/82, NFL 2/17, FOOTBALL 4/38). Established by the migration-025 cold-start '
    'guard. Consumers: recalculate_event_percentiles blends season_composite_score '
    'with the prior-season anchor while current-season games < window; '
    'refresh_momentum_scores reads each entity''s rating slope over its last <window> '
    'rated games (a season-spanning lookback, mig 130). Tune it here and every '
    'season-boundary consumer moves together — do not re-introduce inline copies.';

COMMENT ON TABLE public.momentum_scores IS
    'Durable Momentum snapshots (append-only). vibe_slope: 21 calendar days. rating_slope: '
    'the entity''s last season_bridge_window(sport) rated games (~10% of season, the same '
    'schedule as the percentile cold-start guard), a game-count lookback that spans the '
    'season boundary and cannot be starved by bye weeks. momentum_score: SIGNED average '
    'of the present slopes (falls recorded, not clamped). Retention: full resolution 30 '
    'days, then thinned to the last snapshot per entity per day by the Go cleanup ticker.';

-- Re-snapshot every sport under the new semantics via the normal dirty-queue
-- path (the Go drain picks these up; harmless if drained manually first).
SELECT public.mark_momentum_refresh_needed(id, 'migration_130_game_lookback')
FROM public.sports
WHERE id IN ('NBA', 'NFL', 'FOOTBALL');

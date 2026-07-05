-- 128_momentum_scores.sql
-- Durable Momentum leaderboard input. The profile /momentum payload still exposes
-- raw trajectory context; this table stores the latest leaderboard-grade slopes so
-- /leaderboard/momentum is a DB-first product read instead of request-time derivation.

CREATE TABLE IF NOT EXISTS public.momentum_scores (
    id BIGSERIAL PRIMARY KEY,
    sport TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id INTEGER NOT NULL,
    season INTEGER,
    league_id INTEGER,
    team_id INTEGER,
    position TEXT,
    position_group TEXT,
    conference TEXT,
    division TEXT,
    vibe_slope NUMERIC,
    vibe_samples INTEGER NOT NULL DEFAULT 0,
    vibe_window_start TIMESTAMPTZ,
    vibe_window_end TIMESTAMPTZ,
    rating_slope NUMERIC,
    rating_samples INTEGER NOT NULL DEFAULT 0,
    rating_window_start TIMESTAMPTZ,
    rating_window_end TIMESTAMPTZ,
    momentum_score NUMERIC,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT momentum_scores_entity_type_check CHECK (entity_type IN ('player', 'team'))
);

CREATE INDEX IF NOT EXISTS idx_momentum_scores_entity_recent
    ON public.momentum_scores (entity_type, entity_id, sport, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_momentum_scores_sport_vibe
    ON public.momentum_scores (sport, vibe_slope DESC, generated_at DESC)
    WHERE vibe_slope IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_momentum_scores_sport_rating
    ON public.momentum_scores (sport, rating_slope DESC, generated_at DESC)
    WHERE rating_slope IS NOT NULL;

COMMENT ON TABLE public.momentum_scores IS
    'Durable Momentum leaderboard snapshots. Stores entity-local Vibe and Rating slopes plus top-down cohort dimensions for DB-first /leaderboard/momentum reads.';

CREATE TABLE IF NOT EXISTS public.momentum_refresh_needed (
    sport TEXT PRIMARY KEY,
    reason TEXT,
    first_marked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_marked_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE public.momentum_refresh_needed IS
    'Durable dirty-sport queue for Momentum snapshots. Upstream Vibe/event-rating changes mark a sport dirty; the API listener drains only pending rows into momentum_scores.';

CREATE OR REPLACE FUNCTION public.mark_momentum_refresh_needed(p_sport TEXT, p_reason TEXT DEFAULT NULL)
RETURNS VOID
LANGUAGE plpgsql
AS $$
DECLARE
    v_sport TEXT := upper(NULLIF(p_sport, ''));
BEGIN
    IF v_sport IS NULL THEN
        RETURN;
    END IF;

    INSERT INTO public.momentum_refresh_needed (sport, reason, first_marked_at, last_marked_at)
    VALUES (v_sport, p_reason, NOW(), NOW())
    ON CONFLICT (sport) DO UPDATE SET
        reason = COALESCE(EXCLUDED.reason, public.momentum_refresh_needed.reason),
        last_marked_at = NOW();

    PERFORM pg_notify('momentum_refresh_ready', v_sport);
END;
$$;

CREATE OR REPLACE FUNCTION public.mark_momentum_refresh_from_vibe()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.sentiment IS NULL THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'INSERT' OR OLD.sentiment IS DISTINCT FROM NEW.sentiment THEN
        PERFORM public.mark_momentum_refresh_needed(NEW.sport, 'vibe');
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.mark_momentum_refresh_from_event_rating()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.rating_composite_pct IS NULL THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'INSERT' OR OLD.rating_composite_pct IS DISTINCT FROM NEW.rating_composite_pct THEN
        PERFORM public.mark_momentum_refresh_needed(NEW.sport, 'rating');
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS mark_momentum_refresh_vibe_scores ON public.vibe_scores;
CREATE TRIGGER mark_momentum_refresh_vibe_scores
AFTER INSERT OR UPDATE OF sentiment ON public.vibe_scores
FOR EACH ROW
EXECUTE FUNCTION public.mark_momentum_refresh_from_vibe();

DROP TRIGGER IF EXISTS mark_momentum_refresh_event_box_scores ON public.event_box_scores;
CREATE TRIGGER mark_momentum_refresh_event_box_scores
AFTER INSERT OR UPDATE OF rating_composite_pct ON public.event_box_scores
FOR EACH ROW
EXECUTE FUNCTION public.mark_momentum_refresh_from_event_rating();

DROP TRIGGER IF EXISTS mark_momentum_refresh_event_team_stats ON public.event_team_stats;
CREATE TRIGGER mark_momentum_refresh_event_team_stats
AFTER INSERT OR UPDATE OF rating_composite_pct ON public.event_team_stats
FOR EACH ROW
EXECUTE FUNCTION public.mark_momentum_refresh_from_event_rating();

CREATE OR REPLACE FUNCTION public.refresh_momentum_scores(p_sport TEXT DEFAULT NULL)
RETURNS INTEGER
LANGUAGE plpgsql
AS $$
DECLARE
    inserted_count INTEGER;
BEGIN
    WITH target_sports AS (
        SELECT id AS sport, current_season
        FROM public.sports
        WHERE p_sport IS NULL OR id = upper(p_sport)
    ),
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
    player_rating AS (
        SELECT 'player'::text AS entity_type, e.player_id AS entity_id, e.sport,
               max(e.season) AS season,
               ((array_agg(e.rating_composite_pct ORDER BY f.start_time DESC))[1]
                - (array_agg(e.rating_composite_pct ORDER BY f.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(f.start_time) AS rating_window_start,
               max(f.start_time) AS rating_window_end
        FROM public.event_box_scores e
        JOIN public.fixtures f ON f.id = e.fixture_id
        WHERE e.rating_composite_pct IS NOT NULL
          AND f.start_time > NOW() - INTERVAL '60 days'
          AND e.sport IN (SELECT sport FROM target_sports)
        GROUP BY e.player_id, e.sport
        HAVING count(*) >= 3
    ),
    team_rating AS (
        SELECT 'team'::text AS entity_type, e.team_id AS entity_id, e.sport,
               max(e.season) AS season,
               ((array_agg(e.rating_composite_pct ORDER BY f.start_time DESC))[1]
                - (array_agg(e.rating_composite_pct ORDER BY f.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(f.start_time) AS rating_window_start,
               max(f.start_time) AS rating_window_end
        FROM public.event_team_stats e
        JOIN public.fixtures f ON f.id = e.fixture_id
        WHERE e.rating_composite_pct IS NOT NULL
          AND f.start_time > NOW() - INTERVAL '60 days'
          AND e.sport IN (SELECT sport FROM target_sports)
        GROUP BY e.team_id, e.sport
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
           round((
               COALESCE(GREATEST(vibe_slope, 0), 0)
               + COALESCE(GREATEST(rating_slope, 0), 0)
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

SELECT public.mark_momentum_refresh_needed(id, 'migration_backfill')
FROM public.sports
WHERE id IN ('NBA', 'NFL', 'FOOTBALL');

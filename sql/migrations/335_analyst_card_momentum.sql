BEGIN;
SET LOCAL lock_timeout = '2s';

-- Older snapshots used Scout notability and remain historical observations of that measure.
ALTER TABLE public.momentum_scores ADD COLUMN scoring_version text;
-- Both the card writer and the periodic scoreboard read these same observations.
CREATE FUNCTION public.analyst_card_changes(p_sport text, p_type text, p_id integer)
RETURNS TABLE (entity_type text, entity_id integer, sport text, season integer,
    vibe_slope numeric, vibe_samples integer, vibe_window_start timestamptz, vibe_window_end timestamptz,
    rating_slope numeric, rating_samples integer, rating_window_start timestamptz, rating_window_end timestamptz,
    momentum_score numeric) LANGUAGE sql STABLE AS $$
WITH target_sports AS (
    SELECT id AS sport, current_season FROM public.sports
    WHERE p_sport IS NULL OR id = upper(p_sport)
), observations AS (
    SELECT 'scout' AS source, s.entity_type, s.entity_id, s.sport, s.reporting_date, s.generated_at, s.id,
           CASE WHEN NULLIF(btrim(s.body), '') IS NOT NULL THEN s.card_score END AS score
    FROM public.stat_summaries s
    JOIN target_sports t ON t.sport = s.sport AND t.current_season = s.season
    WHERE public.coverage_peer_visible(s.candidate_edition)
    UNION ALL
    SELECT 'influencer', s.entity_type, s.entity_id, s.sport, s.reporting_date, s.generated_at, s.id,
           CASE WHEN NULLIF(btrim(s.prompt), '') IS NOT NULL THEN s.sentiment END
    FROM public.vibe_scores s JOIN target_sports t ON t.sport = s.sport
    WHERE public.coverage_peer_visible(s.candidate_edition)
), daily AS (
    SELECT DISTINCT ON (source, entity_type, entity_id, sport, reporting_date) *
    FROM observations
    WHERE (p_type IS NULL OR entity_type = p_type) AND (p_id IS NULL OR entity_id = p_id)
      AND generated_at <= now()
      AND reporting_date BETWEEN public.character_reporting_date() - 21 AND public.character_reporting_date()
    ORDER BY source, entity_type, entity_id, sport, reporting_date, generated_at DESC, id DESC
), ranked AS (
    SELECT *, row_number() OVER (
        PARTITION BY source, entity_type, entity_id, sport ORDER BY reporting_date DESC
    ) AS recency
    FROM daily WHERE score IS NOT NULL
), changes AS (
    SELECT source, entity_type, entity_id, sport,
           max(score) FILTER (WHERE recency = 1) - max(score) FILTER (WHERE recency = 2) AS change,
           min(reporting_date)::timestamp AT TIME ZONE 'America/Detroit' AS window_start,
           max(reporting_date)::timestamp AT TIME ZONE 'America/Detroit' AS window_end
    FROM ranked WHERE recency <= 2
    GROUP BY source, entity_type, entity_id, sport HAVING count(*) = 2
)
SELECT c.entity_type, c.entity_id, c.sport, t.current_season,
       max(change) FILTER (WHERE source = 'influencer'), 2 * count(*) FILTER (WHERE source = 'influencer')::int,
       max(window_start) FILTER (WHERE source = 'influencer'), max(window_end) FILTER (WHERE source = 'influencer'),
       max(change) FILTER (WHERE source = 'scout'), 2 * count(*) FILTER (WHERE source = 'scout')::int,
       max(window_start) FILTER (WHERE source = 'scout'), max(window_end) FILTER (WHERE source = 'scout'),
       round(avg(change), 3)
FROM changes c JOIN target_sports t ON t.sport = c.sport
GROUP BY c.entity_type, c.entity_id, c.sport, t.current_season
$$;

CREATE OR REPLACE FUNCTION public.refresh_momentum_scores(p_sport text DEFAULT NULL::text) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE inserted_count integer;
BEGIN
    IF NOT pg_try_advisory_xact_lock(hashtext('refresh_momentum_scores')) THEN RETURN NULL; END IF;
    WITH entity_scores AS (
        SELECT * FROM public.analyst_card_changes(p_sport,NULL,NULL)
    ),
    enriched AS (
        SELECT es.sport, es.entity_type, es.entity_id,
               es.season,
               pci.league_id, pci.team_id, pci.position,
               COALESCE(pci.position_group, public.position_group(es.sport, pci.position)) AS position_group,
               t.conference, t.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end, es.momentum_score
        FROM entity_scores es
        LEFT JOIN public.player_current_identity pci
          ON pci.player_id = es.entity_id AND pci.sport = es.sport AND es.entity_type = 'player'
        LEFT JOIN public.teams t
          ON t.id = pci.team_id AND t.sport = es.sport
        WHERE es.entity_type = 'player'

        UNION ALL

        SELECT es.sport, es.entity_type, es.entity_id,
               es.season,
               tm.league_id, tm.id AS team_id, NULL::text AS position, NULL::text AS position_group,
               tm.conference, tm.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end, es.momentum_score
        FROM entity_scores es
        JOIN public.teams tm
          ON tm.id = es.entity_id AND tm.sport = es.sport
        WHERE es.entity_type = 'team'
    )
    INSERT INTO public.momentum_scores (
        sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
        conference, division, vibe_slope, vibe_samples, vibe_window_start, vibe_window_end,
        rating_slope, rating_samples, rating_window_start, rating_window_end, momentum_score, scoring_version
    )
    SELECT sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
           conference, division,
           round(vibe_slope, 3), vibe_samples, vibe_window_start, vibe_window_end,
           round(rating_slope, 3), rating_samples, rating_window_start, rating_window_end,
           momentum_score, 'card-momentum-v1'
    FROM enriched
    WHERE vibe_slope IS NOT NULL OR rating_slope IS NOT NULL;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;

    RETURN inserted_count;
END;
$$;

INSERT INTO public.schema_migrations(version) VALUES ('335_analyst_card_momentum') ON CONFLICT DO NOTHING;
COMMIT;

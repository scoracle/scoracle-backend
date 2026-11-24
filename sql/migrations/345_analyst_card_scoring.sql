BEGIN;
SET LOCAL lock_timeout = '2s';

-- Use the same published history that readers can see, including pre-schedule cards.
CREATE OR REPLACE FUNCTION public.analyst_card_changes(p_sport text, p_type text, p_id integer)
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
    FROM public.qualified_stat_summaries s
    JOIN target_sports t ON t.sport = s.sport AND t.current_season = s.season
    UNION ALL
    SELECT 'influencer', s.entity_type, s.entity_id, s.sport, s.reporting_date, s.generated_at, s.id,
           CASE WHEN NULLIF(btrim(s.prompt), '') IS NOT NULL THEN s.sentiment END
    FROM public.qualified_vibe_scores s JOIN target_sports t ON t.sport = s.sport
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

ALTER TABLE public.momentum_summaries ADD COLUMN estimated_score smallint
    CHECK (estimated_score BETWEEN 0 AND 100);
COMMENT ON COLUMN public.momentum_summaries.estimated_score IS
    'The Analyst’s 0–100 judgment when there is no dated momentum measurement.';

CREATE OR REPLACE VIEW public.qualified_momentum_summaries AS
SELECT id, entity_type, entity_id, sport, season, trigger_type, trigger_payload,
       direction, score, blurb, input_components, input_hash, model_version, prompt_version,
       generated_at, headline, week_season, week_no, reporting_date, candidate_edition,
       COALESCE((input_components->>'card_score')::numeric, estimated_score) AS card_score,
       CASE WHEN input_components->>'card_score' IS NOT NULL THEN 'measured'
            WHEN estimated_score IS NOT NULL THEN 'estimated' END AS card_score_source
FROM public.momentum_summaries m;

INSERT INTO public.schema_migrations(version) VALUES ('345_analyst_card_scoring') ON CONFLICT DO NOTHING;
COMMIT;

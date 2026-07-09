-- 140_latest_momentum_scores_projection.sql
--
-- DATA_FLOW_FRICTION_PRUNE_PLAN Wave 4 / D1, scoped to the measured pain:
-- /leaderboard/momentum was deriving "latest momentum row per entity" from the
-- append-only momentum_scores history on every read. FOOTBALL had ~942k sport
-- rows to sort/spill for a cold board read. Keep the append-only history, but
-- project the current row once per write statement.

CREATE MATERIALIZED VIEW IF NOT EXISTS public.latest_momentum_scores_per_entity AS
SELECT DISTINCT ON (sport, entity_type, entity_id)
    id,
    sport,
    entity_type,
    entity_id,
    season,
    league_id,
    team_id,
    position,
    position_group,
    conference,
    division,
    vibe_slope,
    vibe_samples,
    vibe_window_start,
    vibe_window_end,
    rating_slope,
    rating_samples,
    rating_window_start,
    rating_window_end,
    momentum_score,
    generated_at
FROM public.momentum_scores
ORDER BY sport, entity_type, entity_id, generated_at DESC;

CREATE UNIQUE INDEX IF NOT EXISTS idx_latest_momentum_scores_per_entity_key
    ON public.latest_momentum_scores_per_entity (sport, entity_type, entity_id);

CREATE INDEX IF NOT EXISTS idx_latest_momentum_scores_per_entity_vibe
    ON public.latest_momentum_scores_per_entity (sport, vibe_slope DESC, generated_at DESC)
    WHERE vibe_slope IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_latest_momentum_scores_per_entity_rating
    ON public.latest_momentum_scores_per_entity (sport, rating_slope DESC, generated_at DESC)
    WHERE rating_slope IS NOT NULL;

COMMENT ON MATERIALIZED VIEW public.latest_momentum_scores_per_entity IS
    'Current-row projection for momentum_scores. D1 latest-row read optimization: '
    'one row per (sport, entity_type, entity_id), refreshed by a statement trigger '
    'after momentum_scores changes so /leaderboard/momentum does not sort the full '
    'append-only history on every read.';

CREATE OR REPLACE FUNCTION public.refresh_latest_momentum_scores_per_entity()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    REFRESH MATERIALIZED VIEW public.latest_momentum_scores_per_entity;
    RETURN NULL;
END;
$$;

COMMENT ON FUNCTION public.refresh_latest_momentum_scores_per_entity() IS
    'Statement trigger helper for latest_momentum_scores_per_entity. The source '
    'history remains append-only; this refresh moves the current-row projection '
    'cost to writes/cleanup instead of every hot leaderboard read.';

DROP TRIGGER IF EXISTS refresh_latest_momentum_scores_per_entity ON public.momentum_scores;
CREATE TRIGGER refresh_latest_momentum_scores_per_entity
    AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON public.momentum_scores
    FOR EACH STATEMENT
    EXECUTE FUNCTION public.refresh_latest_momentum_scores_per_entity();

-- If this migration is re-run after a partial manual application, force the
-- projection current before recording it.
REFRESH MATERIALIZED VIEW public.latest_momentum_scores_per_entity;

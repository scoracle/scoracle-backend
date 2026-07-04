-- 121_fold_headlines_into_narratives.sql
--
-- Retire Headlines as a standalone product/stage and fold its useful signals
-- into Narratives. The news rail becomes:
--
--   candle scrub -> transfers -> narratives -> vibes
--
-- The existing headlines table is left in place as inert history. New product
-- reads and new derivation work should use news_summaries.

BEGIN;

ALTER TABLE public.news_summaries
    ADD COLUMN IF NOT EXISTS narrative_updated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS source_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS source_names TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS source_latest_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS source_oldest_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS trajectory TEXT NOT NULL DEFAULT 'developing_story'
        CHECK (trajectory IN ('developing_story', 'heating_up', 'cooling_off')),
    ADD COLUMN IF NOT EXISTS trajectory_components JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN public.news_summaries.narrative_updated_at IS
    'Narrative freshness timestamp for the News hub. Usually the newest cited source timestamp; falls back to generated_at.';
COMMENT ON COLUMN public.news_summaries.source_count IS
    'Number of cited source articles behind this narrative row.';
COMMENT ON COLUMN public.news_summaries.source_names IS
    'Distinct source names from the cited articles, preserved for provenance and client display.';
COMMENT ON COLUMN public.news_summaries.trajectory IS
    'Narrative trajectory marker: developing_story, heating_up, or cooling_off.';
COMMENT ON COLUMN public.news_summaries.trajectory_components IS
    'Transparent inputs behind the trajectory marker, usually impact delta versus the previous matching narrative.';

ALTER TABLE public.transfer_rumors
    ADD COLUMN IF NOT EXISTS rumor_updated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS source_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS source_names TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS source_latest_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS source_oldest_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS trajectory TEXT NOT NULL DEFAULT 'developing_story'
        CHECK (trajectory IN ('developing_story', 'heating_up', 'cooling_off')),
    ADD COLUMN IF NOT EXISTS trajectory_components JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN public.transfer_rumors.rumor_updated_at IS
    'Transfer-rumor freshness timestamp for the News hub. Usually the newest cited source timestamp; falls back to generated_at.';
COMMENT ON COLUMN public.transfer_rumors.trajectory IS
    'Transfer-rumor trajectory marker shared with narratives: developing_story, heating_up, or cooling_off.';

-- Backfill source metadata from existing input_news_ids where possible.
WITH src AS (
    SELECT ns.id,
           count(a.id)::int AS source_count,
           COALESCE(ARRAY(
               SELECT DISTINCT NULLIF(a2.source, '')
                 FROM public.news_articles a2
                WHERE a2.id = ANY(ns.input_news_ids)
                  AND NULLIF(a2.source, '') IS NOT NULL
                ORDER BY 1
           ), '{}'::text[]) AS source_names,
           max(COALESCE(a.published_at, a.fetched_at)) AS source_latest_at,
           min(COALESCE(a.published_at, a.fetched_at)) AS source_oldest_at
    FROM public.news_summaries ns
    LEFT JOIN LATERAL unnest(ns.input_news_ids) AS nid(article_id) ON TRUE
    LEFT JOIN public.news_articles a ON a.id = nid.article_id
    GROUP BY ns.id
)
UPDATE public.news_summaries ns
   SET source_count = COALESCE(src.source_count, 0),
       source_names = COALESCE(src.source_names, '{}'::text[]),
       source_latest_at = src.source_latest_at,
       source_oldest_at = src.source_oldest_at,
       narrative_updated_at = COALESCE(src.source_latest_at, ns.generated_at)
  FROM src
 WHERE ns.id = src.id
   AND (ns.narrative_updated_at IS NULL OR ns.source_count = 0);

WITH src AS (
    SELECT tr.id,
           count(a.id)::int AS source_count,
           COALESCE(ARRAY(
               SELECT DISTINCT NULLIF(a2.source, '')
                 FROM public.news_articles a2
                WHERE a2.id = ANY(tr.input_news_ids)
                  AND NULLIF(a2.source, '') IS NOT NULL
                ORDER BY 1
           ), '{}'::text[]) AS source_names,
           max(COALESCE(a.published_at, a.fetched_at)) AS source_latest_at,
           min(COALESCE(a.published_at, a.fetched_at)) AS source_oldest_at
    FROM public.transfer_rumors tr
    LEFT JOIN LATERAL unnest(tr.input_news_ids) AS nid(article_id) ON TRUE
    LEFT JOIN public.news_articles a ON a.id = nid.article_id
    GROUP BY tr.id
)
UPDATE public.transfer_rumors tr
   SET source_count = COALESCE(src.source_count, 0),
       source_names = COALESCE(src.source_names, '{}'::text[]),
       source_latest_at = src.source_latest_at,
       source_oldest_at = src.source_oldest_at,
       rumor_updated_at = COALESCE(src.source_latest_at, tr.generated_at)
  FROM src
 WHERE tr.id = src.id
   AND (tr.rumor_updated_at IS NULL OR tr.source_count = 0);

CREATE INDEX IF NOT EXISTS idx_news_summaries_scope
    ON public.news_summaries (sport, entity_type, entity_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_news_summaries_updated
    ON public.news_summaries (sport, narrative_updated_at DESC)
    WHERE body IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_transfer_rumors_scope
    ON public.transfer_rumors (sport, team_id, player_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_transfer_rumors_updated
    ON public.transfer_rumors (sport, rumor_updated_at DESC)
    WHERE is_rumor IS TRUE;

-- Restore the vetted-link enqueue trigger after the temporary headlines stage.
-- Teams get transfer analysis first; narratives then read those vetted heat rows
-- as context; vibe reads the resulting narratives.
CREATE OR REPLACE FUNCTION public.enqueue_derive_on_vetted() RETURNS TRIGGER AS $$
DECLARE
    v_count  integer;
    v_epoch  bigint;
    v_ver    text;
    v_stages text[] := ARRAY['narratives', 'vibe'];
BEGIN
    SELECT count(*),
           COALESCE(EXTRACT(EPOCH FROM max(nae.scrubbed_at))::bigint, 0)
      INTO v_count, v_epoch
      FROM public.news_article_entities nae
      JOIN public.news_articles a ON a.id = nae.article_id
     WHERE nae.entity_type = NEW.entity_type
       AND nae.entity_id   = NEW.entity_id
       AND nae.sport       = NEW.sport
       AND nae.vetted IS TRUE
       AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours');

    IF v_count = 0 THEN
        RETURN NEW;
    END IF;

    v_ver := v_count || ':' || v_epoch;

    IF NEW.entity_type = 'team' THEN
        v_stages := ARRAY['transfers', 'narratives', 'vibe'];
    END IF;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT st, NEW.entity_type, NEW.entity_id, NEW.sport, 'pending', v_ver, NOW(), NOW()
      FROM unnest(v_stages) AS st
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    PERFORM pg_notify('pipeline_work_ready', '');

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Headline rows are no longer claimable by the daemon. Clear outstanding work
-- so retired stage rows do not sit in pipeline_work_status forever.
DELETE FROM public.pipeline_work
 WHERE stage = 'headlines';

COMMIT;

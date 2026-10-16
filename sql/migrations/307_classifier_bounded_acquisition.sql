BEGIN;
CREATE TABLE IF NOT EXISTS public.source_url_resolutions (
    wrapper_url text PRIMARY KEY,
    publisher_url text NOT NULL,
    resolved_at timestamptz NOT NULL DEFAULT now()
);

-- Rank and query-identity changes must not make an unavailable publisher worth fetching again.
CREATE OR REPLACE FUNCTION public.classifier_acquisition_key(article bigint)
RETURNS text LANGUAGE sql STABLE AS $$
    SELECT md5(jsonb_build_array(url,title,COALESCE(full_text,''),extract(epoch FROM published_at))::text)
    FROM public.news_articles WHERE id=article
$$;

CREATE TABLE IF NOT EXISTS public.classifier_acquisitions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL,
    acquisition_key text NOT NULL,
    discovery_version text NOT NULL,
    policy_version text NOT NULL,
    status text NOT NULL CHECK (status IN ('acquired','skipped','retryable_error')),
    reason text NOT NULL,
    receipt jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS classifier_acquisition_outcome
    ON public.classifier_acquisitions(article_id,sport,acquisition_key,policy_version,status);
COMMENT ON TABLE public.classifier_acquisitions IS
    'Claim-fenced acquisition outcomes; skipped is unknown evidence, never a negative Classifier score.';

CREATE OR REPLACE FUNCTION public.classifier_enqueue_acquisition(article bigint, league text)
RETURNS boolean LANGUAGE sql VOLATILE AS $$
    WITH revision AS (SELECT public.classifier_discovery_version(article,league) AS version),
    queued AS (
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT 'classifier_acquire','article',article,league,version FROM revision
        WHERE version IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM public.classifier_sources s
            WHERE s.article_id=article AND s.sport=league AND s.discovery_version=version)
        AND NOT EXISTS (
            SELECT 1 FROM public.classifier_acquisitions a
            WHERE a.article_id=article AND a.sport=league
            AND a.acquisition_key=public.classifier_acquisition_key(article)
            AND a.policy_version='article-acquisition-v1' AND a.status='skipped')
        ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
            status='pending',attempts=0,input_version=EXCLUDED.input_version,
            available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
            updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
        WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
        RETURNING 1
    ) SELECT EXISTS(SELECT 1 FROM queued)
$$;

CREATE OR REPLACE FUNCTION public.classifier_replay_acquisition(batch_limit integer DEFAULT 1000)
RETURNS bigint LANGUAGE sql VOLATILE AS $$
    WITH candidates AS MATERIALIZED (
        SELECT DISTINCT p.article_id,p.sport FROM public.harvester_query_provenance p
        JOIN public.news_articles a ON a.id=p.article_id
        WHERE a.published_at IS NULL OR a.published_at >= now()-interval '48 hours'
        OR EXISTS (SELECT 1 FROM public.classifier_source_heads h WHERE h.article_id=p.article_id AND h.sport=p.sport)
    ), missing AS MATERIALIZED (
        SELECT c.*,public.classifier_discovery_version(c.article_id,c.sport) AS version
        FROM candidates c WHERE NOT EXISTS (
            SELECT 1 FROM public.classifier_acquisitions a
            WHERE a.article_id=c.article_id AND a.sport=c.sport
            AND a.acquisition_key=public.classifier_acquisition_key(c.article_id)
            AND a.policy_version='article-acquisition-v1' AND a.status='skipped')
    ), batch AS MATERIALIZED (
        SELECT m.article_id,m.sport FROM missing m JOIN public.news_articles a ON a.id=m.article_id
        WHERE m.version IS NOT NULL
        AND NOT EXISTS (SELECT 1 FROM public.classifier_sources s
            WHERE s.article_id=m.article_id AND s.sport=m.sport AND s.discovery_version=m.version)
        AND NOT EXISTS (SELECT 1 FROM public.pipeline_work w
            WHERE w.stage='classifier_acquire' AND w.entity_type='article'
            AND w.entity_id=m.article_id AND w.sport=m.sport AND w.input_version=m.version)
        ORDER BY a.published_at DESC NULLS LAST,a.feed_rank ASC NULLS LAST,m.article_id,m.sport
        LIMIT GREATEST(batch_limit,0)
    ) SELECT count(*) FILTER (WHERE public.classifier_enqueue_acquisition(article_id,sport)) FROM batch
$$;
INSERT INTO public.schema_migrations(version) VALUES ('307_classifier_bounded_acquisition') ON CONFLICT DO NOTHING;
COMMIT;

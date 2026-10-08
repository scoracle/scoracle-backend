BEGIN;
ALTER TABLE public.classifier_sources ADD COLUMN IF NOT EXISTS discovery_version text;
CREATE INDEX IF NOT EXISTS classifier_source_discovery
    ON public.classifier_sources(article_id,sport,discovery_version);

-- Queue revision only; SHA-256 body/request identities remain in native receipts.
-- Epoch publication time keeps the revision independent of connection time zones.
CREATE OR REPLACE FUNCTION public.classifier_discovery_version(article bigint, league text)
RETURNS text LANGUAGE sql STABLE AS $$
    SELECT md5(jsonb_build_array(a.url,a.title,COALESCE(a.source,''),
        extract(epoch FROM a.published_at),COALESCE(a.full_text,''),a.duplicate_of,
        jsonb_agg(jsonb_build_array(p.entity_type,p.entity_id,p.sport,p.feed_rank,t.name)
            ORDER BY p.entity_type,p.entity_id))::text)
    FROM public.news_articles a
    JOIN public.harvester_query_provenance p ON p.article_id=a.id AND p.sport=league
    LEFT JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport
    WHERE a.id=article
    GROUP BY a.id
$$;

-- The RSS producer and backlog reconciler share the same revision and conflict policy.
-- Unchanged failures keep their retry backoff/dead-letter disposition.
CREATE OR REPLACE FUNCTION public.classifier_enqueue_acquisition(article bigint, league text)
RETURNS boolean LANGUAGE sql VOLATILE AS $$
    WITH revision AS (SELECT public.classifier_discovery_version(article,league) AS version),
    queued AS (
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT 'classifier_acquire','article',article,league,version FROM revision
        WHERE version IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM public.classifier_sources s
            WHERE s.article_id=article AND s.sport=league AND s.discovery_version=version)
        ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
            status='pending',attempts=0,input_version=EXCLUDED.input_version,
            available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
            updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
        WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
        RETURNING 1
    ) SELECT EXISTS(SELECT 1 FROM queued)
$$;

-- ponytail: periodic corpus scan; use an article-change outbox if scan cost becomes measurable.
CREATE OR REPLACE FUNCTION public.classifier_replay_acquisition(batch_limit integer DEFAULT 1000)
RETURNS bigint LANGUAGE sql VOLATILE AS $$
    WITH candidates AS MATERIALIZED (
        SELECT DISTINCT article_id,sport FROM public.harvester_query_provenance
    ), missing AS MATERIALIZED (
        SELECT c.*,public.classifier_discovery_version(c.article_id,c.sport) AS version
        FROM candidates c
    ), batch AS MATERIALIZED (
        SELECT m.article_id,m.sport FROM missing m WHERE m.version IS NOT NULL
        AND NOT EXISTS (SELECT 1 FROM public.classifier_sources s
            WHERE s.article_id=m.article_id AND s.sport=m.sport AND s.discovery_version=m.version)
        AND NOT EXISTS (SELECT 1 FROM public.pipeline_work w
            WHERE w.stage='classifier_acquire' AND w.entity_type='article'
            AND w.entity_id=m.article_id AND w.sport=m.sport AND w.input_version=m.version)
        ORDER BY m.article_id,m.sport LIMIT GREATEST(batch_limit,0)
    ) SELECT count(*) FILTER (WHERE public.classifier_enqueue_acquisition(article_id,sport)) FROM batch
$$;
COMMENT ON FUNCTION public.classifier_replay_acquisition(integer) IS
    'Idempotent retained RSS replay; never resets unchanged active leases or failed attempts.';
INSERT INTO public.schema_migrations(version) VALUES ('292_classifier_acquisition_intake') ON CONFLICT DO NOTHING;
COMMIT;

BEGIN;
-- Headline revisions never depend on publisher body extraction or its cache.
CREATE OR REPLACE FUNCTION public.classifier_discovery_version(article bigint, league text)
RETURNS text LANGUAGE sql STABLE AS $$
    WITH candidates AS (
        SELECT entity_type,entity_id,sport,feed_rank,true AS rss FROM public.harvester_query_provenance WHERE article_id=article AND sport=league
        UNION ALL SELECT entity_type,entity_id,sport,NULL::integer,false FROM public.news_article_entities WHERE article_id=article AND sport=league
    ), names AS (SELECT 'team' AS entity_type,id,name FROM public.teams WHERE sport=league
        UNION ALL SELECT 'player',id,name FROM public.players WHERE sport=league),
    subjects AS (
        SELECT c.entity_type,c.entity_id,min(c.feed_rank) AS feed_rank,n.name,bool_or(c.rss) AS rss
        FROM candidates c JOIN names n ON n.entity_type=c.entity_type AND n.id=c.entity_id
        GROUP BY c.entity_type,c.entity_id,n.name
    )
    SELECT md5(jsonb_build_array('classification-criteria-v9-headlines',a.url,a.title,COALESCE(a.source,''),
        extract(epoch FROM a.published_at),a.duplicate_of,
        public.classifier_identity_candidates(league,a.title,''),
        jsonb_agg(jsonb_build_array(p.entity_type,p.entity_id,p.feed_rank,p.name,p.rss)
            ORDER BY p.entity_type,p.entity_id))::text)
    FROM public.news_articles a CROSS JOIN subjects p WHERE a.id=article GROUP BY a.id
$$;

CREATE OR REPLACE FUNCTION public.classifier_article_ready() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE measured record;
BEGIN
    IF NEW.status<>'pending' THEN RETURN NEW; END IF;
    SELECT x.criteria_version,x.gate,x.clean_text,s.source INTO STRICT measured
        FROM public.classifier_spectra x JOIN public.classifier_sources s ON s.id=x.source_id
        WHERE x.id=NEW.spectrum_id;
    IF measured.criteria_version='classification-criteria-v9-headlines' THEN
        IF NOT COALESCE((measured.gate->'characters'->>NEW.character)::boolean,false)
            OR measured.source->>'evidence_scope' IS DISTINCT FROM 'headline'
            OR length(btrim(measured.clean_text))=0
            OR measured.clean_text IS DISTINCT FROM measured.source->>'title' THEN
            RAISE EXCEPTION 'Headline delivery requires an admitted character and retained headline';
        END IF;
    ELSIF measured.criteria_version IN ('classification-criteria-v7-headline','classification-criteria-v8-admission') AND
        (length(btrim(COALESCE(measured.source->>'body','')))=0
         OR NOT COALESCE((measured.gate->'characters'->>NEW.character)::boolean,false)) THEN
        RAISE EXCEPTION 'Historical delivery requires an admitted character and retained article';
    END IF;
    RETURN NEW;
END;
$$;

-- Fresh discoveries and retained historical links are candidates, never relevance labels.
-- The newest unscored headlines run first. Older replay work creates archive rows only.
CREATE OR REPLACE FUNCTION public.classifier_replay_admission(batch_limit integer DEFAULT 1000)
RETURNS bigint LANGUAGE sql VOLATILE AS $$
    WITH candidates AS MATERIALIZED (
        SELECT article_id,sport FROM public.harvester_query_provenance WHERE entity_type IN ('team','player')
        UNION SELECT article_id,sport FROM public.news_article_entities WHERE entity_type IN ('team','player')
    ), batch AS MATERIALIZED (
        SELECT c.*,a.published_at FROM candidates c JOIN public.news_articles a ON a.id=c.article_id
        WHERE length(btrim(a.title))>0
            AND NOT EXISTS(SELECT 1 FROM public.classifier_sources s
                WHERE s.article_id=c.article_id AND s.sport=c.sport AND s.discovery_version IS NOT NULL
                AND s.source->>'admission_criteria_version'='classification-criteria-v9-headlines')
            AND NOT EXISTS(SELECT 1 FROM public.pipeline_work w WHERE w.stage='classifier'
                AND w.entity_type='article' AND w.entity_id=c.article_id AND w.sport=c.sport)
        ORDER BY a.published_at DESC NULLS LAST,a.feed_rank ASC NULLS LAST,c.article_id,c.sport
        LIMIT GREATEST(batch_limit-(SELECT count(*) FROM public.pipeline_work
            WHERE stage='classifier' AND entity_type='article'
                AND (status IN ('pending','running') OR (status='failed' AND attempts<5))),0)
    ), revisions AS MATERIALIZED (
        SELECT b.*,public.classifier_discovery_version(article_id,sport) AS version FROM batch b
    ), queued AS (
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT 'classifier','article',article_id,sport,
            CASE WHEN published_at IS NULL OR published_at<now()-interval '48 hours'
                THEN 'headline-history:' ELSE '' END||version FROM revisions WHERE version IS NOT NULL
        ON CONFLICT(stage,entity_type,entity_id,sport) DO NOTHING RETURNING 1
    ) SELECT count(*) FROM queued
$$;

CREATE INDEX classifier_spectra_headline_archive ON public.classifier_spectra
    (sport,(target->>'entity_type'),(target->>'entity_id'),created_at DESC)
    WHERE criteria_version='classification-criteria-v9-headlines';

-- Cut over between editions: retain every receipt and product, reconcile fresh RSS on restart.
DO $$ BEGIN
    IF EXISTS(SELECT 1 FROM public.coverage_schedule WHERE enabled AND phase<>'coverage') THEN
        RAISE EXCEPTION 'Apply the headline cutover during coverage with workers stopped';
    END IF;
END $$;
UPDATE public.classifier_spectrum_deliveries SET status='superseded',reason='headline_corpus_cutover',updated_at=now()
    WHERE status='pending';
DELETE FROM public.pipeline_work WHERE stage='classifier_acquire';
WITH revisions AS MATERIALIZED (
    SELECT w.entity_id,w.sport,a.published_at,public.classifier_discovery_version(w.entity_id,w.sport) AS version
    FROM public.pipeline_work w JOIN public.news_articles a ON a.id=w.entity_id
    WHERE w.stage='classifier' AND w.entity_type='article'
)
UPDATE public.pipeline_work w SET status='pending',attempts=0,available_at=now(),updated_at=now(),
    input_version=CASE WHEN r.published_at IS NULL OR r.published_at<now()-interval '48 hours'
        THEN 'headline-history:' ELSE '' END||r.version,
    running_input_version=NULL,claim_token=NULL,last_error=NULL
FROM revisions r WHERE w.stage='classifier' AND w.entity_type='article'
    AND w.entity_id=r.entity_id AND w.sport=r.sport AND r.version IS NOT NULL;
COMMENT ON TABLE public.classifier_sources IS
    'Immutable RSS headline metadata. v9 has empty body and evidence_scope=headline; historical publisher text remains archived.';
COMMENT ON TABLE public.classifier_spectra IS
    'Immutable source-bound spectra. v9 scores headline relevance, emotion, movement, availability and clickbait; subject-scoped character lenses dispatch daily reporting obligations.';
INSERT INTO public.schema_migrations(version) VALUES ('314_classifier_headline_corpus') ON CONFLICT DO NOTHING;
COMMIT;

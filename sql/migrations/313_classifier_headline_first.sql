BEGIN;
-- Apply with 314: old source versions stay archived; the headline cutover retires their pending deliveries.
-- Discovery revisions depend on RSS evidence and explicit candidates, never a reader body scan.
CREATE OR REPLACE FUNCTION public.classifier_discovery_version(article bigint, league text)
RETURNS text LANGUAGE sql STABLE AS $$
    SELECT md5(jsonb_build_array('classification-criteria-v8-admission',a.url,a.title,COALESCE(a.source,''),
        extract(epoch FROM a.published_at),COALESCE(a.full_text,''),a.duplicate_of,
        jsonb_agg(jsonb_build_array(p.entity_type,p.entity_id,p.sport,p.feed_rank,t.name)
            ORDER BY p.entity_type,p.entity_id))::text)
    FROM public.news_articles a
    JOIN public.harvester_query_provenance p ON p.article_id=a.id AND p.sport=league
    LEFT JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport
    WHERE a.id=article GROUP BY a.id
$$;

ALTER TABLE public.classifier_spectra DROP CONSTRAINT classifier_spectra_complete_receipt;
ALTER TABLE public.classifier_spectra ADD CONSTRAINT classifier_spectra_complete_receipt
    CHECK (COALESCE(gate->>'coverage'='complete'
        AND COALESCE(receipt#>>'{request,state,article,headline}',receipt#>>'{request,state,article,body}',receipt#>>'{request,state,body}')=clean_text
        AND receipt#>>'{response,model}'=model AND request_hash<>'',false)) NOT VALID;
ALTER TABLE public.classifier_acquisitions DROP CONSTRAINT classifier_acquisitions_status_check;
ALTER TABLE public.classifier_acquisitions ADD CONSTRAINT classifier_acquisitions_status_check
    CHECK(status IN ('acquired','skipped','retryable_error','headline_rejected'));

-- Retire news obligations without discarding history or replacing any statistics work.
UPDATE public.classifier_spectrum_deliveries SET status='held',reason='scout_stats_only',updated_at=now()
    WHERE character='scout' AND status='pending';
ALTER TABLE public.classifier_spectrum_deliveries ADD CONSTRAINT classifier_no_scout_pending
    CHECK(character<>'scout' OR status<>'pending');

CREATE FUNCTION public.classifier_article_ready() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE measured record;
BEGIN
    IF NEW.status<>'pending' THEN RETURN NEW; END IF;
    SELECT x.criteria_version,x.gate,s.source INTO STRICT measured
        FROM public.classifier_spectra x JOIN public.classifier_sources s ON s.id=x.source_id
        WHERE x.id=NEW.spectrum_id;
    IF measured.criteria_version IN ('classification-criteria-v7-headline','classification-criteria-v8-admission') AND
        (length(btrim(COALESCE(measured.source->>'body','')))=0
         OR NOT COALESCE((measured.gate->'characters'->>NEW.character)::boolean,false)) THEN
        RAISE EXCEPTION 'Headline delivery requires an admitted character and retained article';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER classifier_article_ready BEFORE INSERT OR UPDATE OF status,spectrum_id,character ON public.classifier_spectrum_deliveries
    FOR EACH ROW EXECUTE FUNCTION public.classifier_article_ready();
COMMENT ON TABLE public.classifier_spectra IS
    'Immutable decisions. v8 is relevance-only admission with no character obligations. v7 clean_text is RSS headline evidence; the linked classifier_sources body is shared character evidence. Historical body decisions remain retained.';
COMMENT ON TABLE public.classifier_acquisitions IS
    'Claim-fenced intake outcomes. headline_rejected is a model verdict with no publisher read; failed reading is unknown article evidence, never negative relevance.';
CREATE OR REPLACE FUNCTION public.classifier_enqueue_admission(article bigint, league text)
RETURNS boolean LANGUAGE sql VOLATILE AS $$
    WITH revision AS (SELECT public.classifier_discovery_version(article,league) AS version),
    queued AS (
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT 'classifier','article',article,league,version FROM revision
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

CREATE OR REPLACE FUNCTION public.classifier_replay_admission(batch_limit integer DEFAULT 1000)
RETURNS bigint LANGUAGE sql VOLATILE AS $$
    WITH candidates AS MATERIALIZED (
        SELECT DISTINCT p.article_id,p.sport FROM public.harvester_query_provenance p
        JOIN public.news_articles a ON a.id=p.article_id
        WHERE a.published_at IS NULL OR a.published_at >= now()-interval '48 hours'
        OR EXISTS (SELECT 1 FROM public.classifier_source_heads h WHERE h.article_id=p.article_id AND h.sport=p.sport)
    ), missing AS MATERIALIZED (
        SELECT c.*,public.classifier_discovery_version(c.article_id,c.sport) AS version
        FROM candidates c
    ), batch AS MATERIALIZED (
        SELECT m.article_id,m.sport FROM missing m JOIN public.news_articles a ON a.id=m.article_id
        WHERE m.version IS NOT NULL
        AND NOT EXISTS (SELECT 1 FROM public.classifier_sources s
            WHERE s.article_id=m.article_id AND s.sport=m.sport AND s.discovery_version=m.version)
        AND NOT EXISTS (SELECT 1 FROM public.pipeline_work w
            WHERE w.stage='classifier' AND w.entity_type='article'
            AND w.entity_id=m.article_id AND w.sport=m.sport AND w.input_version=m.version)
        ORDER BY a.published_at DESC NULLS LAST,a.feed_rank ASC NULLS LAST,m.article_id,m.sport
        LIMIT GREATEST(batch_limit,0)
    ) SELECT count(*) FILTER (WHERE public.classifier_enqueue_admission(article_id,sport)) FROM batch
$$;
-- Older discovery clients enter the admission queue through the same compatibility functions.
CREATE OR REPLACE FUNCTION public.classifier_enqueue_acquisition(article bigint,league text)
RETURNS boolean LANGUAGE sql VOLATILE AS $$ SELECT public.classifier_enqueue_admission(article,league) $$;
CREATE OR REPLACE FUNCTION public.classifier_replay_acquisition(batch_limit integer DEFAULT 1000)
RETURNS bigint LANGUAGE sql VOLATILE AS $$ SELECT public.classifier_replay_admission(batch_limit) $$;
COMMENT ON TABLE public.classifier_sources IS
    'Immutable RSS admission snapshots and separately acquired publisher bodies; acquired versions link admission_source_id. No body is character evidence until downstream selection is implemented.';
INSERT INTO public.schema_migrations(version) VALUES ('313_classifier_headline_first') ON CONFLICT DO NOTHING;
COMMIT;

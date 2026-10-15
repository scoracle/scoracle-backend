BEGIN;
-- The current source is an explicit pointer, independent of an RSS discovery hash.
CREATE TABLE IF NOT EXISTS public.classifier_source_heads (
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL,
    source_id bigint NOT NULL REFERENCES public.classifier_sources(id),
    PRIMARY KEY(article_id,sport)
);
INSERT INTO public.classifier_source_heads(article_id,sport,source_id)
SELECT DISTINCT ON (article_id,sport) article_id,sport,id FROM public.classifier_sources
ORDER BY article_id,sport,id DESC ON CONFLICT DO NOTHING;

CREATE OR REPLACE FUNCTION public.classifier_discovery_version(article bigint, league text)
RETURNS text LANGUAGE sql STABLE AS $$
    SELECT md5(jsonb_build_array(a.url,a.title,COALESCE(a.source,''),
        extract(epoch FROM a.published_at),COALESCE(a.full_text,''),a.duplicate_of,
        public.classifier_identity_candidates(league,a.title,COALESCE(NULLIF(a.full_text,''),
            (SELECT s.source->>'body' FROM public.classifier_source_heads h JOIN public.classifier_sources s ON s.id=h.source_id
             WHERE h.article_id=a.id AND h.sport=league AND s.source->>'url'=a.url),'')),
        jsonb_agg(jsonb_build_array(p.entity_type,p.entity_id,p.sport,p.feed_rank,t.name)
            ORDER BY p.entity_type,p.entity_id))::text)
    FROM public.news_articles a
    JOIN public.harvester_query_provenance p ON p.article_id=a.id AND p.sport=league
    LEFT JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport
    WHERE a.id=article
    GROUP BY a.id
$$;

ALTER TABLE public.classifier_spectra ADD COLUMN IF NOT EXISTS request_hash text NOT NULL DEFAULT '';
ALTER TABLE public.classifier_spectra ADD COLUMN IF NOT EXISTS receipt jsonb NOT NULL DEFAULT '{}'::jsonb;

-- A new source, model fingerprint, question packet or scrubber retains its own spectrum.
DO $$
DECLARE old_key text;
BEGIN
    FOR old_key IN SELECT conname FROM pg_constraint
        WHERE conrelid='public.classifier_spectra'::regclass AND contype='u'
        AND pg_get_constraintdef(oid) IN ('UNIQUE (article_id, sport, target, model, criteria_version)',
            'UNIQUE (source_id, target, model, criteria_version, scrub_version)')
    LOOP
        EXECUTE format('ALTER TABLE public.classifier_spectra DROP CONSTRAINT %I', old_key);
    END LOOP;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint
        WHERE conrelid='public.classifier_spectra'::regclass AND conname='classifier_spectra_revision_key') THEN
        ALTER TABLE public.classifier_spectra ADD CONSTRAINT classifier_spectra_revision_key
            UNIQUE (source_id, target, model, criteria_version, scrub_version, request_hash);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint
        WHERE conrelid='public.classifier_spectra'::regclass AND conname='classifier_spectra_complete_receipt') THEN
        ALTER TABLE public.classifier_spectra ADD CONSTRAINT classifier_spectra_complete_receipt
            CHECK (COALESCE(gate->>'coverage'='complete'
                AND receipt#>>'{request,state,body}'=clean_text
                AND receipt#>>'{response,model}'=model
                AND request_hash<>'',false)) NOT VALID;
    END IF;
END $$;

-- Preserve failed measurements for diagnosis, but never dispatch them as complete evidence.
UPDATE public.classifier_spectrum_deliveries d SET status='superseded',updated_at=now()
FROM public.classifier_spectra x WHERE x.id=d.spectrum_id AND d.status='pending'
AND (COALESCE(x.gate->>'coverage','') <> 'complete' OR x.clean_text ~* '^\s*<(\!doctype|html)');

INSERT INTO public.schema_migrations(version) VALUES ('302_classifier_source_revisions') ON CONFLICT DO NOTHING;
COMMIT;

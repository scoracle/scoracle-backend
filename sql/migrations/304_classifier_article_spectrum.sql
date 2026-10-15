BEGIN;
-- Old per-target scores remain history; pending character work adopts the article-level bank.
UPDATE public.classifier_spectrum_deliveries d SET status='superseded',
    reason='replaced_by_four_score_article_spectrum',updated_at=now()
FROM public.classifier_spectra x WHERE x.id=d.spectrum_id AND d.status='pending'
AND x.criteria_version<>'classification-criteria-v5-article';

-- Do not disturb current claims, retry backoff or acquisition. Completed old measurements
-- get one normal article claim; the worker retains its immutable source/model/request receipt.
INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
SELECT 'classifier','article',h.article_id,h.sport,h.source_id::text
FROM public.classifier_source_heads h
WHERE EXISTS(SELECT 1 FROM public.classifier_spectra x WHERE x.source_id=h.source_id)
AND NOT EXISTS(SELECT 1 FROM public.classifier_spectra x WHERE x.source_id=h.source_id
    AND x.criteria_version='classification-criteria-v5-article' AND x.target->>'entity_type'='article')
ON CONFLICT(stage,entity_type,entity_id,sport) DO NOTHING;

COMMENT ON TABLE public.classifier_spectra IS
    'Immutable source-bound measurements. Article targets retain one four-score model receipt; RSS-subject rows project it into entity-bound character obligations. Historical per-target measurements remain retained.';
INSERT INTO public.schema_migrations(version) VALUES ('304_classifier_article_spectrum') ON CONFLICT DO NOTHING;
COMMIT;

BEGIN;
ALTER TABLE public.classifier_spectrum_deliveries ADD COLUMN IF NOT EXISTS reason text;
ALTER TABLE public.classifier_spectrum_deliveries ADD COLUMN IF NOT EXISTS product_ref jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE public.classifier_spectrum_deliveries DROP CONSTRAINT IF EXISTS classifier_spectrum_deliveries_status_check;
ALTER TABLE public.classifier_spectrum_deliveries ADD CONSTRAINT classifier_spectrum_deliveries_status_check
    CHECK (status IN ('pending','used','abstained','superseded','held','redundant'));

-- Preserve old outputs for diagnosis; only complete native receipts can feed characters.
UPDATE public.classifier_spectrum_deliveries d SET status='superseded',
    reason='replaced_by_native_kev_receipt',updated_at=now()
FROM public.classifier_spectra x WHERE x.id=d.spectrum_id AND d.status='pending'
AND (x.receipt='{}'::jsonb OR x.request_hash='');

INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
SELECT DISTINCT 'classifier','article',h.article_id,h.sport,h.source_id::text
FROM public.classifier_source_heads h JOIN public.classifier_spectra x ON x.source_id=h.source_id
WHERE x.receipt='{}'::jsonb OR x.request_hash=''
ON CONFLICT(stage,entity_type,entity_id,sport) DO NOTHING;

INSERT INTO public.schema_migrations(version) VALUES ('303_classifier_spectrum_consumption') ON CONFLICT DO NOTHING;
COMMIT;

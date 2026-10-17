BEGIN;
-- Accept genuine subject-scoped native packets while preserving historical article receipts.
ALTER TABLE public.classifier_spectra DROP CONSTRAINT classifier_spectra_complete_receipt;
ALTER TABLE public.classifier_spectra ADD CONSTRAINT classifier_spectra_complete_receipt
    CHECK (COALESCE(gate->>'coverage'='complete'
        AND COALESCE(receipt#>>'{request,state,article,body}',receipt#>>'{request,state,body}')=clean_text
        AND receipt#>>'{response,model}'=model
        AND request_hash<>'',false)) NOT VALID;
COMMENT ON TABLE public.classifier_spectra IS
    'Immutable source-bound native measurements. v6 scores each RSS subject with canonical metadata; historical article spectra remain retained.';
INSERT INTO public.schema_migrations(version) VALUES ('312_classifier_subject_reader') ON CONFLICT DO NOTHING;
COMMIT;

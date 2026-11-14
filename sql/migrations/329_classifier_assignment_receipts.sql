BEGIN;
SET LOCAL lock_timeout='2s';
SET LOCAL statement_timeout='15s';

-- Bind the retained headline to the exact native request in either envelope.
ALTER TABLE public.classifier_spectra DROP CONSTRAINT classifier_spectra_complete_receipt;
ALTER TABLE public.classifier_spectra ADD CONSTRAINT classifier_spectra_complete_receipt
    CHECK (COALESCE(gate->>'coverage'='complete'
        AND CASE WHEN receipt->>'request_version'='classifier-brief-v2' THEN
            CASE WHEN jsonb_typeof(receipt#>'{request,state}')='string'
                THEN (regexp_match(receipt#>>'{request,state}', '(?n)^Headline: (.*)$'))[1]::jsonb#>>'{}'
            END
        WHEN receipt->>'request_version'='classifier-assignment-v1' THEN
            CASE WHEN jsonb_typeof(receipt#>'{request,state}')='string'
                THEN (receipt#>>'{request,state}')::jsonb#>>'{new_content,headline}'
            END
        ELSE COALESCE(receipt#>>'{request,state,article,headline}',
                      receipt#>>'{request,state,article,body}',receipt#>>'{request,state,body}')
        END = clean_text
        AND receipt#>>'{response,model}'=model
        AND request_hash<>'',false)) NOT VALID;

INSERT INTO public.schema_migrations(version) VALUES('329_classifier_assignment_receipts') ON CONFLICT DO NOTHING;
COMMIT;

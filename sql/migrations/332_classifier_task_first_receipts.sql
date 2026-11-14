BEGIN;
SET LOCAL lock_timeout='2s';
SET LOCAL statement_timeout='15s';

-- Bind the retained headline to the exact native request in either envelope.
ALTER TABLE public.classifier_spectra DROP CONSTRAINT classifier_spectra_complete_receipt;
ALTER TABLE public.classifier_spectra ADD CONSTRAINT classifier_spectra_complete_receipt
    CHECK (COALESCE(gate->>'coverage'='complete'
        AND CASE WHEN receipt->>'request_version' IN ('classifier-brief-v5-target-first','classifier-brief-v6-task-first','classifier-brief-v7-structured-target','classifier-brief-v8-shared-relations') THEN
            CASE WHEN jsonb_typeof(receipt#>'{request,state}')='string'
                THEN (receipt#>>'{request,state}')::jsonb#>>'{evidence,headline}'
            END
        WHEN receipt->>'request_version' IN ('classifier-brief-v2','classifier-brief-v3','classifier-brief-v4-target-context') THEN
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


CREATE OR REPLACE FUNCTION public.classifier_article_ready() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE measured record;
BEGIN
    IF NEW.status<>'pending' THEN RETURN NEW; END IF;
    SELECT x.criteria_version,x.gate,x.clean_text,s.source INTO STRICT measured
        FROM public.classifier_spectra x JOIN public.classifier_sources s ON s.id=x.source_id
        WHERE x.id=NEW.spectrum_id;
    IF measured.criteria_version IN ('classification-criteria-v9-headlines','classification-criteria-v10-availability','classification-criteria-v11-targeted','classification-criteria-v12-newsworthy','classification-criteria-v13-relations','classification-criteria-v14-concise','classification-criteria-v15-task-first','classification-criteria-v16-structured-target','classification-criteria-v17-shared-relations') THEN
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

INSERT INTO public.schema_migrations(version) VALUES('332_classifier_task_first_receipts') ON CONFLICT DO NOTHING;
COMMIT;

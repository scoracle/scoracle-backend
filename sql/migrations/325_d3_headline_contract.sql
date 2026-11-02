BEGIN;
SET LOCAL lock_timeout='2s';
SET LOCAL statement_timeout='15s';
-- Apply the existing headline provenance guard to the new prompt version. No stored rows are rescored.
CREATE OR REPLACE FUNCTION public.classifier_article_ready() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE measured record;
BEGIN
    IF NEW.status<>'pending' THEN RETURN NEW; END IF;
    SELECT x.criteria_version,x.gate,x.clean_text,s.source INTO STRICT measured
        FROM public.classifier_spectra x JOIN public.classifier_sources s ON s.id=x.source_id
        WHERE x.id=NEW.spectrum_id;
    IF measured.criteria_version IN ('classification-criteria-v9-headlines','classification-criteria-v10-availability','classification-criteria-v11-targeted') THEN
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
INSERT INTO public.schema_migrations(version) VALUES('325_d3_headline_contract') ON CONFLICT DO NOTHING;
COMMIT;

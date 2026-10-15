BEGIN;
-- Dispatch character work from a spectrum obligation. This replaces the retired
-- measurement-based trigger (293): the spectrum is the source of truth, and each fired
-- character gets one durable pipeline_work item whose input_version carries the spectrum
-- contract prefix that Insider/Scout key on.
CREATE OR REPLACE FUNCTION public.classifier_spectrum_dispatch()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_type text;
    target_id integer;
    league text;
    stage text;
    revision text;
BEGIN
    IF NEW.status <> 'pending' THEN RETURN NEW; END IF;
    stage := CASE NEW.character
        WHEN 'journalist' THEN 'narratives'
        WHEN 'insider' THEN 'transfers'
        WHEN 'scout' THEN 'rating'
        WHEN 'influencer' THEN 'vibe'
        ELSE NULL END;
    IF stage IS NULL THEN
        RAISE EXCEPTION 'Classifier spectrum character is not wired: %', NEW.character;
    END IF;
    SELECT x.target->>'entity_type', (x.target->>'entity_id')::integer, x.sport
      INTO target_type, target_id, league
      FROM public.classifier_spectra x WHERE x.id = NEW.spectrum_id;
    IF target_type IS NULL OR target_type NOT IN ('team','player') OR target_id IS NULL OR target_id <= 0 THEN
        RAISE EXCEPTION 'Classifier spectrum delivery requires a canonical team/player target';
    END IF;
    revision := 'classifier-spectrum-v1:' || NEW.character || ':s' || NEW.spectrum_id;
    INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
    VALUES(stage,target_type,target_id,league,revision)
    ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
        status='pending',attempts=0,input_version=EXCLUDED.input_version,
        available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
        updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
    WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version;
    RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS classifier_spectrum_delivery_dispatch ON public.classifier_spectrum_deliveries;
CREATE TRIGGER classifier_spectrum_delivery_dispatch AFTER INSERT
    ON public.classifier_spectrum_deliveries FOR EACH ROW
    EXECUTE FUNCTION public.classifier_spectrum_dispatch();

INSERT INTO public.schema_migrations(version) VALUES ('301_classifier_spectrum_dispatch') ON CONFLICT DO NOTHING;
COMMIT;

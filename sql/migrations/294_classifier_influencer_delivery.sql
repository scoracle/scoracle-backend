BEGIN;
-- Release and dispatch Influencer obligations under the existing claim fence.
CREATE OR REPLACE FUNCTION public.classifier_dispatch_delivery()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_type text;
    target_id integer;
    league text;
    revision text;
    destination text;
BEGIN
    IF NEW.status <> 'pending' OR NOT NEW.production_eligible THEN RETURN NEW; END IF;
    IF TG_OP = 'UPDATE' THEN
        IF OLD.status = 'pending' AND OLD.production_eligible
            AND OLD.policy_version = NEW.policy_version THEN RETURN NEW; END IF;
    END IF;
    destination := CASE NEW.plugin_id
        WHEN 'scoracle.character.narrative' THEN 'narratives'
        WHEN 'scoracle.character.vibe' THEN 'vibe' END;
    IF destination IS NULL THEN
        RAISE EXCEPTION 'Classifier delivery plugin is not wired: %', NEW.plugin_id;
    END IF;
    SELECT m.receipt->'target'->>'entity_type', (m.receipt->'target'->>'entity_id')::integer, m.sport
      INTO target_type,target_id,league FROM public.classifier_measurements m
      WHERE m.id=NEW.measurement_id AND m.status='source_bound_provisional';
    IF target_type IS NULL OR target_type NOT IN ('team','player') OR target_id IS NULL OR target_id <= 0 THEN
        RAISE EXCEPTION 'Classifier delivery requires a canonical team/player target';
    END IF;
    revision := 'classifier-qualified-claims-v1:' || NEW.policy_version || ':m' || NEW.measurement_id;
    INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
    VALUES(destination,target_type,target_id,league,revision)
    ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
        status='pending',attempts=0,input_version=EXCLUDED.input_version,
        available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
        updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
    WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version;
    RETURN NEW;
END;
$$;
INSERT INTO public.schema_migrations(version) VALUES ('294_classifier_influencer_delivery') ON CONFLICT DO NOTHING;
COMMIT;

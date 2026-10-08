BEGIN;
-- Scout's evaluated selection is explicit; raw measurements never imply a trigger.
ALTER TABLE public.classifier_deliveries ADD COLUMN IF NOT EXISTS selection jsonb;
ALTER TABLE public.classifier_deliveries DROP CONSTRAINT IF EXISTS classifier_scout_selection;
ALTER TABLE public.classifier_deliveries ADD CONSTRAINT classifier_scout_selection CHECK (
    plugin_id<>'scoracle.character.rating' OR NOT production_eligible OR status IN ('held','superseded')
    OR COALESCE(jsonb_typeof(selection)='object'
        AND selection->>'kind' IN ('performance','roster','availability'),false));
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
            AND OLD.policy_version = NEW.policy_version AND OLD.selection IS NOT DISTINCT FROM NEW.selection THEN RETURN NEW; END IF;
    END IF;
    destination := CASE NEW.plugin_id
        WHEN 'scoracle.character.narrative' THEN 'narratives'
        WHEN 'scoracle.character.vibe' THEN 'vibe'
        WHEN 'scoracle.character.transfers' THEN 'transfers'
        WHEN 'scoracle.character.rating' THEN 'rating' END;
    IF destination IS NULL THEN
        RAISE EXCEPTION 'Classifier delivery plugin is not wired: %', NEW.plugin_id;
    END IF;
    SELECT m.receipt->'target'->>'entity_type', (m.receipt->'target'->>'entity_id')::integer, m.sport
      INTO target_type,target_id,league FROM public.classifier_measurements m
      WHERE m.id=NEW.measurement_id AND m.status='source_bound_provisional';
    IF target_type IS NULL OR (target_type NOT IN ('team','player') AND NOT (destination='transfers' AND target_type='person')) OR target_id IS NULL OR target_id <= 0 THEN
        RAISE EXCEPTION 'Classifier delivery requires a canonical team/player target';
    END IF;
    revision := 'classifier-qualified-claims-v1:' || NEW.policy_version || ':m' || NEW.measurement_id || ':' || md5(COALESCE(NEW.selection::text,''));
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
DROP TRIGGER IF EXISTS classifier_delivery_dispatch ON public.classifier_deliveries;
CREATE TRIGGER classifier_delivery_dispatch AFTER INSERT OR UPDATE OF status,production_eligible,policy_version,selection
    ON public.classifier_deliveries FOR EACH ROW EXECUTE FUNCTION public.classifier_dispatch_delivery();
INSERT INTO public.schema_migrations(version) VALUES ('296_classifier_scout_delivery') ON CONFLICT DO NOTHING;
COMMIT;

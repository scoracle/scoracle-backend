BEGIN;
-- Independent character obligations; unassessed evidence remains held.
CREATE TABLE IF NOT EXISTS public.classifier_deliveries (
    measurement_id bigint NOT NULL REFERENCES public.classifier_measurements(id) ON DELETE CASCADE,
    plugin_id text NOT NULL,
    policy_version text NOT NULL,
    status text NOT NULL DEFAULT 'held'
        CHECK (status IN ('held','pending','used','abstained','redundant','superseded')),
    reason text,
    production_eligible boolean NOT NULL DEFAULT false,
    product_ref jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (measurement_id,plugin_id),
    CHECK (status NOT IN ('pending','used','abstained','redundant') OR production_eligible)
);
CREATE INDEX IF NOT EXISTS classifier_delivery_pending
    ON public.classifier_deliveries(plugin_id,measurement_id) WHERE status='pending';
COMMENT ON TABLE public.classifier_deliveries IS
    'Versioned native character obligations; measurement scores never implicitly release unassessed evidence.';
-- Releasing a ready obligation and its durable dispatch are one database operation.
-- Repeated updates do not reset an active lease or an unchanged failed attempt.
CREATE OR REPLACE FUNCTION public.classifier_dispatch_delivery()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_type text;
    target_id integer;
    league text;
    revision text;
BEGIN
    IF NEW.status <> 'pending' OR NOT NEW.production_eligible THEN RETURN NEW; END IF;
    IF TG_OP = 'UPDATE' THEN
        IF OLD.status = 'pending' AND OLD.production_eligible
            AND OLD.policy_version = NEW.policy_version THEN RETURN NEW; END IF;
    END IF;
    IF NEW.plugin_id <> 'scoracle.character.narrative' THEN
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
    VALUES('narratives',target_type,target_id,league,revision)
    ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
        status='pending',attempts=0,input_version=EXCLUDED.input_version,
        available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
        updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
    WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version;
    RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS classifier_delivery_dispatch ON public.classifier_deliveries;
CREATE TRIGGER classifier_delivery_dispatch AFTER INSERT OR UPDATE OF status,production_eligible,policy_version
    ON public.classifier_deliveries FOR EACH ROW EXECUTE FUNCTION public.classifier_dispatch_delivery();
INSERT INTO public.schema_migrations(version) VALUES ('293_classifier_character_delivery') ON CONFLICT DO NOTHING;
COMMIT;

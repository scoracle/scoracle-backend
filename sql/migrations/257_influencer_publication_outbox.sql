-- 257_influencer_publication_outbox.sql
--
-- The Influencer is the first model-facing seat whose publication is joined to its exact
-- pipeline_work claim. After inference, one short transaction locks the claimed row, writes the
-- vibe product (with its required provenance), records a durable reconciliation intent, and
-- deletes the claim. A stale execution therefore publishes nothing.
--
-- The outbox is intentionally narrow. `vibe_completed` means "reconcile the deterministic
-- Momentum offer, then check the Oracle completion barrier". The worker retries that idempotent
-- database-only work until it succeeds. Other seats remain on their existing publication path.

BEGIN;

CREATE TABLE IF NOT EXISTS public.application_outbox (
    id uuid DEFAULT gen_random_uuid() PRIMARY KEY,
    kind text NOT NULL,
    source_stage text NOT NULL,
    source_claim_token uuid NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL REFERENCES public.sports(id),
    source_input_version text,
    attempts integer NOT NULL DEFAULT 0,
    available_at timestamptz NOT NULL DEFAULT now(),
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT application_outbox_kind_check CHECK (kind = 'vibe_completed'),
    CONSTRAINT application_outbox_source_stage_check CHECK (source_stage = 'vibe'),
    CONSTRAINT application_outbox_entity_type_check
        CHECK (entity_type = ANY (ARRAY['player'::text, 'team'::text])),
    CONSTRAINT application_outbox_source_claim_unique
        UNIQUE (kind, source_stage, source_claim_token)
);

CREATE INDEX IF NOT EXISTS idx_application_outbox_ready
    ON public.application_outbox (available_at, created_at);

COMMENT ON TABLE public.application_outbox IS
    'Durable post-publication reconciliation. Initially owns only Influencer completion: Momentum offer, then Oracle barrier.';
COMMENT ON COLUMN public.application_outbox.source_claim_token IS
    'The exact pipeline_work lease committed atomically with the product and queue completion.';

CREATE OR REPLACE FUNCTION public.notify_application_outbox_ready()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS application_outbox_notify_insert ON public.application_outbox;
CREATE TRIGGER application_outbox_notify_insert
    AFTER INSERT ON public.application_outbox
    FOR EACH ROW
    EXECUTE FUNCTION public.notify_application_outbox_ready();

INSERT INTO public.schema_migrations(version)
VALUES ('257_influencer_publication_outbox')
ON CONFLICT DO NOTHING;

COMMIT;

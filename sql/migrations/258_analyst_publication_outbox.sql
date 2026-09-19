-- 258_analyst_publication_outbox.sql
--
-- Extend the deliberately narrow publication outbox to the Analyst. A completed Momentum claim
-- carries one obligation: evaluate the Oracle pillar barrier after the product (or NoMaterial),
-- required provenance, and exact claim completion commit together.

BEGIN;

ALTER TABLE public.application_outbox
    DROP CONSTRAINT IF EXISTS application_outbox_kind_check,
    DROP CONSTRAINT IF EXISTS application_outbox_source_stage_check,
    DROP CONSTRAINT IF EXISTS application_outbox_kind_stage_check;

ALTER TABLE public.application_outbox
    ADD CONSTRAINT application_outbox_kind_stage_check CHECK (
        (kind = 'vibe_completed' AND source_stage = 'vibe')
        OR (kind = 'momentum_completed' AND source_stage = 'momentum')
    );

COMMENT ON TABLE public.application_outbox IS
    'Durable post-publication reconciliation for claim-aware seats: Vibe completion offers Momentum then checks Oracle; Momentum completion checks Oracle.';

INSERT INTO public.schema_migrations(version)
VALUES ('258_analyst_publication_outbox')
ON CONFLICT DO NOTHING;

COMMIT;

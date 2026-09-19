-- 259_scout_publication_outbox.sql
--
-- Extend exact-claim publication fencing to the Scout. A published rating (including the durable
-- no-stats marker) offers Momentum and then checks Oracle. A stats-debounced completion publishes
-- no product and must only check Oracle, preserving the existing downstream behavior.

BEGIN;

ALTER TABLE public.application_outbox
    DROP CONSTRAINT IF EXISTS application_outbox_kind_check,
    DROP CONSTRAINT IF EXISTS application_outbox_source_stage_check,
    DROP CONSTRAINT IF EXISTS application_outbox_kind_stage_check;

ALTER TABLE public.application_outbox
    ADD CONSTRAINT application_outbox_kind_stage_check CHECK (
        (kind = 'vibe_completed' AND source_stage = 'vibe')
        OR (kind = 'momentum_completed' AND source_stage = 'momentum')
        OR (kind = 'rating_completed' AND source_stage = 'rating')
        OR (kind = 'rating_debounced' AND source_stage = 'rating')
    );

COMMENT ON TABLE public.application_outbox IS
    'Durable post-publication reconciliation for claim-aware seats: product-bearing Vibe and Rating completions offer Momentum then check Oracle; Momentum and debounced Rating completions check Oracle.';

INSERT INTO public.schema_migrations(version)
VALUES ('259_scout_publication_outbox')
ON CONFLICT DO NOTHING;

COMMIT;

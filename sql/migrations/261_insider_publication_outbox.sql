BEGIN;

ALTER TABLE public.application_outbox
    DROP CONSTRAINT IF EXISTS application_outbox_kind_stage_check,
    DROP CONSTRAINT IF EXISTS application_outbox_source_claim_unique;

DROP INDEX IF EXISTS public.application_outbox_completion_claim_unique;
DROP INDEX IF EXISTS public.application_outbox_transfer_target_unique;

ALTER TABLE public.application_outbox
    ADD CONSTRAINT application_outbox_kind_stage_check CHECK (
        (kind = 'vibe_completed' AND source_stage = 'vibe')
        OR (kind = 'momentum_completed' AND source_stage = 'momentum')
        OR (kind = 'rating_completed' AND source_stage = 'rating')
        OR (kind = 'rating_debounced' AND source_stage = 'rating')
        OR (kind = 'narratives_completed' AND source_stage = 'narratives')
        OR (kind = 'transfer_published' AND source_stage = 'transfers')
    );

CREATE UNIQUE INDEX application_outbox_completion_claim_unique
    ON public.application_outbox (kind, source_stage, source_claim_token)
    WHERE kind <> 'transfer_published';

CREATE UNIQUE INDEX application_outbox_transfer_target_unique
    ON public.application_outbox (
        kind, source_stage, source_claim_token, entity_type, entity_id, sport
    )
    WHERE kind = 'transfer_published';

COMMENT ON TABLE public.application_outbox IS
    'Durable post-publication reconciliation for claim-aware seats. Transfer events may fan one team claim out to several player/team Oracle barriers.';

COMMIT;

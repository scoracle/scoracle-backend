-- Make the immediate Scout refresh required by an applied transfer identity durable.
-- One transfer claim may apply one or more identities, and each application fans out
-- to the player plus its old/new teams, so these obligations use target-level identity.
BEGIN;

ALTER TABLE public.application_outbox
    DROP CONSTRAINT application_outbox_kind_stage_check;

DROP INDEX public.application_outbox_completion_claim_unique;
DROP INDEX public.application_outbox_transfer_target_unique;

ALTER TABLE public.application_outbox
    ADD CONSTRAINT application_outbox_kind_stage_check CHECK (
        (kind = 'vibe_completed' AND source_stage = 'vibe')
        OR (kind = 'momentum_completed' AND source_stage = 'momentum')
        OR (kind = 'rating_completed' AND source_stage = 'rating')
        OR (kind = 'rating_debounced' AND source_stage = 'rating')
        OR (kind = 'narratives_completed' AND source_stage = 'narratives')
        OR (kind = 'transfer_published' AND source_stage = 'transfers')
        OR (kind = 'transfer_identity_applied' AND source_stage = 'transfers')
    );

CREATE UNIQUE INDEX application_outbox_completion_claim_unique
    ON public.application_outbox (kind, source_stage, source_claim_token)
    WHERE kind NOT IN ('transfer_published', 'transfer_identity_applied');

CREATE UNIQUE INDEX application_outbox_transfer_target_unique
    ON public.application_outbox (
        kind, source_stage, source_claim_token, entity_type, entity_id, sport
    )
    WHERE kind IN ('transfer_published', 'transfer_identity_applied');

COMMENT ON TABLE public.application_outbox IS
    'Durable post-publication reconciliation for claim-aware seats. Transfer publication and applied-identity events may fan one team claim out to several player/team targets.';

INSERT INTO public.schema_migrations(version)
VALUES ('267_transfer_identity_rating_outbox')
ON CONFLICT DO NOTHING;

COMMIT;

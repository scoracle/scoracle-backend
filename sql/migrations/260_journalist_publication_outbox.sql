-- 260_journalist_publication_outbox.sql
--
-- Extend exact-claim publication fencing to the Journalist. Product-bearing, called-empty,
-- no-corpus, and debounced completions all carry the same obligation: check the Oracle pillar
-- barrier after multi-row/marker publication, storyline progression, and exact completion commit.

BEGIN;

ALTER TABLE public.application_outbox
    DROP CONSTRAINT IF EXISTS application_outbox_kind_stage_check;

ALTER TABLE public.application_outbox
    ADD CONSTRAINT application_outbox_kind_stage_check CHECK (
        (kind = 'vibe_completed' AND source_stage = 'vibe')
        OR (kind = 'momentum_completed' AND source_stage = 'momentum')
        OR (kind = 'rating_completed' AND source_stage = 'rating')
        OR (kind = 'rating_debounced' AND source_stage = 'rating')
        OR (kind = 'narratives_completed' AND source_stage = 'narratives')
    );

COMMENT ON TABLE public.application_outbox IS
    'Durable post-publication reconciliation for claim-aware seats: product-bearing Vibe and Rating completions offer Momentum then check Oracle; Momentum, debounced Rating, and Narratives completions check Oracle.';

INSERT INTO public.schema_migrations(version)
VALUES ('260_journalist_publication_outbox')
ON CONFLICT DO NOTHING;

COMMIT;

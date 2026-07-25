-- 192_cognition_ledger_article_entity.sql
--
-- Graph extraction is model-backed article work and writes cognition ledger rows
-- with entity_type='article'. The original ledger constraint only allowed player
-- and team rows, which made graph GPU time invisible in DB telemetry.

BEGIN;

ALTER TABLE public.cognition_ledger
    DROP CONSTRAINT IF EXISTS cognition_ledger_entity_type_check;

ALTER TABLE public.cognition_ledger
    ADD CONSTRAINT cognition_ledger_entity_type_check
    CHECK (entity_type IN ('player', 'team', 'article'));

INSERT INTO public.schema_migrations(version)
VALUES ('192_cognition_ledger_article_entity')
ON CONFLICT DO NOTHING;

COMMIT;

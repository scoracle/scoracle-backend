-- A missing recent pair corpus is an abstention, not a model-cleared rumor.
BEGIN;
ALTER TABLE public.harvester_insider_pairs
    DROP CONSTRAINT harvester_insider_pairs_status_check;
ALTER TABLE public.harvester_insider_pairs
    ADD CONSTRAINT harvester_insider_pairs_status_check
    CHECK (status IN ('pending','rumor','cleared','abstained'));
INSERT INTO public.schema_migrations(version)
VALUES ('275_harvester_insider_pair_abstention') ON CONFLICT DO NOTHING;
COMMIT;

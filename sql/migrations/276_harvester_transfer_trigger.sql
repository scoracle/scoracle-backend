-- Distinguish source-grounded Harvester transfer verdicts from periodic scans.
BEGIN;
ALTER TABLE public.transfer_rumors
    DROP CONSTRAINT transfer_rumors_trigger_type_check;
ALTER TABLE public.transfer_rumors
    ADD CONSTRAINT transfer_rumors_trigger_type_check
    CHECK (trigger_type IN ('news_spike','periodic','manual','harvester'));
INSERT INTO public.schema_migrations(version)
VALUES ('276_harvester_transfer_trigger') ON CONFLICT DO NOTHING;
COMMIT;

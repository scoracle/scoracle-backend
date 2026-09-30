BEGIN;

ALTER TABLE public.harvester_insider_wraps
    DROP CONSTRAINT harvester_insider_wraps_status_check;

ALTER TABLE public.harvester_insider_wraps
    ADD CONSTRAINT harvester_insider_wraps_status_check
    CHECK (status IN ('pending','scored','skipped','superseded'));

COMMENT ON COLUMN public.harvester_insider_wraps.status IS
    'Pending wraps are terminally superseded when a newer Harvester classification reopens the team claim; scored and skipped rows retain their original outcome.';

INSERT INTO public.schema_migrations(version)
VALUES ('281_harvester_insider_wrap_superseded') ON CONFLICT (version) DO NOTHING;

COMMIT;

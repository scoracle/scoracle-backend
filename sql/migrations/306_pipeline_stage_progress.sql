BEGIN;
-- Claim completion is the successful signal even when a plugin legitimately abstains.
CREATE TABLE IF NOT EXISTS public.pipeline_stage_progress (
    stage text PRIMARY KEY,
    completed_at timestamptz NOT NULL
);
CREATE OR REPLACE FUNCTION public.pipeline_record_completion()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO public.pipeline_stage_progress(stage,completed_at) VALUES(OLD.stage,clock_timestamp())
    ON CONFLICT(stage) DO UPDATE SET completed_at=EXCLUDED.completed_at;
    RETURN OLD;
END;
$$;
DROP TRIGGER IF EXISTS pipeline_record_completion ON public.pipeline_work;
CREATE TRIGGER pipeline_record_completion AFTER DELETE ON public.pipeline_work
    FOR EACH ROW WHEN (OLD.status='running') EXECUTE FUNCTION public.pipeline_record_completion();
INSERT INTO public.schema_migrations(version) VALUES ('306_pipeline_stage_progress') ON CONFLICT DO NOTHING;
COMMIT;

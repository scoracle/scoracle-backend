-- A bounded live canary needs a durable cohort receipt because successful
-- pipeline_work claims are deleted inside their publication transaction.
BEGIN;
CREATE TABLE public.harvester_live_canary_items (
    run_id bigint NOT NULL REFERENCES public.pipeline_runs(id) ON DELETE CASCADE,
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL,
    enqueued_at timestamptz NOT NULL DEFAULT now(),
    acquisition_attempts_before integer NOT NULL CHECK (acquisition_attempts_before >= 0),
    PRIMARY KEY (run_id,article_id,sport)
);
COMMENT ON TABLE public.harvester_live_canary_items IS
    'Durable bounded Harvester live-canary cohort. Successful pipeline_work claims are deleted; compare the acquisition attempt count and updated_at with this enqueue receipt before calling a replay complete.';
INSERT INTO public.schema_migrations(version)
VALUES ('285_harvester_live_canary_items') ON CONFLICT DO NOTHING;
COMMIT;

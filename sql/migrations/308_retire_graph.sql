BEGIN;
-- Stop old writers and deploy the Graph-free worker before resuming acquisition.
-- Removing running claims also fences any late publication from an old Graph worker.
DELETE FROM public.pipeline_work WHERE stage='graph';
DELETE FROM public.pipeline_stage_progress WHERE stage='graph';
ALTER TABLE public.pipeline_work DROP CONSTRAINT IF EXISTS pipeline_work_graph_retired;
ALTER TABLE public.pipeline_work ADD CONSTRAINT pipeline_work_graph_retired CHECK (stage <> 'graph');
-- Retain historical extractions, event/story links, identities and candidate evidence.
INSERT INTO public.schema_migrations(version) VALUES ('308_retire_graph') ON CONFLICT DO NOTHING;
COMMIT;

-- Reopen a bounded sample of character work that exhausted its attempts on
-- the palette schema's missing maximum choice index. Use only after installing
-- the fixed cognition binary. This never touches acquisition dead letters.
--
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 \
--   -v per_stage_limit=2 -f rust/examples/harvester_requeue_palette_failures.sql
BEGIN;
WITH ranked AS (
    SELECT ctid,stage,
           row_number() OVER (PARTITION BY stage ORDER BY updated_at DESC) AS rank
      FROM public.pipeline_work
     WHERE status='failed' AND attempts>=5
       AND last_error='composition used an unknown phrasing'
       AND stage IN ('narratives','vibe','transfers','rating','momentum','sigil')
), reopened AS (
    UPDATE public.pipeline_work w
       SET status='pending',attempts=0,available_at=now(),updated_at=now(),
           last_error='requeued after palette choice bound',
           running_input_version=NULL,claim_token=NULL
      FROM ranked r
     WHERE w.ctid=r.ctid
       AND r.rank<=LEAST(GREATEST(:'per_stage_limit'::int,1),20)
     RETURNING w.stage
)
SELECT stage,count(*) AS reopened FROM reopened GROUP BY stage ORDER BY stage;
COMMIT;

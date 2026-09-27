-- Read-only, aggregate-only audit for a named live Harvester canary run.
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 -v run_id=333 \
--   -f rust/examples/harvester_live_canary_check.sql
WITH canary AS (
    SELECT entity_id AS article_id,sport,status
      FROM public.pipeline_work
     WHERE stage='harvester' AND entity_type='article'
       AND input_version LIKE ('harvest-context-v1:live-canary:run' || :'run_id' || ':%')
), classified AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport) c.*
      FROM public.harvester_classifications c
      JOIN canary x ON x.article_id=c.article_id AND x.sport=c.sport
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
), assignments AS (
    SELECT d.* FROM public.harvester_assignments d
      JOIN classified c ON c.id=d.classification_id
)
SELECT (SELECT count(*) FROM canary) AS canary_articles,
       (SELECT count(*) FROM canary WHERE status='completed') AS completed_articles,
       (SELECT count(*) FROM canary WHERE status='failed') AS failed_articles,
       (SELECT count(*) FROM classified) AS classified_edges,
       (SELECT count(*) FROM classified c JOIN public.news_articles a ON a.id=c.article_id
         WHERE a.full_text IS NOT NULL AND a.title=c.headline
           AND encode(sha256(convert_to(a.full_text,'UTF8')),'hex')=c.body_sha256
           AND substring(convert_to(a.full_text,'UTF8') FROM c.context_start+1
                         FOR c.context_end-c.context_start)=convert_to(c.context_text,'UTF8')
           AND substring(convert_to(a.full_text,'UTF8') FROM c.model_input_start+1
                         FOR c.model_input_end-c.model_input_start)=convert_to(c.model_input_text,'UTF8'))
           AS exact_source_edges,
       (SELECT count(*) FROM assignments) AS assignments,
       (SELECT count(*) FROM assignments WHERE status='pending' AND reason='delivery_held')
           AS held_assignments,
       (SELECT count(*) FROM assignments WHERE reason IS DISTINCT FROM 'delivery_held')
           AS unheld_assignments,
       (SELECT count(*) FROM public.harvester_resolved_links l
         JOIN canary x ON x.article_id=l.article_id AND x.sport=l.sport) AS resolved_links,
       (SELECT count(*) FROM public.harvester_unresolved_names n
         JOIN canary x ON x.article_id=n.article_id AND x.sport=n.sport) AS unresolved_names,
       (SELECT count(*) FROM public.pipeline_work g
         JOIN canary x ON x.article_id=g.entity_id AND x.sport=g.sport
         WHERE g.stage='graph' AND g.entity_type='article') AS graph_work_rows;

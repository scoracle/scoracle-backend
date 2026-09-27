-- Read-only, aggregate-only audit for a named live Harvester canary run.
-- Reads durable migration-285 cohort receipts because successful queue claims
-- are deleted at commit. A deleted claim alone is not a completion receipt.
-- psql "$DATABASE_PRIVATE_URL" -X -v ON_ERROR_STOP=1 -v run_id=333 \
--   -f rust/examples/harvester_live_canary_check.sql
-- Pass -v canary_after='2026-09-27 11:48:10-04' to inspect only a later
-- bounded batch when the same ingest run has earlier canary receipts.
\if :{?canary_after}
\else
\set canary_after 1970-01-01T00:00:00Z
\endif
WITH canary AS (
    SELECT x.article_id,x.sport,x.enqueued_at,w.status AS work_status,
           h.status AS acquisition_status,
           h.attempts>x.acquisition_attempts_before
             AND h.updated_at>=x.enqueued_at AS replayed
      FROM public.harvester_live_canary_items x
      LEFT JOIN public.pipeline_work w
        ON w.stage='harvester' AND w.entity_type='article'
       AND w.entity_id=x.article_id AND w.sport=x.sport
       AND w.input_version=('harvest-context-v5:live-canary:run' || x.run_id
                            || ':a' || x.article_id)
      LEFT JOIN public.harvester_acquisitions h ON h.article_id=x.article_id
     WHERE x.run_id=:'run_id'::bigint
       AND x.enqueued_at>=:'canary_after'::timestamptz
), classified AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport) c.*
      FROM public.harvester_classifications c
      JOIN canary x ON x.article_id=c.article_id AND x.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
), headline_gates AS (
    SELECT DISTINCT ON (g.article_id,g.entity_type,g.entity_id,g.sport) g.*
      FROM public.harvester_headline_gates g
      JOIN canary x ON x.article_id=g.article_id AND x.sport=g.sport
     WHERE g.contract_version='harvest-headline-v2'
       AND g.policy_version='headline-read-p025-v1'
     ORDER BY g.article_id,g.entity_type,g.entity_id,g.sport,g.created_at DESC
), assignments AS (
    SELECT d.* FROM public.harvester_assignments d
      JOIN classified c ON c.id=d.classification_id
)
SELECT (SELECT count(*) FROM canary) AS canary_articles,
       (SELECT count(*) FROM canary WHERE work_status IS NULL AND replayed
          AND acquisition_status IN ('acquired','duplicate')) AS completed_articles,
       (SELECT count(*) FROM canary WHERE work_status='failed') AS failed_articles,
       (SELECT count(*) FROM canary WHERE work_status IN ('pending','running'))
           AS active_articles,
       (SELECT count(*) FROM canary WHERE work_status IS NULL AND NOT COALESCE(replayed,false))
           AS missing_replay_receipts,
       (SELECT count(*) FROM headline_gates) AS headline_gate_edges,
       (SELECT count(*) FROM headline_gates WHERE admitted) AS headline_admitted_edges,
       (SELECT count(*) FROM headline_gates WHERE NOT admitted) AS headline_rejected_edges,
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
         WHERE g.stage='graph' AND g.entity_type='article') AS graph_work_rows,
       (SELECT count(*) FROM public.graph_extractions g
         JOIN canary x ON x.article_id=g.article_id AND x.sport=g.sport
        WHERE g.extracted_at>=x.enqueued_at) AS graph_receipts;

WITH canary AS (
    SELECT article_id,sport FROM public.harvester_live_canary_items
     WHERE run_id=:'run_id'::bigint
       AND enqueued_at>=:'canary_after'::timestamptz
), classified AS (
    SELECT DISTINCT ON (c.article_id,c.entity_type,c.entity_id,c.sport) c.id
      FROM public.harvester_classifications c
      JOIN canary x ON x.article_id=c.article_id AND x.sport=c.sport
     WHERE c.contract_version='harvest-context-v5'
     ORDER BY c.article_id,c.entity_type,c.entity_id,c.sport,c.created_at DESC,c.id DESC
)
SELECT d.plugin_id,d.status,count(*) AS assignments,
       count(*) FILTER (WHERE d.reason='delivery_held') AS held,
       count(*) FILTER (WHERE d.product_ref IS NOT NULL) AS with_product_receipt
  FROM public.harvester_assignments d JOIN classified c ON c.id=d.classification_id
 GROUP BY d.plugin_id,d.status ORDER BY d.plugin_id,d.status;

-- Read-only acquisition pilot: run after migration 307 and a bounded worker cohort.
BEGIN TRANSACTION READ ONLY;

SELECT status,reason,count(*) AS attempts,count(DISTINCT article_id) AS articles,
       round((percentile_cont(0.5) WITHIN GROUP (ORDER BY (receipt->>'elapsed_ms')::numeric))::numeric) AS median_ms,
       round((percentile_cont(0.95) WITHIN GROUP (ORDER BY (receipt->>'elapsed_ms')::numeric))::numeric) AS p95_ms
FROM classifier_acquisitions WHERE created_at >= now()-interval '24 hours'
GROUP BY status,reason ORDER BY status,attempts DESC;

SELECT a.source,
       count(DISTINCT r.article_id) FILTER (WHERE r.status='acquired') AS acquired_articles,
       count(DISTINCT r.article_id) FILTER (WHERE r.status='skipped') AS skipped_articles,
       count(*) FILTER (WHERE r.status='retryable_error') AS deferred_attempts,
       count(*) FILTER (WHERE (r.receipt->>'reused')::boolean) AS retained_body_reuses
FROM classifier_acquisitions r JOIN news_articles a ON a.id=r.article_id
WHERE r.created_at >= now()-interval '24 hours'
GROUP BY a.source ORDER BY acquired_articles DESC,skipped_articles DESC;

SELECT w.status,count(*) AS articles,
       count(*) FILTER (WHERE a.published_at >= now()-interval '48 hours') AS fresh_articles,
       count(*) FILTER (WHERE a.published_at IS NULL) AS unknown_publication_date,
       min(a.published_at) AS oldest_published
FROM pipeline_work w JOIN news_articles a ON a.id=w.entity_id
WHERE w.stage='classifier_acquire' AND w.entity_type='article'
GROUP BY w.status;

-- Historical rank slices approximate the 25/50 tradeoff. They do not measure story recall
-- or simulate alias lanes that a lower RSS limit would stop querying.
SELECT p.sport,p.entity_id,t.name,
       count(*) FILTER (WHERE p.feed_rank<25) AS ranked_under_25,
       count(*) FILTER (WHERE p.feed_rank<50) AS ranked_under_50,
       count(*) AS retained_discoveries,
       count(DISTINCT a.source) FILTER (WHERE p.feed_rank<25) AS publishers_under_25,
       count(DISTINCT a.source) FILTER (WHERE p.feed_rank<50) AS publishers_under_50
FROM harvester_query_provenance p JOIN news_articles a ON a.id=p.article_id
JOIN teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport
WHERE a.published_at >= now()-interval '24 hours'
GROUP BY p.sport,p.entity_id,t.name ORDER BY retained_discoveries DESC;

-- Review these headlines for important reporting covered by another accepted publisher.
SELECT a.id,a.title,a.source,a.published_at,r.reason,a.url
FROM classifier_acquisitions r JOIN news_articles a ON a.id=r.article_id
WHERE r.status='skipped' AND r.created_at >= now()-interval '24 hours'
ORDER BY r.created_at DESC LIMIT 30;
COMMIT;

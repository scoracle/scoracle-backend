BEGIN READ ONLY;
WITH largest AS (
 SELECT entity_type,entity_id,sport,spectrum_ids FROM coverage_assignments
 WHERE edition_date=(SELECT max(edition_date) FROM coverage_editions WHERE frozen_at IS NOT NULL)
 AND stage='narratives' AND outcome='pending'
 ORDER BY cardinality(spectrum_ids) DESC,entity_type,entity_id LIMIT 10
), reports AS (
 SELECT a.entity_type,a.entity_id,a.sport,cardinality(a.spectrum_ids) AS memberships,x.clean_text,s.article_id,s.source->>'source' AS publisher,
 s.source->>'published_at' AS published_at,
 EXISTS(SELECT 1 FROM classifier_source_heads h WHERE h.article_id=s.article_id AND h.sport=s.sport AND h.source_id=s.id) AS current_head
 FROM largest a CROSS JOIN LATERAL unnest(a.spectrum_ids) ids(id)
 JOIN classifier_spectra x ON x.id=ids.id JOIN classifier_sources s ON s.id=x.source_id
)
SELECT row_to_json(a) FROM (
 SELECT entity_type,entity_id,sport,max(memberships) AS memberships,count(DISTINCT article_id) AS articles,
 count(DISTINCT clean_text) AS distinct_texts,count(DISTINCT (publisher,clean_text)) AS distinct_publisher_texts,
 sum(length(clean_text)) AS source_chars,min(published_at) AS oldest,max(published_at) AS newest,
 count(*) FILTER(WHERE current_head) AS current_head_memberships
 FROM reports GROUP BY entity_type,entity_id,sport ORDER BY max(memberships) DESC
) a;
ROLLBACK;

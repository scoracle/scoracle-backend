BEGIN READ ONLY;
SELECT row_to_json(a) FROM (
 SELECT t.name, c.entity_type, c.entity_id, c.sport
 FROM coverage_assignments c JOIN teams t ON t.id=c.entity_id AND t.sport=c.sport
 WHERE c.edition_date=(SELECT max(edition_date) FROM coverage_editions WHERE frozen_at IS NOT NULL)
 AND c.stage='narratives' AND c.outcome='pending' AND c.entity_type='team'
 ORDER BY cardinality(c.spectrum_ids) DESC,c.entity_id LIMIT 10
) a;
ROLLBACK;

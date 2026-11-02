BEGIN READ ONLY;
WITH packages AS (
 SELECT a.entity_type,a.entity_id,a.sport,a.edition_date,
 count(*) n, bool_or(x.clean_text ~* '(den(y|ies|ied)|not interested|no intention|rules out)') denial,
 json_agg(json_build_object('id',s.article_id,'headline',x.clean_text,'publisher',s.source->>'source') ORDER BY x.id) reports
 FROM coverage_assignments a JOIN classifier_spectra x ON x.id=ANY(a.spectrum_ids) JOIN classifier_sources s ON s.id=x.source_id
 WHERE a.edition_date=(SELECT max(edition_date) FROM coverage_editions WHERE frozen_at IS NOT NULL)
 AND a.stage='transfers' AND a.sport IN ('FOOTBALL','NBA','NFL')
 AND (a.entity_type,a.entity_id,a.sport) NOT IN (('team',8,'FOOTBALL'),('team',2,'NBA'),('team',22,'NFL'),('player',129602,'FOOTBALL'),('player',237,'NBA'),('player',744,'NFL'),('person',959,'NFL'),('person',23,'FOOTBALL'),('person',86,'FOOTBALL'),('player',37,'NBA'),('player',60,'NFL'))
 GROUP BY a.entity_type,a.entity_id,a.sport,a.edition_date HAVING count(*) BETWEEN 1 AND 12
), ranked AS (SELECT *,row_number() OVER(PARTITION BY sport ORDER BY denial DESC,n DESC,entity_type,entity_id) rn FROM packages)
SELECT row_to_json(ranked) FROM ranked WHERE rn<=18 ORDER BY sport,rn;
ROLLBACK;

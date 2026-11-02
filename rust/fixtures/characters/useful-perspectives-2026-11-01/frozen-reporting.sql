BEGIN READ ONLY;
SELECT json_build_object('entity_type',a.entity_type,'entity_id',a.entity_id,'sport',a.sport,'stage',a.stage,'edition_date',a.edition_date,'frozen_at',e.frozen_at,
'reports',(SELECT json_agg(json_build_object('classification_id',x.id,'article_id',s.article_id,'headline',s.source->>'title','context',x.clean_text,'source',s.source->>'source',
'published_at_epoch',floor(extract(epoch FROM (s.source->>'published_at')::timestamptz))::bigint,
'classifier_world',json_build_object('evidence_scope','headline','url',s.source->'url','spectrum',x.spectrum,'gate',x.gate,'target_identity_resolved',EXISTS(SELECT 1 FROM jsonb_array_elements(s.source->'identity_candidates') c WHERE c->>'entity_type'=a.entity_type AND (c->>'entity_id')::bigint=a.entity_id)),
'identity_candidates',s.source->'identity_candidates') ORDER BY (s.source->>'published_at')::timestamptz DESC,x.id DESC)
FROM classifier_spectra x JOIN classifier_sources s ON s.id=x.source_id WHERE x.id=ANY(a.spectrum_ids)))
FROM coverage_assignments a JOIN coverage_editions e USING(edition_date)
WHERE a.edition_date=(SELECT max(edition_date) FROM coverage_editions WHERE frozen_at IS NOT NULL)
AND (a.entity_type,a.entity_id,a.sport) IN (('team',8,'FOOTBALL'),('team',2,'NBA'),('team',22,'NFL'),('player',129602,'FOOTBALL'),('player',237,'NBA'),('player',744,'NFL'),('person',959,'NFL'),('person',23,'FOOTBALL'),('person',86,'FOOTBALL'),('player',37,'NBA'),('player',60,'NFL'))
AND stage IN ('narratives','vibe','transfers') ORDER BY a.sport,a.entity_type,a.entity_id,a.stage;
ROLLBACK;

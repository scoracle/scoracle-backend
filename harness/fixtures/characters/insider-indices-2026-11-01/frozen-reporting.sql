BEGIN READ ONLY;
SELECT json_build_object('entity_type',a.entity_type,'entity_id',a.entity_id,'sport',a.sport,'stage',a.stage,'edition_date',a.edition_date,'frozen_at',e.frozen_at,
'reports',(SELECT json_agg(json_build_object('classification_id',x.id,'article_id',s.article_id,'headline',s.source->>'title','context',x.clean_text,'source',s.source->>'source',
'published_at_epoch',floor(extract(epoch FROM (s.source->>'published_at')::timestamptz))::bigint,
'classifier_world',json_build_object('evidence_scope','headline','url',s.source->'url','spectrum',x.spectrum,'gate',x.gate,'target_identity_resolved',EXISTS(SELECT 1 FROM jsonb_array_elements(s.source->'identity_candidates') c WHERE c->>'entity_type'=a.entity_type AND (c->>'entity_id')::bigint=a.entity_id)),
'identity_candidates',s.source->'identity_candidates') ORDER BY (s.source->>'published_at')::timestamptz DESC,x.id DESC)
FROM classifier_spectra x JOIN classifier_sources s ON s.id=x.source_id WHERE x.id=ANY(a.spectrum_ids)))
FROM coverage_assignments a JOIN coverage_editions e USING(edition_date)
WHERE a.edition_date=(SELECT max(edition_date) FROM coverage_editions WHERE frozen_at IS NOT NULL)
AND (a.entity_type,a.entity_id,a.sport) IN (('team',1,'FOOTBALL'),('team',37,'FOOTBALL'),('player',37550443,'FOOTBALL'),('person',107,'FOOTBALL'),('team',7,'NBA'),('player',322,'NBA'),('player',185,'NBA'),('player',17896040,'NBA'),('player',45,'NFL'),('player',83946,'NFL'),('player',278370,'NFL'),('person',8,'NFL'))
AND stage='transfers' ORDER BY a.sport,a.entity_type,a.entity_id,a.stage;
ROLLBACK;

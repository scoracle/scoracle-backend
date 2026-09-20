SELECT r.id, r.created_at::text AS observed_at, floor(extract(epoch FROM r.created_at))::bigint AS observed_unix,
 jsonb_build_object('relationship',r.predicate,'team',t.name,'state',r.state,'valid_from',r.valid_from,'valid_to',r.valid_to,'source_document',r.source_document_id) AS data
FROM public.entity_relationships r
LEFT JOIN public.teams t ON t.id=r.object_entity_id AND t.sport=r.object_sport AND r.object_entity_type='team'
WHERE r.subject_entity_type=$1 AND r.subject_entity_id=$2 AND r.subject_sport=$3
 AND r.state IN ('active','conflicted') AND (r.valid_from IS NULL OR r.valid_from<=now())
 AND (r.valid_to IS NULL OR r.valid_to>now())
 AND r.predicate IN ('coach_of','player_of','member_of','manager_of')
ORDER BY r.predicate,r.object_entity_id,r.id

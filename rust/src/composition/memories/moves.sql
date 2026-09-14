SELECT concat(g.ledger,'/',g.ref_id) AS source_key,g.applied_at::text AS observed_at,
 floor(extract(epoch FROM g.applied_at))::bigint AS observed_unix,
 jsonb_build_object('player',p.name,'joined',t.name,'status','applied and unreverted identity move') AS data
FROM public.transfer_ground_truth g JOIN public.players p ON p.id=g.player_id AND p.sport=g.sport
JOIN public.teams t ON t.id=g.team_id AND t.sport=g.sport
WHERE g.sport=$3 AND (($1='player' AND g.player_id=$2) OR ($1='team' AND g.team_id=$2))
 AND g.applied_at BETWEEN now()-INTERVAL '1 year' AND now()
ORDER BY (g.team_id=$4) DESC NULLS LAST,g.applied_at DESC,g.ref_id DESC LIMIT 2

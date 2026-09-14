-- Current-season statistical affiliations are observations, not an override of
-- the canonical projection. Keep every distinct team so disagreement stays visible.
SELECT DISTINCT ON (s.team_id) s.team_id,s.season,s.league_id,
 s.updated_at::text AS observed_at,floor(extract(epoch FROM s.updated_at))::bigint AS observed_unix,
 jsonb_build_object('played_for',t.name,'season',s.season,'league_id',s.league_id,
 'recorded_sample',(SELECT jsonb_object_agg(k,v) FROM jsonb_each(s.stats) e(k,v) WHERE k IN ('appearances','games_played','minutes_played')),
 'coverage','stored statistical snapshot; not proof of complete season participation or current employment') AS data
FROM public.player_stats s JOIN public.teams t ON t.id=s.team_id AND t.sport=s.sport
WHERE s.player_id=$1 AND s.sport=$2 AND s.season=$3
ORDER BY s.team_id,s.updated_at DESC,s.league_id

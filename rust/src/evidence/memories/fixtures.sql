WITH team AS (
 SELECT CASE $1 WHEN 'team' THEN $2 WHEN 'player' THEN (SELECT team_id FROM public.player_current_identity WHERE player_id=$2 AND sport=$3)
 WHEN 'person' THEN (SELECT team_id FROM public.persons WHERE id=$2 AND sport=$3) END AS id
), selected AS (
 SELECT f.* FROM public.fixtures f,team t WHERE f.sport=$3 AND t.id IN (f.home_team_id,f.away_team_id)
 AND f.start_time BETWEEN now()-INTERVAL '7 days' AND now()+INTERVAL '14 days'
 AND COALESCE(f.meta->>'needs_verification','false') <> 'true'
 ORDER BY abs(extract(epoch FROM f.start_time-now())),f.id LIMIT 2
)
SELECT f.id,f.updated_at::text AS observed_at,floor(extract(epoch FROM f.updated_at))::bigint AS observed_unix,
 jsonb_build_object('competition',l.name,'season',f.season,'scheduled_at',f.start_time,'status',f.status,'home',h.name,'away',a.name) AS data
FROM selected f LEFT JOIN public.leagues l ON l.id=f.league_id AND l.sport=f.sport
JOIN public.teams h ON h.id=f.home_team_id AND h.sport=f.sport JOIN public.teams a ON a.id=f.away_team_id AND a.sport=f.sport
ORDER BY f.start_time,f.id

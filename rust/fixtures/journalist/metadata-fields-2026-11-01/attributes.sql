SELECT json_build_object('name',t.name,'record',json_strip_nulls(json_build_object(
'name',t.name,'league_name',l.name,'league_country',CASE WHEN l.name IS NOT NULL THEN l.country END,
'conference',CASE WHEN l.name IS NULL THEN t.conference END,
'division',CASE WHEN l.name IS NULL AND t.conference IS NOT NULL THEN t.division END,
'venue_name',CASE WHEN t.venue_name IS NOT NULL AND t.city IS NOT NULL THEN t.venue_name END,
'city',CASE WHEN t.venue_name IS NOT NULL AND t.city IS NOT NULL THEN t.city END,
'coach_name_on_record',(SELECT pe.full_name FROM public.persons pe WHERE pe.sport=t.sport AND pe.team_id=t.id AND pe.kind='coach' ORDER BY pe.created_at DESC LIMIT 1))))
FROM public.teams t LEFT JOIN public.leagues l ON l.id=t.league_id AND l.sport=t.sport
WHERE (t.id,t.sport) IN ((113,'FOOTBALL'),(24,'NFL'),(13,'NBA'),(8,'FOOTBALL'),(2,'NBA'),(22,'NFL')) ORDER BY t.sport,t.name;

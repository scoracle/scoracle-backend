SELECT json_build_object('name',name,'entity_type','team','entity_id',id,'sport',sport)
FROM public.teams
WHERE (name='Liverpool' AND sport='FOOTBALL')
OR (name='Green Bay Packers' AND sport='NFL')
OR (name='Boston Celtics' AND sport='NBA')
ORDER BY sport,name;

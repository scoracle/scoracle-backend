SELECT id,name,meta->>'gender' AS gender FROM public.teams WHERE (sport='FOOTBALL' AND id=8) OR (sport='NBA' AND id=2) OR (sport='NFL' AND id=22);

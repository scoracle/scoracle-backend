-- Two snapshots from the same competition. No cross-skill percentile subtraction
-- and no assumption that a stored season row is complete participation.
WITH current_row AS (
 SELECT s.* FROM public.{table} s WHERE s.sport=$1 AND s.{id}=$2 AND s.season=$3
 ORDER BY COALESCE(s.league_id,0)=0 DESC,jsonb_array_length(COALESCE(s.rating_breakdown,'[]'::jsonb)) DESC,s.league_id LIMIT 1
), prior_row AS (
 SELECT s.* FROM public.{table} s WHERE s.sport=$1 AND s.{id}=$2 AND s.season<$3
 AND (NOT EXISTS(SELECT 1 FROM current_row) OR s.league_id=(SELECT league_id FROM current_row))
 ORDER BY s.season DESC,COALESCE(s.league_id,0)=0 DESC,jsonb_array_length(COALESCE(s.rating_breakdown,'[]'::jsonb)) DESC,s.league_id LIMIT 1
), snapshots AS (
 SELECT * FROM prior_row UNION ALL SELECT * FROM current_row
)
SELECT s.season,s.league_id,s.updated_at::text AS observed_at,
 floor(extract(epoch FROM s.updated_at))::bigint AS observed_unix,
 jsonb_build_object('season',s.season,'league_id',s.league_id,'played_for',t.name,
 'recorded_sample',(SELECT jsonb_object_agg(k,v) FROM jsonb_each(s.stats) e(k,v) WHERE k IN ('appearances','games_played','matches_played','minutes_played')),
 'measurements',(SELECT jsonb_agg(jsonb_build_object('measure',d.key_name,'label',d.display_name,'value',s.stats->d.key_name,'unit',d.unit) ORDER BY d.sort_order,d.key_name)
 FROM public.stat_definitions d WHERE d.sport=$1 AND d.entity_type=$4
 AND d.key_name IN ('goals','assists','shots_on_target','chances_created','key_passes','expected_goals','expected_assists','points','rebounds','passing_yards','rushing_yards','receiving_yards','touchdowns')
 AND (s.stats ? d.key_name OR EXISTS(SELECT 1 FROM snapshots other WHERE other.stats ? d.key_name))),
 'coverage','stored snapshot; completeness unknown') AS data
FROM snapshots s LEFT JOIN public.teams t ON t.id=s.team_id AND t.sport=s.sport
ORDER BY s.season

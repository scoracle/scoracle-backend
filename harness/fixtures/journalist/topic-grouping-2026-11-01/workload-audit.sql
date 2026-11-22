BEGIN READ ONLY;
SELECT json_build_object('kind','snapshot','captured_at',clock_timestamp());
SELECT json_build_object('kind','edition','data',row_to_json(e)) FROM (SELECT edition_date,status,cutoff_at,frozen_at,finished_at FROM coverage_editions ORDER BY edition_date DESC LIMIT 3) e;
SELECT json_build_object('kind','latest_frozen_assignments','data',row_to_json(a)) FROM (
 SELECT edition_date,stage,outcome,resolution,count(*) AS assignments,
 sum(cardinality(spectrum_ids)) AS source_memberships,
 percentile_cont(0.5) WITHIN GROUP(ORDER BY cardinality(spectrum_ids)) AS median_sources,
 max(cardinality(spectrum_ids)) AS max_sources
 FROM coverage_assignments
 WHERE edition_date=(SELECT max(edition_date) FROM coverage_editions WHERE frozen_at IS NOT NULL)
 GROUP BY edition_date,stage,outcome,resolution ORDER BY stage,outcome,resolution
) a;
SELECT json_build_object('kind','outstanding_work_not_a_daily_call_forecast','data',row_to_json(w)) FROM (
 SELECT stage,status,count(*) AS jobs,sum(attempts) AS attempts,max(attempts) AS max_attempts
 FROM pipeline_work WHERE stage IN ('narratives','vibe','rating','transfers','momentum','sigil') GROUP BY stage,status ORDER BY stage,status
) w;
ROLLBACK;

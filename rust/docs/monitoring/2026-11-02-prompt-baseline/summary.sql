WITH products AS MATERIALIZED (
 SELECT 'narratives' stage,entity_type,entity_id,sport,prompt_version,model_version,reporting_date,
 coverage_character_visible(candidate_edition,'narratives',entity_type,entity_id,sport) qualified
 FROM news_summaries WHERE generated_at>=:'since'::timestamptz
 UNION ALL SELECT 'vibe',entity_type,entity_id,sport,prompt_version,model_version,reporting_date,
 coverage_character_visible(candidate_edition,'vibe',entity_type,entity_id,sport)
 FROM vibe_scores WHERE generated_at>=:'since'::timestamptz
 UNION ALL SELECT 'transfers',entity_type,entity_id,sport,prompt_version,model_version,reporting_date,
 coverage_character_visible(candidate_edition,'transfers',entity_type,entity_id,sport)
 FROM insider_scores WHERE generated_at>=:'since'::timestamptz
)
SELECT json_build_object('captured_at',clock_timestamp(),
 'schedule',(SELECT row_to_json(s) FROM coverage_schedule s),
 'publication',(SELECT row_to_json(p) FROM coverage_publication p),
 'article_cap',headline_entity_cap(),
 'queue',(SELECT json_agg(q) FROM (SELECT stage,status,count(*) count,max(attempts) max_attempts FROM pipeline_work WHERE stage IN ('classifier','narratives','vibe','transfers') GROUP BY stage,status) q),
 'active_assignments',(SELECT json_agg(q) FROM (SELECT stage,outcome,resolution,count(*) count FROM coverage_assignments WHERE edition_date=(SELECT edition_date FROM coverage_schedule) GROUP BY stage,outcome,resolution) q),
 'products',(SELECT json_agg(q) FROM (SELECT stage,entity_type,sport,prompt_version,model_version,reporting_date,count(*) generated_rows,count(DISTINCT entity_id) distinct_targets,count(*) FILTER(WHERE qualified) qualified_rows FROM products GROUP BY stage,entity_type,sport,prompt_version,model_version,reporting_date) q),
 'delivery_states',(SELECT json_agg(q) FROM (SELECT character,status,reason,count(*) rows FROM classifier_spectrum_deliveries WHERE updated_at>=:'since'::timestamptz AND character IN ('journalist','insider','influencer') GROUP BY character,status,reason) q));

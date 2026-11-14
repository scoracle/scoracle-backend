BEGIN;
SET LOCAL lock_timeout='2s';
SET LOCAL statement_timeout='15s';
CREATE OR REPLACE VIEW public.qualified_news_summaries AS SELECT id,entity_type,entity_id,sport,trigger_type,trigger_payload,body,impact,impact_components,input_news_ids,model_version,prompt_version,generated_at,narrative_title,narrative_updated_at,source_count,source_names,source_latest_at,source_oldest_at,trajectory,trajectory_components,input_hash,card_score,card_score_prev,storyline_id,headline,week_season,week_no,reporting_date,candidate_edition FROM public.news_summaries WHERE NOT (SELECT enabled FROM public.coverage_schedule) UNION ALL SELECT * FROM (SELECT id,
    entity_type,
    entity_id,
    sport,
    trigger_type,
    trigger_payload,
    body,
    impact,
    impact_components,
    input_news_ids,
    model_version,
    prompt_version,
    generated_at,
    narrative_title,
    narrative_updated_at,
    source_count,
    source_names,
    source_latest_at,
    source_oldest_at,
    trajectory,
    trajectory_components,
    input_hash,
    card_score,
    card_score_prev,
    storyline_id,
    headline,
    week_season,
    week_no,
    reporting_date,
    candidate_edition
   FROM news_summaries p
  WHERE (NOT ( SELECT coverage_schedule.enabled
           FROM coverage_schedule) OR published_edition() IS NULL) AND candidate_edition IS NULL OR candidate_edition <= published_edition() AND (EXISTS ( SELECT 1
           FROM coverage_editions e
          WHERE e.edition_date = p.candidate_edition AND e.status = 'complete'::text)) AND coverage_carry_allowed(candidate_edition, 'narratives'::text, entity_type, entity_id::bigint, sport) OR coverage_character_visible(candidate_edition, 'narratives'::text, entity_type, entity_id::bigint, sport)) old_view WHERE (SELECT enabled FROM public.coverage_schedule);
CREATE OR REPLACE VIEW public.qualified_vibe_scores AS SELECT id,entity_type,entity_id,sport,trigger_type,trigger_payload,input_news_ids,model_version,prompt_version,generated_at,sentiment,prompt,input_hash,hook,week_season,week_no,reporting_start,reporting_end,evidence_cutoff,scoring_version,source_references,reporting_date,candidate_edition FROM public.vibe_scores WHERE NOT (SELECT enabled FROM public.coverage_schedule) UNION ALL SELECT * FROM (SELECT id,
    entity_type,
    entity_id,
    sport,
    trigger_type,
    trigger_payload,
    input_news_ids,
    model_version,
    prompt_version,
    generated_at,
    sentiment,
    prompt,
    input_hash,
    hook,
    week_season,
    week_no,
    reporting_start,
    reporting_end,
    evidence_cutoff,
    scoring_version,
    source_references,
    reporting_date,
    candidate_edition
   FROM vibe_scores p
  WHERE (NOT ( SELECT coverage_schedule.enabled
           FROM coverage_schedule) OR published_edition() IS NULL) AND candidate_edition IS NULL OR candidate_edition <= published_edition() AND (EXISTS ( SELECT 1
           FROM coverage_editions e
          WHERE e.edition_date = p.candidate_edition AND e.status = 'complete'::text)) AND coverage_carry_allowed(candidate_edition, 'vibe'::text, entity_type, entity_id::bigint, sport) OR coverage_character_visible(candidate_edition, 'vibe'::text, entity_type, entity_id::bigint, sport)) old_view WHERE (SELECT enabled FROM public.coverage_schedule);
CREATE OR REPLACE VIEW public.qualified_insider_scores AS SELECT id,sport,entity_type,entity_id,score,previous_score,read,model_version,prompt_version,input_hash,generated_at,headline,week_season,week_no,reporting_date,candidate_edition FROM public.insider_scores WHERE NOT (SELECT enabled FROM public.coverage_schedule) UNION ALL SELECT * FROM (SELECT id,
    sport,
    entity_type,
    entity_id,
    score,
    previous_score,
    read,
    model_version,
    prompt_version,
    input_hash,
    generated_at,
    headline,
    week_season,
    week_no,
    reporting_date,
    candidate_edition
   FROM insider_scores p
  WHERE (NOT ( SELECT coverage_schedule.enabled
           FROM coverage_schedule) OR published_edition() IS NULL) AND candidate_edition IS NULL OR candidate_edition <= published_edition() AND (EXISTS ( SELECT 1
           FROM coverage_editions e
          WHERE e.edition_date = p.candidate_edition AND e.status = 'complete'::text)) AND coverage_carry_allowed(candidate_edition, 'transfers'::text, entity_type, entity_id::bigint, sport) OR coverage_character_visible(candidate_edition, 'transfers'::text, entity_type, entity_id::bigint, sport)) old_view WHERE (SELECT enabled FROM public.coverage_schedule);
CREATE OR REPLACE VIEW public.qualified_transfer_rumors AS SELECT id,team_id,player_id,sport,trigger_type,trigger_payload,heat,heat_components,is_rumor,direction,stage,model_summary,source_attribution,confidence,input_news_ids,model_version,prompt_version,generated_at,rumor_updated_at,source_count,source_names,source_latest_at,source_oldest_at,trajectory,trajectory_components,input_hash,subject_type,week_season,week_no,reporting_date,candidate_entity_type,candidate_entity_id,candidate_edition FROM public.transfer_rumors WHERE NOT (SELECT enabled FROM public.coverage_schedule) UNION ALL SELECT * FROM (SELECT id,
    team_id,
    player_id,
    sport,
    trigger_type,
    trigger_payload,
    heat,
    heat_components,
    is_rumor,
    direction,
    stage,
    model_summary,
    source_attribution,
    confidence,
    input_news_ids,
    model_version,
    prompt_version,
    generated_at,
    rumor_updated_at,
    source_count,
    source_names,
    source_latest_at,
    source_oldest_at,
    trajectory,
    trajectory_components,
    input_hash,
    subject_type,
    week_season,
    week_no,
    reporting_date,
    candidate_entity_type,
    candidate_entity_id,
    candidate_edition
   FROM transfer_rumors p
  WHERE (NOT ( SELECT coverage_schedule.enabled
           FROM coverage_schedule) OR published_edition() IS NULL) AND candidate_edition IS NULL OR candidate_edition <= published_edition() AND (EXISTS ( SELECT 1
           FROM coverage_editions e
          WHERE e.edition_date = p.candidate_edition AND e.status = 'complete'::text)) AND coverage_carry_allowed(candidate_edition, 'transfers'::text, candidate_entity_type, candidate_entity_id, sport) OR coverage_character_visible(candidate_edition, 'transfers'::text, candidate_entity_type, candidate_entity_id, sport)) old_view WHERE (SELECT enabled FROM public.coverage_schedule);
CREATE OR REPLACE FUNCTION public.published_edition() RETURNS date LANGUAGE sql STABLE AS $$ SELECT CASE WHEN (SELECT enabled FROM public.coverage_schedule) THEN COALESCE(NULLIF(current_setting('scoracle.published_edition',true),'')::date,edition_date) END FROM public.coverage_publication $$;
INSERT INTO schema_migrations(version) VALUES('333_continuous_corpus_visibility') ON CONFLICT DO NOTHING;
COMMIT;

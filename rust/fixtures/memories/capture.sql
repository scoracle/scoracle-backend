-- Diagnostic source capture only. No model calls, writes, or derived-stat rebuilds.
-- psql -X -qAt -v ON_ERROR_STOP=1 -f fixtures/memories/capture.sql
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL statement_timeout = '20s';
SET LOCAL TIME ZONE 'UTC';
WITH records AS (
  SELECT 'morgan' AS case_name, 'identity' AS kind, 'players/player_current_identity' AS source,
    'FOOTBALL/player/4592198' AS source_key, i.source_updated_at AS observed_at,
    jsonb_build_object('name',p.name,'role',i.position,'team_id',i.team_id,'team',t.name,
      'league_id',i.league_id,'record_source',i.source) AS data
  FROM players p JOIN player_current_identity i ON i.player_id=p.id AND i.sport=p.sport
  LEFT JOIN teams t ON t.id=i.team_id AND t.sport=i.sport
  WHERE p.id=4592198 AND p.sport='FOOTBALL'
  UNION ALL
  SELECT 'morgan','statistics','player_stats',concat(s.sport,'/',s.player_id,'/',s.season,'/',s.league_id),s.updated_at,
    jsonb_build_object('season',s.season,'league_id',s.league_id,'team_id',s.team_id,
      'basis','season totals','appearances',s.stats->'appearances','minutes',s.stats->'minutes_played',
      'goals',s.stats->'goals','assists',s.stats->'assists','expected_goals',s.stats->'expected_goals',
      'expected_assists',s.stats->'expected_assists','shots_on_target',s.stats->'shots_on_target',
      'stored_rating_score',s.rating_score,
      'measurement_contract_present',NOT EXISTS (
        SELECT 1 FROM jsonb_array_elements(s.rating_breakdown) d WHERE NOT d ? 'measure'))
  FROM player_stats s WHERE s.player_id=4592198 AND s.sport='FOOTBALL' AND s.season IN (2025,2026)
  UNION ALL
  SELECT 'morgan','affiliation_observation','player_team_history',h.id::text,h.created_at,
    jsonb_build_object('team_id',h.team_id,'team',t.name,'season',h.season,'is_current',h.is_current,
      'observed_from',h.valid_from,'observed_until',h.valid_until,
      'date_semantics','box-score ingestion boundaries, not signing/leaving dates')
  FROM player_team_history h JOIN teams t ON t.id=h.team_id AND t.sport=h.sport
  WHERE h.player_id=4592198 AND h.sport='FOOTBALL'
  UNION ALL
  SELECT 'iraola','identity','persons','FOOTBALL/person/11',NULL,
    jsonb_build_object('name',p.full_name,'role',p.kind,'team_id',p.team_id,'team',t.name,
      'checked_at',p.meta->'affiliation_checked_at','check_outcome',p.meta->'affiliation_check_outcome',
      'affiliation_note',p.meta->'team_affiliation_note')
  FROM persons p LEFT JOIN teams t ON t.id=p.team_id AND t.sport=p.sport WHERE p.id=11 AND p.sport='FOOTBALL'
  UNION ALL
  SELECT 'iraola','role_fact','entity_facts',f.id::text,f.created_at,
    jsonb_build_object('type',f.fact_type,'value',f.value_text,'state',f.state,'valid_from',f.valid_from,
      'valid_to',f.valid_to,'source_document_id',f.source_document_id)
  FROM entity_facts f WHERE f.entity_type='person' AND f.entity_id=11 AND f.sport='FOOTBALL' AND f.fact_type='role'
  UNION ALL
  SELECT 'iraola','role_relationship','entity_relationships',r.id::text,r.created_at,
    jsonb_build_object('predicate',r.predicate,'team_id',r.object_entity_id,'team',t.name,'state',r.state,
      'valid_from',r.valid_from,'valid_to',r.valid_to,'source_document_id',r.source_document_id)
  FROM entity_relationships r LEFT JOIN teams t ON t.id=r.object_entity_id AND t.sport=r.object_sport
  WHERE r.subject_entity_type='person' AND r.subject_entity_id=11 AND r.subject_sport='FOOTBALL'
  UNION ALL
  -- Retain the complete relevant Wikidata statement, including date precision and references.
  SELECT 'iraola','career_statement','source_documents',concat(sd.id,'/P54/',claim->>'id'),sd.fetched_at,
    jsonb_build_object('url',sd.url,'property','P54','statement',claim)
  FROM source_documents sd CROSS JOIN LATERAL
    jsonb_array_elements(sd.retained_excerpt::jsonb#>'{entities,Q317216,claims,P54}') claim
  WHERE sd.id=66096 AND claim#>>'{mainsnak,datavalue,value,id}'='Q8687'
  UNION ALL
  SELECT 'iraola','news','news_articles',n.id::text,n.fetched_at,
    jsonb_build_object('title',n.title,'description',n.description,'source',n.source,'url',n.url,
      'published_at',n.published_at,'duplicate_of',n.duplicate_of)
  FROM news_articles n WHERE n.id IN (145830,585115,585118)
  UNION ALL
  SELECT 'lions','identity','teams','NFL/team/25',t.updated_at,
    jsonb_build_object('name',t.name,'league_id',t.league_id)
  FROM teams t WHERE t.sport='NFL' AND t.id=25
  UNION ALL
  SELECT 'lions','fixture','fixtures',f.id::text,f.updated_at,
    jsonb_build_object('sport',f.sport,'season',f.season,'league_id',f.league_id,'round',f.round,
      'start_time',f.start_time,'status',f.status,'home_team',ht.name,'away_team',at.name,
      'home_score',f.home_score,'away_score',f.away_score,'meta',f.meta)
  FROM fixtures f JOIN teams ht ON ht.id=f.home_team_id AND ht.sport=f.sport
  JOIN teams at ON at.id=f.away_team_id AND at.sport=f.sport
  WHERE f.sport='NFL' AND f.season=2026 AND f.league_id=0
    AND (f.home_team_id=25 OR f.away_team_id=25) AND f.round IN ('Week 1','Week 2')
  UNION ALL
  SELECT c.case_name,'reporting_clock','sports/season_weeks',concat(s.id,'/',w.season,'/',w.week_no),NULL,
    jsonb_build_object('sport',s.id,'season',s.current_season,'clock_league_id',s.clock_league_id,
      'reporting_week',w.week_no,'starts_at',w.starts_at,'ends_at',w.ends_at)
  FROM (VALUES ('morgan','FOOTBALL'),('iraola','FOOTBALL'),('lions','NFL')) c(case_name,sport)
  JOIN sports s ON s.id=c.sport LEFT JOIN season_weeks w ON w.sport=s.id
    AND now()>=w.starts_at AND now()<w.ends_at
  UNION ALL
  SELECT 'morgan','schedule_coverage','fixtures','FOOTBALL/8/2026',max(f.updated_at),
    jsonb_build_object('league_id',8,'season',2026,'rows',count(*),'rounds',array_agg(DISTINCT f.round),
      'first_stored_start',min(f.start_time),'last_stored_start',max(f.start_time),
      'unverified_rows',count(*) FILTER (WHERE f.meta->>'needs_verification'='true'))
  FROM fixtures f WHERE f.sport='FOOTBALL' AND f.season=2026 AND f.league_id=8
)
SELECT jsonb_build_object('captured_at',now(),'captured_unix',extract(epoch FROM now())::bigint,
  'schema_version',(SELECT max(version) FROM schema_migrations),
  'records',jsonb_agg(jsonb_build_object('case',case_name,'kind',kind,'source',source,'source_key',source_key,
    'observed_at',observed_at,'observed_unix',extract(epoch FROM observed_at)::bigint,'data',data)
    ORDER BY case_name,kind,source_key)) FROM records;
ROLLBACK;

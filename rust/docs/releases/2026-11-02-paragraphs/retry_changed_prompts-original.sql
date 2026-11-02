-- Historical failed routing version; DO NOT RUN. See fix_retry_routing.sql.
-- One-time, explicit recovery of failed model work after the paragraph prompt release.
-- Preview rolls back; apply requires a private backup path. No existing product is replaced.
\set ON_ERROR_STOP on
\if :{?apply}
\else
 \set apply false
\endif
BEGIN;
SET LOCAL lock_timeout='3s';
SET LOCAL statement_timeout='600s';
SELECT pg_advisory_xact_lock(1935896421,1);
CREATE TEMP TABLE retry_keys ON COMMIT DROP AS
 SELECT * FROM jsonb_to_recordset(:'assignment_scope'::jsonb) AS k(stage text,entity_type text,entity_id bigint,sport text);
ALTER TABLE retry_keys ADD PRIMARY KEY(stage,entity_type,entity_id,sport);
CREATE TEMP TABLE retry_parameters ON COMMIT DROP AS SELECT :'edition_date'::date edition_date;
DO $$ BEGIN
 IF (SELECT count(*) FROM retry_keys) NOT BETWEEN 1 AND 300 OR EXISTS(SELECT 1 FROM retry_keys WHERE stage NOT IN ('narratives','vibe','transfers')) THEN RAISE EXCEPTION 'Explicit bounded news-character scope required'; END IF;
 IF NOT EXISTS(SELECT 1 FROM coverage_schedule c JOIN retry_parameters p USING(edition_date) WHERE enabled AND phase='characters') THEN RAISE EXCEPTION 'Frozen edition changed'; END IF;
 IF EXISTS(SELECT 1 FROM pipeline_work WHERE status='running' AND coverage_is_character(stage)) THEN RAISE EXCEPTION 'Drain character claims first'; END IF;
 IF EXISTS(SELECT 1 FROM retry_keys k CROSS JOIN retry_parameters p LEFT JOIN coverage_assignments a USING(edition_date,stage,entity_type,entity_id,sport)
  LEFT JOIN pipeline_work w USING(stage,entity_type,entity_id,sport)
  WHERE a.stage IS NULL OR a.outcome<>'failed' OR a.prepared_input IS NOT NULL OR (w.stage IS NOT NULL AND (w.status NOT IN ('pending','failed') OR w.input_version IS DISTINCT FROM a.input_version))) THEN RAISE EXCEPTION 'Assignment state changed'; END IF;
 IF EXISTS(SELECT 1 FROM retry_keys k CROSS JOIN retry_parameters p WHERE
  (k.stage='narratives' AND EXISTS(SELECT 1 FROM news_summaries n WHERE n.candidate_edition=p.edition_date AND n.entity_type=k.entity_type AND n.entity_id=k.entity_id AND n.sport=k.sport)) OR
  (k.stage='vibe' AND EXISTS(SELECT 1 FROM vibe_scores n WHERE n.candidate_edition=p.edition_date AND n.entity_type=k.entity_type AND n.entity_id=k.entity_id AND n.sport=k.sport)) OR
  (k.stage='transfers' AND EXISTS(SELECT 1 FROM insider_scores n WHERE n.candidate_edition=p.edition_date AND n.entity_type=k.entity_type AND n.entity_id=k.entity_id AND n.sport=k.sport))) THEN RAISE EXCEPTION 'Existing candidate product'; END IF;
END $$;
CREATE TEMP TABLE retry_assignments ON COMMIT DROP AS
 SELECT a.*,CASE a.stage WHEN 'narratives' THEN 'journalist' WHEN 'vibe' THEN 'influencer' ELSE 'insider' END AS character_key
 FROM coverage_assignments a JOIN retry_keys k USING(stage,entity_type,entity_id,sport) JOIN retry_parameters p USING(edition_date);
CREATE TEMP TABLE retry_sources ON COMMIT DROP AS
 SELECT DISTINCT s.id,COALESCE(s.discovery_version=classifier_discovery_version(s.article_id,s.sport),false)
 AND EXISTS(SELECT 1 FROM classifier_source_heads h WHERE h.article_id=s.article_id AND h.sport=s.sport AND h.source_id=s.id) AS current_source
 FROM retry_assignments a JOIN classifier_spectra x ON x.id=ANY(a.spectrum_ids) JOIN classifier_sources s ON s.id=x.source_id;
-- Keep every source/identity hold, stale packet and previously recovered packet untouched.
DELETE FROM retry_assignments a WHERE a.input_version LIKE 'paragraphs-20261102:%' OR NOT EXISTS(
 SELECT 1 FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids) AND d.character=a.character_key AND d.status='held'
 AND d.reason IN ('model_reply_rejected','character_assignment_failed','incomplete_reporting_day')) OR EXISTS(
 SELECT 1 FROM classifier_spectra x JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id AND d.character=a.character_key
 JOIN classifier_sources s ON s.id=x.source_id JOIN retry_sources c ON c.id=s.id WHERE x.id=ANY(a.spectrum_ids) AND d.status IN ('held','pending') AND
 (NOT c.current_source OR (d.status='held' AND (d.reason IS NULL OR d.reason NOT IN ('model_reply_rejected','character_assignment_failed','incomplete_reporting_day'))) OR
 NOT EXISTS(SELECT 1 FROM jsonb_array_elements(s.source->'identity_candidates') i WHERE i->>'entity_type'=a.entity_type AND i->>'entity_id'=a.entity_id::text)));
SELECT stage,count(*) AS retryable_current_packets FROM retry_assignments GROUP BY stage;
\if :apply
 \if :{?backup_file}
 \else
  \echo 'Apply requires backup_file'
  \quit 1
 \endif
 \pset format unaligned
 \pset tuples_only on
 \o :backup_file
 SELECT jsonb_build_object('table','coverage_assignments','row',to_jsonb(a)-'character_key') FROM retry_assignments a
 UNION ALL SELECT jsonb_build_object('table','pipeline_work','row',to_jsonb(w)) FROM pipeline_work w JOIN retry_assignments a USING(stage,entity_type,entity_id,sport)
 UNION ALL SELECT jsonb_build_object('table','classifier_spectrum_deliveries','row',to_jsonb(d)) FROM classifier_spectrum_deliveries d JOIN retry_assignments a ON d.spectrum_id=ANY(a.spectrum_ids) AND d.character=a.character_key;
 \o
 \pset tuples_only off
 \pset format aligned
\endif
UPDATE classifier_spectrum_deliveries d SET status='pending',reason=NULL,updated_at=clock_timestamp(),
 product_ref=jsonb_build_object('recovery_policy','paragraphs-20261102','previous_delivery',to_jsonb(d))
 FROM retry_assignments a WHERE d.spectrum_id=ANY(a.spectrum_ids) AND d.character=a.character_key AND d.status='held';
SELECT coverage_select_headlines(:'spam_spectra'::bigint[],COALESCE((SELECT jsonb_agg(jsonb_build_object('stage',stage,'entity_type',entity_type,'entity_id',entity_id,'sport',sport)) FROM retry_assignments),'[]'::jsonb));
UPDATE coverage_assignments a SET outcome='pending',resolution=NULL,finished_at=NULL,detail='Retry with paragraph prompts and top-ten evidence',input_version='paragraphs-20261102:'||COALESCE(a.input_version,'')
 FROM retry_assignments r WHERE a.edition_date=r.edition_date AND a.stage=r.stage AND a.entity_type=r.entity_type AND a.entity_id=r.entity_id AND a.sport=r.sport;
-- The existing frozen-edition offer trigger is bypassed only inside this locked transaction.
UPDATE coverage_schedule SET enabled=false;
INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
 SELECT a.stage,a.entity_type,a.entity_id,a.sport,a.input_version FROM coverage_assignments a JOIN retry_assignments r USING(edition_date,stage,entity_type,entity_id,sport)
 ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET input_version=EXCLUDED.input_version,status='pending',attempts=0,claim_token=NULL,running_input_version=NULL,available_at=clock_timestamp(),updated_at=clock_timestamp(),last_error=NULL;
UPDATE coverage_schedule SET enabled=true;
SELECT count(*) AS recovered_assignments FROM retry_assignments;
\if :apply
 COMMIT;
\else
 ROLLBACK;
\endif

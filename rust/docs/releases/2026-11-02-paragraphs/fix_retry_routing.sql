\set ON_ERROR_STOP on
BEGIN;
SET LOCAL lock_timeout='3s';
SELECT pg_advisory_xact_lock(1935896421,1);
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM coverage_schedule WHERE enabled AND phase='characters' AND edition_date='2026-10-23') THEN RAISE EXCEPTION 'Edition changed'; END IF;
 IF EXISTS(SELECT 1 FROM pipeline_work WHERE stage='transfers' AND status='running') THEN RAISE EXCEPTION 'Insider claim still running'; END IF;
END $$;
CREATE TEMP TABLE routing_fix ON COMMIT DROP AS
 SELECT a.* FROM coverage_assignments a JOIN pipeline_work w USING(stage,entity_type,entity_id,sport)
 WHERE a.edition_date='2026-10-23' AND a.stage='transfers' AND a.outcome='pending' AND a.prepared_input IS NULL
 AND a.input_version LIKE 'paragraphs-20261102:classifier-spectrum-v1:%'
 AND w.input_version=a.input_version AND w.status='failed' AND w.last_error LIKE '%Cannot resolve undelivered frozen sources%'
 AND NOT EXISTS(SELECT 1 FROM insider_scores p WHERE p.candidate_edition=a.edition_date AND p.entity_type=a.entity_type AND p.entity_id=a.entity_id AND p.sport=a.sport);
DO $$ BEGIN IF (SELECT count(*) FROM routing_fix) NOT BETWEEN 1 AND 199 THEN RAISE EXCEPTION 'Unexpected correction scope'; END IF; END $$;
\pset format unaligned
\pset tuples_only on
\o /mnt/data/backup/scoracle/releases/paragraphs-20261102-93e3aa7d/routing-fix-before.jsonl
SELECT jsonb_build_object('table','coverage_assignments','row',to_jsonb(a)) FROM routing_fix a
UNION ALL SELECT jsonb_build_object('table','pipeline_work','row',to_jsonb(w)) FROM pipeline_work w JOIN routing_fix a USING(stage,entity_type,entity_id,sport);
\o
\pset tuples_only off
\pset format aligned
UPDATE coverage_schedule SET enabled=false;
UPDATE coverage_assignments a SET input_version=substr(a.input_version,length('paragraphs-20261102:')+1)||':paragraphs-20261102',detail='Correct retry marker placement; previous attempt took legacy path without inference'
 FROM routing_fix r WHERE a.edition_date=r.edition_date AND a.stage=r.stage AND a.entity_type=r.entity_type AND a.entity_id=r.entity_id AND a.sport=r.sport;
UPDATE pipeline_work w SET input_version=a.input_version,status='pending',attempts=0,available_at=clock_timestamp(),updated_at=clock_timestamp(),last_error=NULL
 FROM coverage_assignments a JOIN routing_fix r USING(edition_date,stage,entity_type,entity_id,sport)
 WHERE w.stage=a.stage AND w.entity_type=a.entity_type AND w.entity_id=a.entity_id AND w.sport=a.sport;
UPDATE coverage_schedule SET enabled=true;
SELECT count(*) AS corrected_routing_without_model_call FROM routing_fix;
COMMIT;

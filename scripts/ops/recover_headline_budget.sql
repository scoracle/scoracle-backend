-- Scoped Journalist byte-budget recovery, after migration 323 and matching binaries.
-- Preview (rolls back): psql "$DATABASE_URL" -X -v edition_date=2026-10-23 \
--   -v assignment_scope='[{"stage":"narratives","entity_type":"player","entity_id":15,"sport":"NBA"}]' \
--   -v spam_spectra='{...shared Rust spam-screen spectrum IDs...}' -f scripts/ops/recover_headline_budget.sql
-- Apply: add -v apply=true -v backup_file=/private/server/path/unique-rows.jsonl
-- Stop both hosts' character workers/controller first; no model calls occur here.
\set ON_ERROR_STOP on
\if :{?apply}
\else
  \set apply false
\endif
BEGIN;
SET LOCAL lock_timeout='3s';
SET LOCAL statement_timeout='30s';
SELECT pg_advisory_xact_lock(1935896421,1);
CREATE TEMP TABLE recovery_scope ON COMMIT DROP AS
SELECT * FROM jsonb_to_recordset(:'assignment_scope'::jsonb)
    AS k(stage text,entity_type text,entity_id bigint,sport text);
ALTER TABLE recovery_scope ADD PRIMARY KEY(stage,entity_type,entity_id,sport);
CREATE TEMP TABLE recovery_parameters ON COMMIT DROP AS
SELECT :'edition_date'::date edition_date,:'spam_spectra'::bigint[] spam_spectra;
DO $$ BEGIN
    IF (SELECT count(*) FROM recovery_scope) NOT BETWEEN 1 AND 40
       OR EXISTS(SELECT 1 FROM recovery_scope WHERE stage<>'narratives') THEN
        RAISE EXCEPTION 'Recovery requires 1–40 explicit Journalist assignment keys';
    END IF;
    IF NOT EXISTS(SELECT 1 FROM coverage_schedule c JOIN recovery_parameters p USING(edition_date)
        WHERE c.enabled AND c.phase='characters') THEN
        RAISE EXCEPTION 'Requested edition is not the active frozen character edition';
    END IF;
    IF EXISTS(SELECT 1 FROM pipeline_work WHERE coverage_is_character(stage) AND status='running') THEN
        RAISE EXCEPTION 'Character claims must drain before recovery';
    END IF;
    IF EXISTS(SELECT 1 FROM recovery_scope k CROSS JOIN recovery_parameters p
        LEFT JOIN coverage_assignments a ON a.edition_date=p.edition_date AND a.stage=k.stage
            AND a.entity_type=k.entity_type AND a.entity_id=k.entity_id AND a.sport=k.sport
        WHERE a.stage IS NULL) THEN RAISE EXCEPTION 'Unknown assignment key'; END IF;
END $$;
CREATE TEMP TABLE recovery_assignments ON COMMIT DROP AS
SELECT a.* FROM coverage_assignments a JOIN recovery_scope k USING(stage,entity_type,entity_id,sport)
JOIN recovery_parameters p USING(edition_date)
WHERE a.input_version IS NULL OR a.input_version NOT LIKE 'headline-policy-v2:%';
DO $$ BEGIN
    IF EXISTS(SELECT 1 FROM recovery_assignments a LEFT JOIN pipeline_work w
        USING(stage,entity_type,entity_id,sport)
        WHERE a.outcome<>'failed' OR a.resolution IS DISTINCT FROM 'blocked' OR a.prepared_input IS NOT NULL
            OR (w.stage IS NOT NULL AND (w.status<>'failed' OR w.input_version IS DISTINCT FROM a.input_version))) THEN
        RAISE EXCEPTION 'Recovery scope changed or is not failed byte-budget work';
    END IF;
    IF EXISTS(SELECT 1 FROM recovery_assignments a WHERE
        NOT EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
            AND d.character='journalist' AND d.status='held' AND d.reason IN
            ('daily_package_exceeds_context_budget','complete_report_exceeds_context_budget','daily_package_exceeds_token_budget'))
        OR EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
            AND d.character='journalist' AND d.status='held' AND (d.reason IS NULL OR d.reason NOT IN
            ('daily_package_exceeds_context_budget','complete_report_exceeds_context_budget','daily_package_exceeds_token_budget')))) THEN
        RAISE EXCEPTION 'Scope includes identity, source or model holds; separate reconciliation required';
    END IF;
    IF EXISTS(SELECT 1 FROM recovery_assignments a JOIN classifier_spectra x ON x.id=ANY(a.spectrum_ids)
        JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id AND d.character='journalist'
        JOIN classifier_sources s ON s.id=x.source_id
        WHERE d.status IN ('pending','held') AND (
            NOT COALESCE(s.discovery_version=classifier_discovery_version(s.article_id,s.sport),false)
            OR NOT EXISTS(SELECT 1 FROM classifier_source_heads h WHERE h.article_id=s.article_id
                AND h.sport=s.sport AND h.source_id=s.id)
            OR NOT EXISTS(SELECT 1 FROM jsonb_array_elements(s.source->'identity_candidates') candidate
                WHERE candidate->>'entity_type'=a.entity_type AND candidate->>'entity_id'=a.entity_id::text))) THEN
        RAISE EXCEPTION 'Scope includes stale or unresolved sources';
    END IF;
    IF EXISTS(SELECT 1 FROM recovery_assignments a JOIN news_summaries n
        ON n.candidate_edition=a.edition_date AND n.entity_type=a.entity_type AND n.entity_id=a.entity_id AND n.sport=a.sport) THEN
        RAISE EXCEPTION 'Assignment already has a candidate product';
    END IF;
    IF EXISTS(SELECT 1 FROM recovery_parameters p CROSS JOIN unnest(p.spam_spectra) id
        WHERE NOT EXISTS(SELECT 1 FROM coverage_assignments a JOIN recovery_scope k USING(stage,entity_type,entity_id,sport)
            WHERE a.edition_date=p.edition_date AND id=ANY(a.spectrum_ids))) THEN
        RAISE EXCEPTION 'Spam IDs fall outside the requested scope';
    END IF;
END $$;
SELECT entity_type,entity_id,sport,cardinality(spectrum_ids) frozen FROM recovery_assignments ORDER BY sport,entity_type,entity_id;
\if :apply
  \if :{?backup_file}
  \else
    \echo 'Apply requires a unique private backup_file on the operator host.'
    \quit 1
  \endif
  \pset format unaligned
  \pset tuples_only on
  \o :backup_file
  SELECT jsonb_build_object('table','coverage_assignments','row',to_jsonb(a)) FROM recovery_assignments a
  UNION ALL SELECT jsonb_build_object('table','coverage_schedule','row',to_jsonb(c)) FROM coverage_schedule c
  UNION ALL SELECT jsonb_build_object('table','pipeline_work','row',to_jsonb(w)) FROM pipeline_work w
    JOIN recovery_assignments a USING(stage,entity_type,entity_id,sport)
  UNION ALL SELECT jsonb_build_object('table','classifier_spectrum_deliveries','row',to_jsonb(d))
    FROM classifier_spectrum_deliveries d WHERE d.character='journalist'
        AND EXISTS(SELECT 1 FROM recovery_assignments a WHERE d.spectrum_id=ANY(a.spectrum_ids));
  \o
  \pset tuples_only off
  \pset format aligned
\endif
UPDATE classifier_spectrum_deliveries d SET status='pending',reason=NULL,updated_at=clock_timestamp(),
    product_ref=jsonb_build_object('recovery_policy','headline-policy-v2','previous_delivery',to_jsonb(d))
WHERE d.character='journalist' AND d.status='held' AND EXISTS(
    SELECT 1 FROM recovery_assignments a WHERE d.spectrum_id=ANY(a.spectrum_ids));
SELECT public.coverage_select_headlines(p.spam_spectra,
    COALESCE((SELECT jsonb_agg(jsonb_build_object('stage',stage,'entity_type',entity_type,'entity_id',entity_id,'sport',sport))
        FROM recovery_assignments),'[]'::jsonb)) AS excluded_without_model_call FROM recovery_parameters p;
UPDATE coverage_assignments a SET outcome='pending',resolution=NULL,finished_at=NULL,
    detail='byte-budget recovery: headline-selection-v2',input_version='headline-policy-v2:'||COALESCE(a.input_version,'')
FROM recovery_assignments r WHERE a.edition_date=r.edition_date AND a.stage=r.stage
    AND a.entity_type=r.entity_type AND a.entity_id=r.entity_id AND a.sport=r.sport;
UPDATE pipeline_work w SET status='pending',attempts=0,claim_token=NULL,running_input_version=NULL,
    available_at=clock_timestamp(),updated_at=clock_timestamp(),last_error=NULL,input_version=a.input_version
FROM coverage_assignments a JOIN recovery_assignments r USING(edition_date,stage,entity_type,entity_id,sport)
WHERE w.stage=a.stage AND w.entity_type=a.entity_type AND w.entity_id=a.entity_id AND w.sport=a.sport;
-- The enqueue trigger normally defers offers for frozen assignments. Under the
-- exclusive coverage lock, bypass it inside this transaction for missing jobs only.
UPDATE coverage_schedule SET enabled=false;
INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
SELECT a.stage,a.entity_type,a.entity_id,a.sport,a.input_version
FROM coverage_assignments a JOIN recovery_assignments r USING(edition_date,stage,entity_type,entity_id,sport)
WHERE NOT EXISTS(SELECT 1 FROM pipeline_work w WHERE w.stage=a.stage AND w.entity_type=a.entity_type
    AND w.entity_id=a.entity_id AND w.sport=a.sport);
UPDATE coverage_schedule SET enabled=true;
SELECT d.status,d.reason,count(*) FROM classifier_spectrum_deliveries d
WHERE d.character='journalist' AND EXISTS(SELECT 1 FROM recovery_assignments a WHERE d.spectrum_id=ANY(a.spectrum_ids))
GROUP BY d.status,d.reason ORDER BY 1,2;
\if :apply
  COMMIT;
\else
  ROLLBACK;
\endif

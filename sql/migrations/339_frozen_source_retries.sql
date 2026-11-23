BEGIN;
SET LOCAL lock_timeout='5s';

-- Keep the IDs transferred to a later edition alongside the original assignment.
-- Their delivery remains pending. Only the still-owned IDs participate in settlement.
ALTER TABLE public.coverage_assignments ADD COLUMN carried_spectrum_ids bigint[] NOT NULL DEFAULT '{}';

CREATE FUNCTION public.coverage_reconcile_sources(a public.coverage_assignments, saved_card boolean)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE voice text; packet jsonb; spill bigint[];
BEGIN
 voice := CASE a.stage WHEN 'narratives' THEN 'journalist' WHEN 'vibe' THEN 'influencer'
                      WHEN 'rating' THEN 'scout' WHEN 'transfers' THEN 'insider' END;
 IF voice IS NULL OR cardinality(a.spectrum_ids)=0 THEN RETURN; END IF;
 packet := coalesce(a.prepared_input#>'{Ready,parts,availability}',a.prepared_input#>'{NoStats,availability}');
 -- Only terminalize sources for an independently verifiable reason. In particular,
 -- a live, readable source that the worker forgot to acknowledge must still fail.
 WITH pending AS MATERIALIZED (
  SELECT d.id AS delivery_id,x.criteria_version,s.id AS source_id,s.article_id,s.sport,s.source,s.discovery_version FROM unnest(a.spectrum_ids) sid
  JOIN classifier_spectrum_deliveries d ON d.spectrum_id=sid AND d.character=voice AND d.status='pending'
  JOIN classifier_spectra x ON x.id=sid JOIN classifier_sources s ON s.id=x.source_id
 ), reasons AS (
  SELECT p.delivery_id, CASE
   WHEN p.criteria_version NOT IN ('classification-criteria-v17-shared-relations','classification-criteria-v12-newsworthy',
       'classification-criteria-v11-targeted','classification-criteria-v10-availability','classification-criteria-v9-headlines')
    THEN 'unsupported_character_criteria'
   WHEN p.discovery_version IS NULL THEN 'unknown_source_revision'
   WHEN p.discovery_version IS DISTINCT FROM classifier_discovery_version(p.article_id,p.sport)
      OR NOT EXISTS(SELECT 1 FROM classifier_source_heads h WHERE h.article_id=p.article_id AND h.sport=p.sport AND h.source_id=p.source_id)
    THEN 'obsolete_source_revision'
   WHEN NOT EXISTS(SELECT 1 FROM jsonb_array_elements(p.source->'identity_candidates') i
         WHERE i->>'entity_type'=a.entity_type AND i->>'entity_id'=a.entity_id::text)
    THEN 'unresolved_target_identity' END AS reason FROM pending p
 )
 UPDATE classifier_spectrum_deliveries d SET status=CASE WHEN r.reason='obsolete_source_revision' THEN 'redundant' ELSE 'held' END,
 reason=r.reason,product_ref=jsonb_build_object('edition',a.edition_date,'model_called',false),updated_at=clock_timestamp()
 FROM reasons r WHERE d.id=r.delivery_id AND r.reason IS NOT NULL AND d.status='pending';

 SELECT array_agg(sid ORDER BY sid) INTO spill FROM unnest(a.spectrum_ids) sid
 JOIN classifier_spectrum_deliveries d ON d.spectrum_id=sid AND d.character=voice AND d.status='pending'
 WHERE saved_card OR (a.stage='rating' AND jsonb_typeof(packet->'sources')='array'
  AND jsonb_typeof(packet->'held')='array' AND jsonb_typeof(packet->'redundant')='array'
  AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements((packet->'sources')||(packet->'held')||(packet->'redundant')) s
                 WHERE s->>'classification_id'=sid::text));
 IF spill IS NOT NULL THEN
  UPDATE coverage_assignments SET spectrum_ids=ARRAY(SELECT id FROM unnest(spectrum_ids) id WHERE NOT id=ANY(spill)),
   carried_spectrum_ids=carried_spectrum_ids||spill,
   deferred_versions=CASE WHEN cardinality(deferred_versions)>0 THEN deferred_versions
    ELSE ARRAY['carry:'||a.edition_date::text||':'||a.stage] END
  WHERE (edition_date,stage,entity_type,entity_id,sport)=(a.edition_date,a.stage,a.entity_type,a.entity_id,a.sport);
 END IF;
END $$;

-- A committed daily card and its reviewed sources survive a crash or a normal
-- batch deferral. A new exact lease finishes it through the ordinary worker path.
CREATE FUNCTION public.coverage_resume_daily_card() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE a coverage_assignments;
BEGIN
 SELECT a0.* INTO a FROM coverage_assignments a0 JOIN coverage_schedule c USING(edition_date)
 WHERE c.enabled AND c.phase='characters' AND a0.outcome='pending'
 AND (a0.stage,a0.entity_type,a0.entity_id,a0.sport)=(NEW.stage,NEW.entity_type,NEW.entity_id,NEW.sport)
 AND a0.input_version IS NOT DISTINCT FROM NEW.running_input_version FOR UPDATE OF a0;
 IF NOT FOUND OR cardinality(a.spectrum_ids)=0 THEN RETURN NULL; END IF;
 IF (a.stage='narratives' AND EXISTS(SELECT 1 FROM news_summaries n WHERE n.candidate_edition=a.edition_date
     AND (n.entity_type,n.entity_id,n.sport)=(a.entity_type,a.entity_id,a.sport)))
 OR (a.stage='vibe' AND EXISTS(SELECT 1 FROM vibe_scores v WHERE v.candidate_edition=a.edition_date
     AND (v.entity_type,v.entity_id,v.sport)=(a.entity_type,a.entity_id,a.sport))) THEN
  PERFORM coverage_reconcile_sources(a,true);
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER resume_frozen_daily_card AFTER UPDATE OF status ON public.pipeline_work FOR EACH ROW
 WHEN (NEW.status='running' AND OLD.status<>'running' AND NEW.claim_token IS NOT NULL AND NEW.stage IN ('narratives','vibe'))
 EXECUTE FUNCTION public.coverage_resume_daily_card();

CREATE OR REPLACE FUNCTION public.coverage_resolve_claim(task text,subject_type text,subject_id bigint,league text,token text)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE a coverage_assignments; has_product boolean; has_record boolean; held boolean; pending boolean; product_table text; body_column text;
BEGIN
 SELECT a0.* INTO a FROM coverage_assignments a0 JOIN coverage_schedule s USING(edition_date)
 WHERE s.enabled AND s.phase='characters' AND a0.stage=task AND a0.entity_type=subject_type
 AND a0.entity_id=subject_id AND a0.sport=league FOR UPDATE OF a0;
 IF NOT FOUND THEN RETURN; END IF;
 IF NOT EXISTS(SELECT 1 FROM pipeline_work w WHERE w.stage=task AND w.entity_type=subject_type
 AND w.entity_id=subject_id AND w.sport=league AND w.status='running' AND w.claim_token::text=token
 AND w.running_input_version IS NOT DISTINCT FROM a.input_version) THEN
 RAISE EXCEPTION 'Cannot resolve an unfenced assignment'; END IF;
 product_table:=CASE task WHEN 'narratives' THEN 'news_summaries' WHEN 'vibe' THEN 'vibe_scores'
 WHEN 'rating' THEN 'stat_summaries' WHEN 'transfers' THEN 'insider_scores'
 WHEN 'momentum' THEN 'momentum_summaries' WHEN 'sigil' THEN 'sigil_synthesis' END;
 body_column:=CASE task WHEN 'vibe' THEN 'prompt' WHEN 'transfers' THEN 'read'
 WHEN 'momentum' THEN 'blurb' WHEN 'sigil' THEN 'reading' ELSE 'body' END;
 EXECUTE format('SELECT count(*)>0,COALESCE(bool_or(length(btrim(%I))>0),false) FROM public.%I
 WHERE candidate_edition=$1 AND entity_type=$2 AND entity_id=$3 AND sport=$4',body_column,product_table)
 INTO has_record,has_product USING a.edition_date,subject_type,subject_id,league;
 IF EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
    AND d.status='pending' AND d.character=CASE task WHEN 'narratives' THEN 'journalist' WHEN 'rating' THEN 'scout'
    WHEN 'vibe' THEN 'influencer' WHEN 'transfers' THEN 'insider' END) THEN
  PERFORM coverage_reconcile_sources(a,false);
  SELECT * INTO STRICT a FROM coverage_assignments WHERE (edition_date,stage,entity_type,entity_id,sport)=(a.edition_date,task,subject_type,subject_id,league);
 END IF;
 SELECT COALESCE(bool_or(d.status='held'),false),COALESCE(bool_or(d.status='pending'),false)
 INTO held,pending FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
 AND d.character=CASE task WHEN 'narratives' THEN 'journalist' WHEN 'rating' THEN 'scout'
 WHEN 'vibe' THEN 'influencer' WHEN 'transfers' THEN 'insider' END;
 IF pending THEN RAISE EXCEPTION 'Cannot resolve undelivered frozen sources'; END IF;
 -- Holds belong to their source. Only qualify cards backed by this frozen
 -- assignment's reviewed receipts, with no held input attached to the card.
 IF held AND has_product AND task='narratives' THEN
  SELECT EXISTS (
   SELECT 1 FROM news_summaries n
   WHERE n.candidate_edition=a.edition_date AND n.entity_type=subject_type AND n.entity_id=subject_id AND n.sport=league
    AND (COALESCE(cardinality(n.input_news_ids),0)=0 OR EXISTS (
     SELECT 1 FROM unnest(n.input_news_ids) article_id
     WHERE NOT EXISTS (SELECT 1 FROM classifier_spectra x JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id
      WHERE x.id=ANY(a.spectrum_ids) AND x.article_id=article_id.article_id AND d.character='journalist'
       AND d.status='used' AND d.product_ref->'news_summary_ids' @> jsonb_build_array(n.id))
     OR EXISTS (SELECT 1 FROM classifier_spectra x JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id
      WHERE x.id=ANY(a.spectrum_ids) AND x.article_id=article_id.article_id AND d.character='journalist' AND d.status='held')))
  ) INTO held;
 ELSIF held AND has_product AND task='transfers' THEN
  SELECT EXISTS (
   SELECT 1 FROM insider_scores i
   WHERE i.candidate_edition=a.edition_date AND i.entity_type=subject_type AND i.entity_id=subject_id AND i.sport=league
    AND (NOT EXISTS (SELECT 1 FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
     AND d.character='insider' AND d.status IN ('used','abstained') AND d.product_ref->'score_id'=to_jsonb(i.id))
    OR EXISTS (SELECT 1 FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
     AND d.character='insider' AND d.status='held' AND d.product_ref->'score_id'=to_jsonb(i.id)))
  ) INTO held;
 END IF;
 UPDATE coverage_assignments SET outcome=CASE WHEN held THEN 'failed' ELSE 'completed' END,
 resolution=CASE WHEN held THEN 'blocked' WHEN has_product THEN 'candidate'
 WHEN task='rating' AND current_setting('scoracle.assignment_resolution',true)='unchanged' THEN 'unchanged'
 WHEN has_record THEN 'abstained' WHEN cardinality(a.spectrum_ids)=0 THEN 'no_new_evidence' ELSE 'abstained' END,
 detail=CASE WHEN held THEN 'held source requires explicit resolution'
 WHEN has_product THEN 'claim-fenced validated product'
 WHEN task='rating' AND current_setting('scoracle.assignment_resolution',true)='unchanged' THEN 'exact prepared input unchanged'
 WHEN has_record THEN 'explicit no-material marker'
 WHEN cardinality(a.spectrum_ids)=0 THEN 'prepared assignment has no new usable evidence'
 ELSE 'validated source dispositions; no expression' END,finished_at=clock_timestamp()
 WHERE edition_date=a.edition_date AND stage=task AND entity_type=subject_type AND entity_id=subject_id AND sport=league;
END $$;
INSERT INTO schema_migrations(version) VALUES ('339_frozen_source_retries');
COMMIT;

BEGIN;
SET LOCAL statement_timeout='20s';
UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_SELECTION','Selection fixture',2026);
INSERT INTO teams(id,sport,name) VALUES(925001,'ZZ_SELECTION','Cedar Policy');

CREATE TEMP TABLE policy_sources AS
SELECT i,925100+i AS article_id,
    CASE WHEN i IN (0,51) THEN 'Cedar Policy announces complete development number 1'
        WHEN i=52 THEN 'CEDAR POLICY announces complete development number 2'
        WHEN i=55 THEN '[WATCHLIVE] Cedar Policy football LIVE Stream'
        WHEN i=53 THEN 'Cedar Policy denies the previously reported agreement'
        ELSE 'Cedar Policy announces complete development number '||i END AS headline
FROM generate_series(0,55) i;
INSERT INTO news_articles(id,url,url_hash,title,source,published_at)
SELECT article_id,'https://example.invalid/policy/'||i,md5('policy-'||i),headline,'Wire',now()-interval '1 hour'
FROM policy_sources;
INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source)
SELECT article_id,'ZZ_SELECTION',md5(i::text),md5(''),jsonb_build_object(
    'article_id',article_id,'title',headline,'source','Wire','evidence_scope','headline',
    'published_at',now()-interval '1 hour','identity_candidates',
    CASE WHEN i=54 THEN '[]'::jsonb ELSE '[{"entity_type":"team","entity_id":925001}]'::jsonb END)
FROM policy_sources;
INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,
    scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt)
SELECT s.id,p.article_id,'ZZ_SELECTION','{"entity_type":"team","entity_id":925001,"sport":"ZZ_SELECTION"}',
    'fixture',CASE WHEN p.i%2=0 THEN 'classification-criteria-v10-availability' ELSE 'classification-criteria-v11-targeted' END,'rss-headline-spectrum-v1',p.headline,md5(p.headline),
    CASE WHEN p.i IN (53,55) THEN '{"subject":100,"emotion":100,"movement":100,"availability":100}'::jsonb
    ELSE jsonb_build_object('subject',55+p.i%46,'emotion',100-p.i,'movement',60+p.i*7%40,'availability',85+p.i%15) END,
    '{"coverage":"complete","admitted":true,"characters":{"journalist":true,"influencer":true,"insider":true,"scout":true}}',
    md5(p.i::text),jsonb_build_object('request',jsonb_build_object('state',jsonb_build_object('article',jsonb_build_object('headline',p.headline))),
        'response',jsonb_build_object('model','fixture'))
FROM policy_sources p JOIN classifier_sources s USING(article_id);
INSERT INTO classifier_spectrum_deliveries(spectrum_id,character,status)
SELECT x.id,character,CASE WHEN p.i=0 THEN 'used' ELSE 'pending' END
FROM classifier_spectra x JOIN policy_sources p USING(article_id)
CROSS JOIN unnest(ARRAY['journalist','influencer','insider','scout']) character;
INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,frozen_at,status)
VALUES('2098-01-02',now()-interval '1 minute',now(),now(),'characters');
UPDATE coverage_schedule SET enabled=true,enrolled_at=now()-interval '1 day',phase='characters',edition_date='2098-01-02';
INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport,spectrum_ids)
SELECT '2098-01-02',stage,'team',925001,'ZZ_SELECTION',array_agg(x.id)
FROM classifier_spectra x JOIN policy_sources p USING(article_id)
CROSS JOIN unnest(ARRAY['narratives','vibe','transfers','rating']) stage
WHERE p.i<>0 GROUP BY stage;
DO $$ BEGIN
    IF public.coverage_select_headlines('{}','[]')<>0 THEN RAISE EXCEPTION 'Empty recovery scope selected other assignments'; END IF;
END $$;
SELECT public.coverage_select_headlines(ARRAY(SELECT x.id FROM classifier_spectra x JOIN policy_sources p USING(article_id) WHERE p.i=55));
DO $$ DECLARE role text; criterion text; BEGIN
    IF public.headline_entity_cap()<>10 THEN RAISE EXCEPTION 'Unexpected headline ceiling'; END IF;
    FOREACH role IN ARRAY ARRAY['journalist','influencer','insider','scout'] LOOP
        criterion:=CASE role WHEN 'journalist' THEN 'subject' WHEN 'influencer' THEN 'emotion'
            WHEN 'insider' THEN 'movement' ELSE 'availability' END;
        IF (SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
            JOIN policy_sources p USING(article_id) WHERE d.character=role AND d.status='pending' AND p.i<>54)<>10 THEN
            RAISE EXCEPTION 'Expected exactly 10 eligible inputs for %',role; END IF;
        IF (SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
            JOIN policy_sources p USING(article_id) WHERE d.character=role AND d.reason='already_reviewed_headline')<>2 THEN
            RAISE EXCEPTION 'Previously reviewed reposts were not excluded for %',role; END IF;
        IF (SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
            JOIN policy_sources p USING(article_id) WHERE d.character=role AND d.reason='reposted_headline')<>1 THEN
            RAISE EXCEPTION 'Presentation-only copy was not deduplicated for %',role; END IF;
        IF EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
            JOIN policy_sources p USING(article_id) WHERE d.character=role AND p.i IN (53,54) AND d.status<>'pending') THEN
            RAISE EXCEPTION 'Denial or unresolved identity was incorrectly disposed for %',role; END IF;
        IF EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
            JOIN policy_sources p USING(article_id) WHERE d.character=role AND d.reason='ranked_below_entity_cap'
            AND (d.status<>'abstained' OR d.product_ref->>'selection_policy' IS DISTINCT FROM 'headline-selection-v3'
                OR d.product_ref->>'model_called' IS DISTINCT FROM 'false'
                OR d.product_ref->>'entity_cap' IS DISTINCT FROM '10')) THEN
            RAISE EXCEPTION 'Ranked exclusion lost its policy receipt for %',role; END IF;
        IF EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
            WHERE d.character=role AND d.reason='ranked_below_entity_cap' AND (x.spectrum->>criterion)::numeric>
                (SELECT min((y.spectrum->>criterion)::numeric) FROM classifier_spectrum_deliveries kept
                 JOIN classifier_spectra y ON y.id=kept.spectrum_id JOIN policy_sources p ON p.article_id=y.article_id
                 WHERE kept.character=role AND kept.status='pending' AND p.i<>54)) THEN
            RAISE EXCEPTION 'A stronger input was trimmed for %',role; END IF;
    END LOOP;
    IF (SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id JOIN policy_sources p USING(article_id) WHERE p.i=55 AND d.reason='livestream_spam')<>4 THEN RAISE EXCEPTION 'Spam consumed ranked slots'; END IF;
    IF public.coverage_select_headlines()<>0 THEN RAISE EXCEPTION 'Selection must be idempotent'; END IF;
    IF headline_content_key('Cedar denies a 2-1 result')=headline_content_key('Cedar denies a 21 result')
       OR headline_content_key('Cedar denies talks')=headline_content_key('Cedar confirms talks') THEN
        RAISE EXCEPTION 'Fingerprint erased a factual difference'; END IF;
END $$;
-- Legacy archives can contain long full text; fixed-size index keys must preserve it.
DO $$ DECLARE archived text; BEGIN
    SELECT string_agg(md5(i::text),' ' ORDER BY i) INTO archived FROM generate_series(1,10000) i;
    UPDATE news_articles SET title=archived WHERE id=925100;
    UPDATE classifier_spectra SET clean_text=archived,clean_sha256=md5(archived),
        receipt=jsonb_set(receipt,'{request,state,article,headline}',to_jsonb(archived)) WHERE article_id=925100;
    IF (SELECT min(octet_length(clean_text)) FROM classifier_spectra WHERE article_id=925100)<300000 THEN
        RAISE EXCEPTION 'Archived long text was lost';
    END IF;
END $$;
-- The new prompt version must retain the same provenance and routing guard.
DO $$ DECLARE sid bigint; caught boolean; bad integer; BEGIN
    SELECT id INTO STRICT sid FROM classifier_spectra WHERE article_id=925101 AND sport='ZZ_SELECTION';
    FOR bad IN 1..3 LOOP
        caught:=false;
        BEGIN
            IF bad=1 THEN UPDATE classifier_spectra SET gate=jsonb_set(gate,'{characters,journalist}','false') WHERE id=sid;
            ELSIF bad=2 THEN UPDATE classifier_sources SET source=jsonb_set(source,'{title}','"different headline"')
                WHERE id=(SELECT source_id FROM classifier_spectra WHERE id=sid);
            ELSE UPDATE classifier_sources SET source=jsonb_set(source,'{evidence_scope}','"article"')
                WHERE id=(SELECT source_id FROM classifier_spectra WHERE id=sid);
            END IF;
            UPDATE classifier_spectrum_deliveries SET status='pending' WHERE spectrum_id=sid AND character='journalist';
        EXCEPTION WHEN raise_exception THEN
            IF SQLERRM NOT LIKE 'Headline delivery requires%' THEN RAISE; END IF;
            caught:=true;
        END;
        IF NOT caught THEN RAISE EXCEPTION 'v11 skipped headline guard case %',bad; END IF;
    END LOOP;
END $$;
-- Finalizing a mixed packet keeps its source holds independent of its card.
DO $$
DECLARE lease text; card bigint; accepted bigint;
BEGIN
 UPDATE pipeline_work SET status='running',input_version=NULL
 WHERE stage='narratives' AND entity_type='team' AND entity_id=925001 AND sport='ZZ_SELECTION'
 RETURNING claim_token::text INTO STRICT lease;
 PERFORM set_config('scoracle.claim_token',lease,true);
 SELECT article_id INTO STRICT accepted FROM policy_sources WHERE i=53;
 INSERT INTO news_summaries(entity_type,entity_id,sport,trigger_type,body,input_news_ids)
 VALUES('team',925001,'ZZ_SELECTION','classifier','A reported denial',ARRAY[accepted]) RETURNING id INTO card;
 UPDATE classifier_spectrum_deliveries d SET status='held',reason='unresolved_target_identity'
 FROM classifier_spectra x WHERE x.id=d.spectrum_id AND x.sport='ZZ_SELECTION'
  AND d.character='journalist' AND d.status='pending';
 UPDATE classifier_spectrum_deliveries d SET status='used',reason=NULL,product_ref=jsonb_build_object('news_summary_ids',jsonb_build_array(card))
 FROM classifier_spectra x WHERE x.id=d.spectrum_id AND x.article_id=accepted AND d.character='journalist';
 PERFORM coverage_resolve_claim('narratives','team',925001,'ZZ_SELECTION',lease);
 IF NOT EXISTS(SELECT 1 FROM qualified_news_summaries WHERE id=card) THEN RAISE EXCEPTION 'uncited hold hid sourced card'; END IF;
 UPDATE news_summaries SET input_news_ids=array_append(input_news_ids,(SELECT article_id FROM policy_sources WHERE i=54)) WHERE id=card;
 PERFORM coverage_resolve_claim('narratives','team',925001,'ZZ_SELECTION',lease);
 IF EXISTS(SELECT 1 FROM qualified_news_summaries WHERE id=card) THEN RAISE EXCEPTION 'card citing held source was qualified'; END IF;
END $$;
-- A second lease resumes an existing daily card, without consuming unread reports.
INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport)
SELECT article_id,'team',925001,'ZZ_SELECTION' FROM policy_sources WHERE i IN (8,9);
UPDATE classifier_sources s SET discovery_version=classifier_discovery_version(s.article_id,s.sport)
WHERE s.sport='ZZ_SELECTION' AND s.article_id IN (925108,925109);
INSERT INTO classifier_source_heads(article_id,sport,source_id)
SELECT article_id,sport,id FROM classifier_sources WHERE sport='ZZ_SELECTION' AND article_id IN (925108,925109)
ON CONFLICT(article_id,sport) DO UPDATE SET source_id=EXCLUDED.source_id;
DO $$
DECLARE sid bigint; stale bigint; lease text; stage_name text; voice text; a coverage_assignments;
BEGIN
 SELECT id INTO STRICT sid FROM classifier_spectra WHERE article_id=925108;
 SELECT id INTO STRICT stale FROM classifier_spectra WHERE article_id=925109;
 UPDATE classifier_sources SET discovery_version='older' WHERE article_id=925109;
 FOREACH stage_name IN ARRAY ARRAY['narratives','vibe','rating'] LOOP
  voice:=CASE stage_name WHEN 'narratives' THEN 'journalist' WHEN 'vibe' THEN 'influencer' ELSE 'scout' END;
  UPDATE coverage_assignments SET outcome='pending',resolution=NULL,spectrum_ids=ARRAY[sid,stale],
    prepared_input=CASE WHEN stage_name='rating' THEN '{"NoStats":{"availability":{"sources":[],"held":[],"redundant":[]}}}'::jsonb END
   WHERE edition_date='2098-01-02' AND stage=stage_name;
  UPDATE classifier_spectrum_deliveries SET status='pending',reason=NULL,product_ref='{}' WHERE character=voice AND spectrum_id IN (sid,stale);
  UPDATE pipeline_work SET status='pending',input_version=NULL WHERE sport='ZZ_SELECTION' AND stage=stage_name;
  UPDATE pipeline_work SET status='running' WHERE sport='ZZ_SELECTION' AND stage=stage_name RETURNING claim_token::text INTO STRICT lease;
  PERFORM set_config('scoracle.claim_token',lease,true);
  IF stage_name='vibe' THEN
   INSERT INTO vibe_scores(entity_type,entity_id,sport,trigger_type,model_version,prompt_version,sentiment,prompt) VALUES('team',925001,'ZZ_SELECTION','manual','fixture','fixture',56,'First daily card');
   UPDATE pipeline_work SET status='pending' WHERE sport='ZZ_SELECTION' AND stage=stage_name;
   UPDATE pipeline_work SET status='running' WHERE sport='ZZ_SELECTION' AND stage=stage_name RETURNING claim_token::text INTO STRICT lease;
   PERFORM set_config('scoracle.claim_token',lease,true);
  END IF;
  PERFORM coverage_resolve_claim(stage_name,'team',925001,'ZZ_SELECTION',lease);
  SELECT * INTO STRICT a FROM coverage_assignments WHERE edition_date='2098-01-02' AND stage=stage_name;
  IF a.outcome<>'completed' OR a.carried_spectrum_ids<>ARRAY[sid] OR sid=ANY(a.spectrum_ids) OR cardinality(a.deferred_versions)<1
   OR NOT EXISTS(SELECT 1 FROM classifier_spectrum_deliveries WHERE character=voice AND spectrum_id=sid AND status='pending')
   OR NOT EXISTS(SELECT 1 FROM classifier_spectrum_deliveries WHERE character=voice AND spectrum_id=stale AND reason='obsolete_source_revision')
   THEN RAISE EXCEPTION 'checkpoint/receipt mismatch for %: %',stage_name,to_jsonb(a); END IF;
 END LOOP;
END $$;
ROLLBACK;

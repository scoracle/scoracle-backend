BEGIN;
SET LOCAL lock_timeout='5s';

ALTER TABLE public.pipeline_runs ADD COLUMN character_outputs jsonb;
ALTER TABLE public.character_output_flags ADD COLUMN watchdog_run_id bigint;
CREATE INDEX character_output_watchdog_pending ON public.character_output_flags(id)
 WHERE flag_kind='malformed_output' AND watchdog_run_id IS NULL;
CREATE INDEX character_output_watchdog_run ON public.character_output_flags(watchdog_run_id)
 WHERE watchdog_run_id IS NOT NULL;

-- Set defaults separately so existing publications retain unknown provenance.
DO $$
DECLARE product regclass;
BEGIN
 FOREACH product IN ARRAY ARRAY['public.news_summaries'::regclass,'public.vibe_scores'::regclass,
     'public.stat_summaries'::regclass,'public.insider_scores'::regclass,
     'public.momentum_summaries'::regclass,'public.sigil_synthesis'::regclass]
 LOOP
   EXECUTE format('ALTER TABLE %s ADD COLUMN generation_claim_token uuid',product);
   EXECUTE format('ALTER TABLE %s ALTER COLUMN generation_claim_token SET DEFAULT
       NULLIF(current_setting(''scoracle.claim_token'',true),'''')::uuid',product);
 END LOOP;
END $$;

-- Only reader-facing prose counts. Abstention markers may share these six tables.
CREATE VIEW public.character_watchdog_cards AS
 SELECT 'narratives'::text stage,entity_type,entity_id,sport,reporting_date,id,generated_at,generation_claim_token FROM public.news_summaries WHERE NULLIF(btrim(body),'') IS NOT NULL
 UNION ALL SELECT 'vibe',entity_type,entity_id,sport,reporting_date,id,generated_at,generation_claim_token FROM public.vibe_scores WHERE NULLIF(btrim(prompt),'') IS NOT NULL
 UNION ALL SELECT 'rating',entity_type,entity_id,sport,reporting_date,id,generated_at,generation_claim_token FROM public.stat_summaries WHERE NULLIF(btrim(body),'') IS NOT NULL
 UNION ALL SELECT 'transfers',entity_type,entity_id,sport,reporting_date,id,generated_at,generation_claim_token FROM public.insider_scores WHERE NULLIF(btrim(read),'') IS NOT NULL
 UNION ALL SELECT 'momentum',entity_type,entity_id,sport,reporting_date,id,generated_at,generation_claim_token FROM public.momentum_summaries WHERE NULLIF(btrim(blurb),'') IS NOT NULL
 UNION ALL SELECT 'sigil',entity_type,entity_id,sport,reporting_date,id,generated_at,generation_claim_token FROM public.sigil_synthesis WHERE NULLIF(btrim(reading),'') IS NOT NULL;

CREATE VIEW public.character_parser_review AS
 SELECT f.id AS flag_id,f.watchdog_run_id,f.recorded_at,f.edition_date,f.stage,f.sport,f.entity_type,f.entity_id,
        f.model,f.claim_token,f.reason,ARRAY(
          SELECT c.id FROM public.character_watchdog_cards c
          WHERE (c.stage,c.sport,c.entity_type,c.entity_id,c.generation_claim_token)=
                (f.stage,f.sport,f.entity_type,f.entity_id,f.claim_token) ORDER BY c.id
        ) AS published_card_ids
 FROM public.character_output_flags f WHERE f.flag_kind='malformed_output';

CREATE FUNCTION public.collect_character_output_report(run_id bigint) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE report jsonb; as_of timestamptz;
BEGIN
 SELECT finished_at,character_outputs INTO STRICT as_of,report FROM public.pipeline_runs
 WHERE id=run_id AND job='watchdog' FOR UPDATE;
 IF report IS NOT NULL THEN RETURN report; END IF;
 -- Concurrent runs serialize on the flag rows. Late commits remain unread for the next run.
 UPDATE public.character_output_flags SET watchdog_run_id=run_id
 WHERE flag_kind='malformed_output' AND watchdog_run_id IS NULL;
 WITH stages AS (SELECT unnest(ARRAY['narratives','vibe','rating','transfers','momentum','sigil']) AS stage),
 cards AS (SELECT c.stage,count(*) AS n,count(c.generation_claim_token) AS tracked,
           count(*) FILTER (WHERE EXISTS (SELECT 1 FROM public.character_output_flags f
               WHERE f.stage=c.stage AND f.claim_token=c.generation_claim_token AND f.flag_kind='malformed_output')) AS flagged
           FROM public.character_watchdog_cards c
           WHERE generated_at>=as_of-interval '24 hours' AND generated_at<=as_of GROUP BY c.stage),
 flags AS MATERIALIZED (SELECT stage,claim_token,reason FROM public.character_output_flags WHERE watchdog_run_id=run_id),
 claims AS (SELECT stage,count(DISTINCT claim_token) AS n FROM flags GROUP BY stage),
 reasons AS (SELECT stage,jsonb_object_agg(reason,n) AS counts FROM
             (SELECT stage,reason,count(*) AS n FROM flags GROUP BY stage,reason) grouped GROUP BY stage)
 SELECT jsonb_object_agg(s.stage,jsonb_build_object('published_cards_24h',COALESCE(cards.n,0),
          'tracked_cards_24h',COALESCE(cards.tracked,0),'flagged_cards_24h',COALESCE(cards.flagged,0),
          'new_flagged_claims',COALESCE(claims.n,0),'reasons',COALESCE(reasons.counts,'{}'::jsonb)))
 INTO report FROM stages s LEFT JOIN cards USING(stage) LEFT JOIN claims USING(stage) LEFT JOIN reasons USING(stage);
 UPDATE public.pipeline_runs SET character_outputs=report WHERE id=run_id;
 RETURN report;
END $$;

INSERT INTO public.schema_migrations(version) VALUES ('344_character_output_watchdog');
COMMIT;

BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
WITH active_work AS (
    SELECT * FROM pipeline_work WHERE
      (:'mode'='classifier' AND stage=ANY(regexp_split_to_array(lower(btrim(:'stages')),'\s*,\s*')))
      AND (public.coverage_claimable(stage,entity_type,entity_id,sport)
        OR (status='running' AND public.coverage_publishable(stage,entity_type,entity_id,sport)))
), queue AS (
    SELECT count(*) FILTER(WHERE status IN ('pending','failed') AND attempts<5 AND available_at<=now()) AS ready,
        count(*) FILTER(WHERE status='failed' AND attempts>=5) AS dead,
        count(*) FILTER(WHERE status='failed' AND attempts>=5 AND NOT EXISTS (
          SELECT 1 FROM pipeline_work_failures f WHERE (f.stage,f.entity_type,f.entity_id,f.sport)=
          (active_work.stage,active_work.entity_type,active_work.entity_id,active_work.sport)
          AND f.input_version IS NOT DISTINCT FROM active_work.input_version)) AS unrecorded,
        count(*) FILTER(WHERE status='running' AND updated_at<now()-interval '2 hours') AS stuck
    FROM active_work
), stage_queue AS (
    SELECT stage,count(*) FILTER(WHERE status IN ('pending','failed') AND attempts<5
        AND available_at<=now()) AS ready FROM active_work GROUP BY stage
), progress AS (
    SELECT stage,max(completed_at) AS newest FROM (
        SELECT stage,completed_at FROM pipeline_stage_progress
        UNION ALL
        SELECT CASE character WHEN 'journalist' THEN 'narratives' WHEN 'influencer' THEN 'vibe'
            WHEN 'insider' THEN 'transfers' WHEN 'scout' THEN 'rating' END,updated_at
        FROM classifier_spectrum_deliveries WHERE status IN ('used','abstained','held','redundant')
    ) successful GROUP BY stage
), checks(name,status,detail) AS (
SELECT 'ingest_recency',CASE WHEN max(fetched_at)>now()-interval '26 hours' THEN 'OK' ELSE 'ALARM' END,
    'newest discovery '||coalesce(max(fetched_at)::text,'none') FROM news_articles
UNION ALL SELECT 'coverage_cycle',CASE WHEN NOT enabled THEN 'INFO'
    WHEN last_tick_at>now()-interval '2 minutes' THEN 'OK' ELSE 'ALARM' END,
    CASE WHEN enabled THEN phase||'; last tick '||coalesce(last_tick_at::text,'never') ELSE 'disabled' END
    FROM coverage_schedule
UNION ALL SELECT 'last_character_edition', CASE WHEN e.status='partial' THEN 'ALARM' ELSE 'OK' END,
    concat(e.edition_date,' ',e.status,' ',e.reason,'; blocked assignments ',
        (SELECT count(*) FROM coverage_assignments a WHERE a.edition_date=e.edition_date AND a.outcome IN ('failed','expired')))
    FROM (SELECT edition_date,status,reason FROM coverage_editions WHERE finished_at IS NOT NULL
          ORDER BY finished_at DESC,edition_date DESC LIMIT 1) e
UNION ALL SELECT 'worker_mode','INFO',:'mode'
UNION ALL SELECT 'queue_depth','INFO',ready||' claimable; retained backlog is expected' FROM queue
UNION ALL SELECT 'held_deliveries','INFO',count(*)||' held; no automatic policy promotion'
    FROM classifier_spectrum_deliveries WHERE status='held'
UNION ALL SELECT 'dead_letters',CASE WHEN unrecorded>0 THEN 'ALARM' WHEN dead>0 THEN 'INFO' ELSE 'OK' END,
    dead||' active-stage rows at attempt cap; '||unrecorded||' missing failure records' FROM queue
UNION ALL SELECT 'stuck_running',CASE WHEN stuck=0 THEN 'OK' ELSE 'ALARM' END,stuck||' active claims older than 2 hours' FROM queue
UNION ALL SELECT 'drain_alive:'||q.stage,
    CASE WHEN q.ready=0 OR p.newest>now()-interval '30 minutes' THEN 'OK' ELSE 'ALARM' END,
    q.ready||' claimable; newest successful progress '||coalesce(p.newest::text,'none')
    FROM stage_queue q LEFT JOIN progress p USING(stage) WHERE :'mode'<>'paused'
)
SELECT jsonb_agg(to_jsonb(checks)) AS health FROM checks \gset

\if :live
-- One committed snapshot, with current queues and a rolling window for volume and errors.
WITH bounds AS (SELECT now()-interval '24 hours' AS since, now() AS as_of),
decisions AS (
    SELECT count(*) AS target_decisions, count(DISTINCT (article_id,sport)) AS articles_classified,
        count(*) FILTER (WHERE gate->>'admitted'='true') AS admitted,
        count(*) FILTER (WHERE gate->>'admitted'='false') AS declined
    FROM classifier_spectra,bounds WHERE created_at BETWEEN since AND as_of
), measurements AS (
    SELECT status,count(*) AS n FROM classifier_measurements,bounds
    WHERE created_at BETWEEN since AND as_of GROUP BY status
), errors AS (
    SELECT COALESCE(NULLIF(receipt->>'error',''),'Reason not recorded') AS reason,
        max(id) AS example_measurement_id,max(created_at) AS last_seen,count(*) AS attempts
    FROM classifier_measurements,bounds WHERE status='error' AND created_at BETWEEN since AND as_of
    GROUP BY 1 ORDER BY attempts DESC,last_seen DESC,reason LIMIT 10
), exhausted AS (
    SELECT failed_at,sport,entity_id AS article_id,attempts,error AS reason
    FROM pipeline_work_failures,bounds WHERE stage='classifier' AND failed_at BETWEEN since AND as_of
), queue AS (
    SELECT status,attempts,entity_id,sport,last_error,updated_at,available_at,
        status IN ('pending','failed') AND attempts<5 AND available_at<=now()
            AND :'mode'='classifier' AND 'classifier'=ANY(regexp_split_to_array(lower(btrim(:'stages')),'\s*,\s*'))
            AND coverage_claimable(stage,entity_type,entity_id,sport) AS ready
    FROM pipeline_work WHERE stage='classifier'
), queue_totals AS (
    SELECT count(*) FILTER (WHERE ready) AS ready,
        count(*) FILTER (WHERE status IN ('pending','failed') AND attempts<5 AND NOT ready) AS waiting,
        count(*) FILTER (WHERE status='running') AS running,
        count(*) FILTER (WHERE status='failed' AND attempts>=5) AS parked
    FROM queue
), cards AS (
    SELECT c.stage,count(*) AS published,count(c.generation_claim_token) AS tracked,
        count(*) FILTER (WHERE EXISTS (SELECT FROM character_output_flags f
            WHERE f.stage=c.stage AND f.claim_token=c.generation_claim_token AND f.flag_kind='malformed_output')) AS flagged
    FROM character_watchdog_cards c,bounds WHERE c.generated_at BETWEEN since AND as_of GROUP BY c.stage
), flags AS (
    SELECT stage,reason,count(*) AS n FROM character_output_flags,bounds
    WHERE flag_kind='malformed_output' AND recorded_at BETWEEN since AND as_of GROUP BY stage,reason
), characters AS (
    SELECT stage,jsonb_build_object('published',COALESCE(published,0),'tracked',COALESCE(tracked,0),
        'flagged_cards',COALESCE(flagged,0),'parser_reasons',COALESCE(
            (SELECT jsonb_object_agg(reason,n) FROM flags WHERE flags.stage=s.stage),'{}')) AS summary
    FROM unnest(ARRAY['narratives','vibe','rating','transfers','momentum','sigil']) s(stage) LEFT JOIN cards USING(stage)
)
SELECT jsonb_build_object('as_of',as_of,'window_start',since,'checks',:'health'::jsonb,
    'classifier',jsonb_build_object(
        'queue_now',(SELECT to_jsonb(queue_totals) FROM queue_totals),
        'last_job_completed_at',(SELECT completed_at FROM pipeline_stage_progress WHERE stage='classifier'),
        'recent_queue_errors',COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY updated_at DESC,article_id,sport) FROM
            (SELECT entity_id AS article_id,sport,status,attempts,updated_at,available_at,
                    left(last_error,300) AS reason FROM queue WHERE last_error IS NOT NULL
             ORDER BY updated_at DESC,entity_id,sport LIMIT 10) e),'[]'),
        'activity_24h',(SELECT to_jsonb(decisions) FROM decisions) || jsonb_build_object(
            'deliveries_created',(SELECT count(*) FROM classifier_spectrum_deliveries WHERE created_at BETWEEN since AND as_of),
            'successful_measurements',COALESCE((SELECT n FROM measurements WHERE status='source_bound_provisional'),0),
            'failed_measurements',COALESCE((SELECT n FROM measurements WHERE status='error'),0),
            'top_measurement_errors',COALESCE((SELECT jsonb_agg(jsonb_build_object('reason',left(reason,300),
                'attempts',attempts,'last_seen',last_seen,'example_measurement_id',example_measurement_id) ORDER BY attempts DESC,last_seen DESC,reason) FROM errors),'[]'),
            'exhausted_jobs',(SELECT count(*) FROM exhausted),
            'recent_exhausted_jobs',COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY failed_at DESC,article_id,sport) FROM
                (SELECT failed_at,sport,article_id,attempts,left(reason,300) AS reason FROM exhausted
                 ORDER BY failed_at DESC,article_id,sport LIMIT 10) e),'[]'))),
    'characters_24h',(SELECT jsonb_object_agg(stage,summary) FROM characters),
    'recent_parser_flags',COALESCE((SELECT jsonb_agg(to_jsonb(f) ORDER BY recorded_at DESC,flag_id DESC) FROM
        (SELECT flag_id,recorded_at,stage,sport,entity_type,entity_id,reason,published_card_ids
         FROM character_parser_review WHERE recorded_at BETWEEN since AND as_of
         ORDER BY recorded_at DESC,flag_id DESC LIMIT 10) f),'[]')) FROM bounds;
\else
SELECT name,status,detail FROM jsonb_to_recordset(:'health'::jsonb) AS check_row(name text,status text,detail text);
\endif
COMMIT;

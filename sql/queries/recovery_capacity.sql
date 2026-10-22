-- Read-only capacity receipt. Run on an approved representative day; never infer capacity from fixtures.
BEGIN READ ONLY;
SET LOCAL statement_timeout='15s';
SELECT clock_timestamp() observed_at,stage,status,count(*) obligations,
 max(clock_timestamp()-updated_at) oldest_state_age,
 max(clock_timestamp()-available_at) FILTER(WHERE status IN ('pending','failed') AND available_at<=now()) oldest_ready_wait,
 count(*) FILTER(WHERE attempts>=5) exhausted,
 count(*) FILTER(WHERE available_at>now() AND attempts<5) backing_off,sum(attempts) failures
FROM pipeline_work GROUP BY stage,status ORDER BY stage,status;
SELECT e.edition_date,e.collection_closed_at,e.cutoff_at,e.frozen_at,e.finished_at,e.status,e.reason,
 e.frozen_at-e.collection_closed_at drain_elapsed,e.finished_at-e.frozen_at character_elapsed,
 e.missed_boundaries,
 (SELECT count(*) FROM coverage_drain_obligations d WHERE d.edition_date=e.edition_date) admitted_obligations,
 (SELECT count(*) FROM coverage_drain_obligations d WHERE d.edition_date=e.edition_date AND settled_at IS NULL) unsettled
FROM coverage_editions e ORDER BY edition_date DESC LIMIT 14;
SELECT edition_date,stage,outcome,resolution,count(*) FROM coverage_assignments
WHERE edition_date >= current_date-14 GROUP BY edition_date,stage,outcome,resolution ORDER BY edition_date,stage;
SELECT stage,edition_date,count(*) product_rows FROM (
 SELECT 'narratives' stage,candidate_edition edition_date FROM news_summaries WHERE length(btrim(body))>0
 UNION ALL SELECT 'rating',candidate_edition FROM stat_summaries WHERE length(btrim(body))>0
 UNION ALL SELECT 'vibe',candidate_edition FROM vibe_scores WHERE length(btrim(prompt))>0
 UNION ALL SELECT 'transfers',candidate_edition FROM insider_scores WHERE length(btrim(read))>0
 UNION ALL SELECT 'momentum',candidate_edition FROM momentum_summaries WHERE length(btrim(blurb))>0
 UNION ALL SELECT 'sigil',candidate_edition FROM sigil_synthesis WHERE length(btrim(reading))>0
) products WHERE edition_date>=current_date-14 GROUP BY stage,edition_date ORDER BY edition_date,stage;
SELECT edition_date,count(*) handoffs,max(attempts) worst_attempts,max(now()-created_at) oldest_age
FROM application_outbox GROUP BY edition_date;
SELECT stage,input_hash,model_version,prompt_version,count(*) recorded_generations,
 sum((context_budget->>'eval_count')::bigint) output_tokens,
 sum((context_budget->>'wall_ms')::numeric)/1000 recorded_call_seconds
FROM cognition_ledger WHERE generated_at>=now()-interval '1 day' AND request_body IS NOT NULL
GROUP BY stage,input_hash,model_version,prompt_version ORDER BY recorded_generations DESC;
-- Queue timestamps measure current state/ready wait, not lifetime age across revisions.
-- Ledger writes are best effort. Reconcile these with `character model call completed` logs,
-- which include prompt/request hashes, input/output tokens and occupied-slot time, including rejected replies.
-- The work span supplies stage, exact claim and input revision; failures have a separate call event.
-- Subtract model and publication timing from work execution time for a preparation/DB/parsing residual.
ROLLBACK;

-- 220_retire_narrative_threads.sql
--
-- ############################################################################
-- ##  PREPARED, NOT YET APPLICABLE. Lives in sql/prepared/ on purpose —     ##
-- ##  the runner only globs sql/migrations/*.sql. When the drain is green   ##
-- ##  and you mean it:                                                      ##
-- ##                                                                        ##
-- ##      cp sql/prepared/220_retire_narrative_threads.sql \                ##
-- ##         sql/migrations/220_retire_narrative_threads.sql                ##
-- ##      ./sql/migrate.sh                                                  ##
-- ##                                                                        ##
-- ##  The migration carries its own data gate and REFUSES TO APPLY unless   ##
-- ##  the story_parts Rust binary has provably drained green (>= 25         ##
-- ##  chapters written with storyline_id since the 76002d7 cutover, and     ##
-- ##  zero thread writes since). Copying it early is safe; the gate, not    ##
-- ##  the folder, is the real fence.                                        ##
-- ############################################################################
--
-- WHAT: collapse of narrative_threads into storyline parts — STEP B of two: the demolition.
-- Drops news_summaries.thread_id (FK + index + column), the narrative_threads table,
-- v_narrative_threads, the thread lifecycle functions (seal_narrative_threads,
-- promote_established_threads, narrative_thread_established_gate), and the dual-period
-- fill function (fill_news_summaries_storylines). Step A (mig 219) landed the successors:
-- storyline_entities carries progression, news_summaries.storyline_id carries the chapter
-- pointer, seal_storylines/promote_established_parts run the nightly lifecycle, and the
-- memory card reads only storylines.
--
-- GATED (the 045 habit): this migration REFUSES TO APPLY unless the Rust cutover is
-- provably live — no thread writes since the daemon restart AND at least 25 chapters
-- written with storyline_id directly by the new persist path. A rollback to the old binary
-- re-arms thread writes, and this gate is what keeps the demolition from running under it.
--
-- Deploy order: DESTRUCTIVE — apply ONLY after mig 219 + the story_parts Rust binary have
-- drained green for a cycle (the gate asserts it). The cron's fill call is to_regprocedure-
-- guarded and silently skips once the function is gone. After applying: run
-- scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

BEGIN;

-- 0. Lock discipline: news_summaries is HOT (the narratives drain writes it
--    continuously). Every statement here is metadata-only (fast), but the DROP
--    COLUMN still takes ACCESS EXCLUSIVE and would queue every writer behind a
--    long-running persist transaction. Fail loud and retry later instead of
--    stalling the drain indefinitely.
SET LOCAL lock_timeout = '30s';

-- 1. The gate. The cutover instant is the story_parts daemon's start (release of 76002d7);
--    the old binary cannot write after it.
DO $$
DECLARE
    v_cutover constant timestamptz := '2026-08-13 11:06:15+00';
    v_last_thread timestamptz;
    v_new_chapters int;
BEGIN
    SELECT max(last_progressed_at) INTO v_last_thread FROM public.narrative_threads;
    IF v_last_thread IS NOT NULL AND v_last_thread >= v_cutover THEN
        RAISE EXCEPTION '220 refused: narrative_threads was progressed at %, after the cutover (%) — is a pre-cutover binary still writing?',
            v_last_thread, v_cutover;
    END IF;

    SELECT count(*) INTO v_new_chapters
    FROM public.news_summaries
    WHERE generated_at > v_cutover
      AND narrative_title IS NOT NULL
      AND storyline_id IS NOT NULL;
    IF v_new_chapters < 25 THEN
        RAISE EXCEPTION '220 refused: only % chapter(s) written with storyline_id since the cutover (need >= 25) — let the new persist path prove itself first',
            v_new_chapters;
    END IF;

    RAISE NOTICE '220 gate passed: no thread writes since %, % chapters written on the new path',
        v_cutover, v_new_chapters;
END $$;

-- 2. The thread machinery first: promote calls the gate function, and the gate function's
--    signature carries the table's row type — so the order is promote, gate, seal. The
--    VIEW goes before the column: it joins chapters on thread_id, so dropping the column
--    first fails on the view's dependency (rehearsal 2026-08-13 measured exactly that).
DROP FUNCTION IF EXISTS public.promote_established_threads(text);
DROP FUNCTION IF EXISTS public.narrative_thread_established_gate(public.narrative_threads);
DROP FUNCTION IF EXISTS public.seal_narrative_threads(text);
DROP VIEW IF EXISTS public.v_narrative_threads;

-- 3. The chapter pointer, retargeted in mig 219 — drop the legacy column end.
ALTER TABLE public.news_summaries
    DROP CONSTRAINT IF EXISTS news_summaries_thread_id_fkey;
DROP INDEX IF EXISTS public.idx_news_summaries_thread;
ALTER TABLE public.news_summaries
    DROP COLUMN IF EXISTS thread_id;

-- 4. The table itself. A plain (non-CASCADE) drop: anything still referencing it fails
--    LOUDLY here instead of being silently swept.
DROP TABLE IF EXISTS public.narrative_threads;

-- 5. The dual-period fill — its work is done (cron guard tolerates the absence).
DROP FUNCTION IF EXISTS public.fill_news_summaries_storylines();

-- 6. Smoke gate: nothing of the thread era remains.
DO $$
BEGIN
    IF to_regclass('public.narrative_threads') IS NOT NULL THEN
        RAISE EXCEPTION '220: narrative_threads still exists';
    END IF;
    IF to_regclass('public.v_narrative_threads') IS NOT NULL THEN
        RAISE EXCEPTION '220: v_narrative_threads still exists';
    END IF;
    IF EXISTS (
        SELECT 1 FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = 'public'
          AND p.proname IN ('seal_narrative_threads', 'promote_established_threads',
                            'narrative_thread_established_gate', 'fill_news_summaries_storylines')
    ) THEN
        RAISE EXCEPTION '220: thread-era functions still present';
    END IF;
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'news_summaries' AND column_name = 'thread_id'
    ) THEN
        RAISE EXCEPTION '220: news_summaries.thread_id still exists';
    END IF;
END $$;

-- Self-record INSIDE the transaction so apply + record are atomic.
INSERT INTO public.schema_migrations(version) VALUES ('220_retire_narrative_threads')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: scripts/hosting/snapshot-schema.sh, commit sql/schema/ with this file.

-- 227_momentum_projection_refresh_off_the_write_path.sql
--
-- The `latest_momentum_scores_per_entity` projection stops being rebuilt, in full and while
-- holding an exclusive lock, from inside a trigger on the write path.
--
-- MEASURED, 2026-08-22, from the cognition journal on archbox:
--
--   slow statement: execution time exceeded alert threshold elapsed=19.141s
--   SELECT vibe_slope, vibe_samples, rating_slope, rating_samples, momentum_score
--   FROM public.latest_momentum_scores_per_entity
--   WHERE entity_type = $1 AND entity_id = $2 AND sport = $3 LIMIT 1
--
-- Nineteen seconds for a single-row lookup that has a UNIQUE btree on exactly its predicate
-- (`idx_latest_momentum_scores_per_entity_key` on (sport, entity_type, entity_id)). The query
-- is not slow; it is BLOCKED. Mig 140 hung a statement trigger on `momentum_scores` that runs
-- a plain `REFRESH MATERIALIZED VIEW`, which takes an ACCESS EXCLUSIVE lock on the projection
-- and rebuilds every row for every sport. Each maintenance drain writes `momentum_scores`, so
-- each drain freezes every reader of the projection for the length of a full rebuild — and the
-- Analyst reads it to load her own context, so the momentum stage blocks on its own pipeline's
-- writes. Multi-second stalls were recurring throughout the 2026-08-22 fleet drain.
--
-- The projection itself is right and mig 140's reasoning is right: paying the current-row cost
-- on writes beats sorting an append-only history on every hot leaderboard read. What is wrong
-- is WHERE the cost is paid. A statement trigger is the one place this refresh cannot be made
-- non-blocking, because `REFRESH MATERIALIZED VIEW CONCURRENTLY` cannot run inside a
-- transaction block and a trigger function is always inside one.
--
-- So the refresh moves to the maintenance drain that already owns this work. That drain is
-- where `momentum_refresh_needed` / `mark_momentum_refresh_needed` / NOTIFY
-- `momentum_refresh_ready` already coalesce this exact cost (mig 128), and it can issue the
-- refresh as a standalone statement, so it goes CONCURRENTLY — readers are never blocked. The
-- required UNIQUE index has existed since mig 140 and is what makes CONCURRENTLY legal here.
--
-- This is the same correction the Insider's pair loop already took: "drain after the pair loop
-- instead of one heavy REFRESH MATERIALIZED VIEW inline" (rust/src/junctions/insider/mod.rs).
-- Second time this trigger-shaped mistake has been paid for.
--
-- ORDERING: the Go binary carrying the drain-side refresh must ship FIRST. Between the release
-- and this migration the trigger simply keeps doing what it does today (correct, just
-- blocking), and after it the drain is the only writer of the projection. There is no window
-- in which the projection goes stale, which is why this is safe to apply live.

BEGIN;

DROP TRIGGER IF EXISTS refresh_latest_momentum_scores_per_entity ON public.momentum_scores;

-- The helper stays: it is the correct body, and dropping it would break any manual or
-- migration-time caller that wants a synchronous rebuild. It is simply no longer wired to the
-- write path. Its comment is rewritten so the next reader is not told it is a live trigger.
COMMENT ON FUNCTION public.refresh_latest_momentum_scores_per_entity() IS
    'Synchronous full rebuild of latest_momentum_scores_per_entity. NO LONGER TRIGGER-WIRED '
    '(mig 227): as an AFTER STATEMENT trigger on momentum_scores this held an ACCESS EXCLUSIVE '
    'lock on the projection for every write, and single-row reads against its unique index were '
    'measured blocking for 19s during the 2026-08-22 drain. The live refresh is now issued '
    'CONCURRENTLY by the maintenance drain (internal/maintenance), outside any transaction. '
    'Kept for manual/migration-time use where a synchronous rebuild is actually wanted.';

COMMENT ON MATERIALIZED VIEW public.latest_momentum_scores_per_entity IS
    'Current-row projection for momentum_scores. One row per (sport, entity_type, entity_id). '
    'Refreshed CONCURRENTLY by the maintenance drain after refresh_momentum_scores (mig 227); '
    'was a blocking statement trigger on momentum_scores from mig 140 until then.';

-- Leave the projection current for the gap between this migration and the next drain.
COMMIT;

REFRESH MATERIALIZED VIEW CONCURRENTLY public.latest_momentum_scores_per_entity;

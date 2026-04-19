-- Track per-sport X API call cost so we can see what we're spending.
--
-- counters_date is the UTC day the running totals correspond to. The
-- update path resets totals to 0 if the date has rolled. That keeps the
-- query path simple (no daily cron required) — every refresh checks the
-- date and rolls if needed before incrementing.

ALTER TABLE twitter_lists
    ADD COLUMN IF NOT EXISTS counters_date    DATE   NOT NULL DEFAULT CURRENT_DATE,
    ADD COLUMN IF NOT EXISTS calls_today      INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS tweets_today     INTEGER NOT NULL DEFAULT 0;

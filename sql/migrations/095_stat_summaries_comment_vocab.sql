-- 095_stat_summaries_comment_vocab.sql
--
-- Cosmetic: refresh the stat_summaries TABLE comment to post-convergence vocabulary.
-- The 086 comment still said "special=how" (the pre-convergence name for the peak
-- datapoint — now "peak", see divined_peak / the PEAK: marker) and credited
-- "ml/stat_commentary.go" (renamed to ml/rating.go). The 086 migration file is left
-- as historical record; this forward migration sets the current live comment.
--
-- COMMENT-only — no schema change, no prepared-statement impact, so it is safe to
-- apply without an API restart (db.New validates columns/functions, not comments).

BEGIN;

COMMENT ON TABLE stat_summaries IS
    'Append, one row per generation per entity (latest-per-entity read). The STATS-rail narrative: Gemma''s on-field IDENTITY analysis derived from the rating engine''s scrubbed datapoints (composite=how well, peak=how). notability (deterministic, distinctiveness) drives dynamic length + a board; NULL body = insufficient-stats marker. Twin of news_summaries; written by ml/rating.go.';

COMMIT;

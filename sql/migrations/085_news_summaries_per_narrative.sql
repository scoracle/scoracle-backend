-- 085_news_summaries_per_narrative.sql
--
-- Restructure news_summaries from one-summary-per-generation (081) to ONE ROW PER
-- NARRATIVE. The Gemma news stage (ml/news_narratives.go) now groups an entity's
-- vetted corpus into the DISTINCT storylines forming around it and writes each up
-- separately. A "generation" is the set of rows sharing
-- (entity_type, entity_id, sport, generated_at); reads take the latest generation,
-- hottest narrative first.
--
-- 081's table is EMPTY (no generator was ever wired live), so this is a free
-- redesign — nothing to migrate.
--
-- Changes:
--   * summary            -> body            (the narrative write-up)
--   * + narrative_title                     (the storyline headline)
--   * - trending_topics                     (redundant; each narrative IS a thread)
--   * impact stays, now PER-NARRATIVE (volume×sources×recency over THIS narrative's
--     articles) and ranks the news leaderboard — hottest narratives across entities.
--   * input_news_ids stays, now the articles that fed THIS narrative.
--   * NULL narrative_title/body = the no-narratives-this-cycle marker row.

BEGIN;

ALTER TABLE news_summaries ADD COLUMN IF NOT EXISTS narrative_title TEXT;
ALTER TABLE news_summaries RENAME COLUMN summary TO body;
ALTER TABLE news_summaries DROP COLUMN IF EXISTS trending_topics;

COMMENT ON TABLE news_summaries IS
    'Append, ONE ROW PER NARRATIVE per generation. A generation = the rows sharing (entity_type, entity_id, sport, generated_at). narrative_title + body = the write-up; per-narrative impact ranks the leaderboard; NULL narrative_title/body = a no-narratives-this-cycle marker. Written by ml/news_narratives.go.';
COMMENT ON COLUMN news_summaries.narrative_title IS
    'Storyline headline (e.g. "Cucurella to Real Madrid saga"); NULL = no-narratives marker row.';
COMMENT ON COLUMN news_summaries.body IS
    'The narrative write-up (original prose); NULL = no-narratives marker row.';
COMMENT ON COLUMN news_summaries.impact IS
    'Per-narrative deterministic impact 0-100 (volume×sources×recency over this narrative''s articles); ranks the news leaderboard.';

-- News leaderboard: a sport's hottest NARRATIVES, newest first. Re-create on the
-- renamed `body` column (only real narratives qualify).
DROP INDEX IF EXISTS idx_news_summaries_sport_impact;
CREATE INDEX IF NOT EXISTS idx_news_summaries_sport_impact
    ON news_summaries(sport, impact DESC, generated_at DESC)
    WHERE body IS NOT NULL AND impact IS NOT NULL;

COMMIT;

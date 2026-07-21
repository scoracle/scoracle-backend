-- 171_news_article_full_text.sql
--
-- Cognition refactor, Phase 0 (groundwork, no behavior change): add the nullable
-- `news_articles.full_text` column so a later corpus fetch can slot in WITHOUT a schema
-- rewrite (plan decision 3, "defer full-text, design for it"). Nothing populates this
-- column yet — every row stays NULL until an article-body fetcher exists. The Rust corpus
-- loader (narratives.rs) already prefers `full_text` when present and falls back to
-- title+description when NULL, so the seam is live but inert.
--
-- Deploy order: ADDITIVE — apply this BEFORE deploying the binary that SELECTs a.full_text
-- (db/query prep validates columns against the live schema). No API restart needed for the
-- Go serving path; it never reads this column.

BEGIN;

ALTER TABLE public.news_articles
    ADD COLUMN IF NOT EXISTS full_text text;

COMMENT ON COLUMN public.news_articles.full_text IS
    'Cognition refactor Phase 0 (mig 171): nullable article body for the corpus loader. '
    'NULL = not fetched (the only state today); the loader falls back to title+description. '
    'Reserved for a later provider/full-text fetch; no column semantics change when it lands.';

INSERT INTO public.schema_migrations(version) VALUES ('171_news_article_full_text')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

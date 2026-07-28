-- 195_pg_trgm.sql
--
-- WHAT: installs the `pg_trgm` extension.
--
-- WHY: the news rail has no cheap similarity primitive. Two measured consequences:
--   * `news_articles.duplicate_of` is set on 1,474 of 56,873 articles in a 7-day window (2.6%),
--     while a hand count of one heavy team-day (Real Madrid, 2026-07-26) found byte-identical
--     titles from different sources going unmarked. Exact-title equality is the only tool we have.
--   * The storyline work (PLAN-ingest-simplification.md, "The turn") needs to attach the ~79% of
--     articles that never earn a model read to stories that already exist, WITHOUT spending a model
--     call on each. Title similarity is the intended mechanism and it does not exist yet.
--
-- ADDITIVE and inert on its own: this migration only makes the operators available. It creates no
-- index and changes no query plan, so nothing behaves differently until a caller opts in. Indexes
-- are deliberately deferred until the dedup/attachment thresholds are measured — a GIN trgm index
-- on `news_articles.title` is ~the size of the column and should not be paid for on spec.
--
-- `unaccent` is already installed (1.1) and is NOT touched here.
--
-- Deploy order: additive, applies before the API restart.

BEGIN;

CREATE EXTENSION IF NOT EXISTS pg_trgm;

INSERT INTO public.schema_migrations(version) VALUES ('195_pg_trgm')
    ON CONFLICT DO NOTHING;

COMMIT;

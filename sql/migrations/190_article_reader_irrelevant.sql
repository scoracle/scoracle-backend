-- 190_article_reader_irrelevant.sql
--
-- Article Reader v2 can reject an article after fetching full text when the
-- RSS hit passed Candle but the publisher body is not materially about any
-- vetted entity. The Rust stage records that as a terminal current reading and
-- clears the article's vetted links for the sport, so Narratives does not see
-- the false-positive article.

BEGIN;

ALTER TABLE public.news_article_readings
    DROP CONSTRAINT IF EXISTS news_article_readings_status_check;

ALTER TABLE public.news_article_readings
    ADD CONSTRAINT news_article_readings_status_check
    CHECK (status IN (
        'success',
        'irrelevant',
        'not_allowlisted',
        'duplicate',
        'no_vetted_entities',
        'paywall',
        'blocked',
        'empty_body',
        'fetch_failed',
        'parse_failed'
    ));

INSERT INTO public.schema_migrations(version) VALUES ('190_article_reader_irrelevant')
    ON CONFLICT DO NOTHING;

COMMIT;

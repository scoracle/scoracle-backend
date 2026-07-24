-- 188_article_readings.sql
--
-- Article Reader v1: insert a Rust-owned article_read stage between Candle scrub and
-- Narratives. Scrub still proves relevance. The new stage fetches the actual publisher page
-- for vetted, canonical, allowlisted articles and persists a compact evidence blurb/card that
-- Narratives can read instead of relying on RSS headline snippets.
--
-- Deploy order: ship the binary that knows article_read, then apply this trigger migration.
-- Once applied, vetted links enqueue article_read instead of narratives; article_read is
-- responsible for enqueueing narratives after success or terminal fallback.

BEGIN;

CREATE TABLE IF NOT EXISTS public.news_article_readings (
    article_id        bigint PRIMARY KEY REFERENCES public.news_articles(id) ON DELETE CASCADE,
    status            text NOT NULL CHECK (status IN (
        'success',
        'not_allowlisted',
        'duplicate',
        'no_vetted_entities',
        'paywall',
        'blocked',
        'empty_body',
        'fetch_failed',
        'parse_failed'
    )),
    final_url         text,
    final_domain      text,
    content_hash      text,
    extracted_words   integer NOT NULL DEFAULT 0 CHECK (extracted_words >= 0),
    evidence_blurb    text,
    evidence          jsonb NOT NULL DEFAULT '{}'::jsonb,
    model_version     text,
    prompt_version    text,
    parser_outcome    text NOT NULL DEFAULT 'no_call',
    last_error        text,
    fetched_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.news_article_readings IS
    'Article Reader v1: one current evidence card per news article, written by the Rust '
    'article_read stage after Candle scrub proves relevance. Stores compact model evidence '
    'and fetch provenance, not a raw article-body archive.';

COMMENT ON COLUMN public.news_article_readings.evidence_blurb IS
    'Compact source-grounded blurb rendered into the Narratives prompt. NULL means the '
    'Journalist falls back to the RSS description.';

COMMENT ON COLUMN public.news_article_readings.content_hash IS
    'SHA-256 prefix over cleaned fetched article text. Used with status/model fields as the '
    'Narratives debounce fingerprint so richer article reads reopen the Journalist.';

CREATE INDEX IF NOT EXISTS idx_news_article_readings_status
    ON public.news_article_readings(status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_news_article_readings_domain
    ON public.news_article_readings(final_domain)
    WHERE final_domain IS NOT NULL;

CREATE TABLE IF NOT EXISTS public.data_fetch_ledger (
    id                      bigserial PRIMARY KEY,
    target_type             text NOT NULL,
    target_id               bigint NOT NULL,
    sport                   text,
    stage                   text NOT NULL,
    status                  text NOT NULL,
    source_url              text,
    final_url               text,
    final_domain            text,
    content_hash            text,
    model_version           text,
    prompt_version          text,
    output_contract_version text,
    parser_outcome          text,
    error                   text,
    generated_at            timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.data_fetch_ledger IS
    'Append-only provenance for browser/fetch-backed data enrichment stages. Article Reader v1 '
    'records one row per fetch attempt so the current news_article_readings card can stay compact.';

CREATE INDEX IF NOT EXISTS idx_data_fetch_ledger_target
    ON public.data_fetch_ledger(target_type, target_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_data_fetch_ledger_stage_status
    ON public.data_fetch_ledger(stage, status, generated_at DESC);

-- Future-proof the shared queue for the deferred fixture-keyed boxscore_shadow stage.
ALTER TABLE public.pipeline_work DROP CONSTRAINT IF EXISTS pipeline_work_entity_type_check;
ALTER TABLE public.pipeline_work ADD CONSTRAINT pipeline_work_entity_type_check
    CHECK (entity_type IN ('player', 'team', 'article', 'fixture'));

CREATE OR REPLACE FUNCTION public.enqueue_derive_on_vetted() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_graph_count integer;
BEGIN
    -- Per-ARTICLE graph extraction stays directly downstream of scrub.
    SELECT count(*) INTO v_graph_count
      FROM public.news_article_entities
     WHERE article_id = NEW.article_id AND vetted IS TRUE;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    VALUES ('graph', 'article', NEW.article_id::integer, NEW.sport, 'pending',
            'g:' || v_graph_count, NOW(), NOW())
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Article Reader is the new layer between scrub and Narratives. It will enqueue
    -- narratives for this article's vetted entities after it writes a success or terminal
    -- fallback row, so The Journalist sees richer evidence when available and still falls
    -- back cleanly when a body cannot be fetched.
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    VALUES ('article_read', 'article', NEW.article_id::integer, NEW.sport, 'pending',
            'ar:' || v_graph_count, NOW(), NOW())
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN NEW;
END;
$$;

INSERT INTO public.schema_migrations(version) VALUES ('188_article_readings')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

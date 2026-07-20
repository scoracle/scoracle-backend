-- 165_graph_stage.sql
--
-- Junction memory rollout step 5b: the `graph` queue stage wires in — AFTER the
-- fixture gate (fixtures/graph/, 12/12 property checks at g2 on 2026-07-19: object
-- attachment, person discovery, over-extraction, unary discipline).
--
-- (1) `graph_extractions` — per-article bookkeeping: the stage's debounce anchor
--     (input_hash over title+description+vetted-candidate identity, plus the prompt
--     contract version) and the observability surface (parser outcome + counts).
--     A fail-closed reply records here too, so identical bytes never re-hammer the
--     GPU; a prompt bump re-extracts everything once (the rating precedent).
--     ON DELETE CASCADE is correct here: this is bookkeeping, NOT memory — the
--     durable products (narrative_events, persons) deliberately survive rail pruning.
--
-- (2) enqueue_derive_on_vetted() learns the per-ARTICLE graph enqueue: one 'graph'
--     item per article (entity_type='article', the scrub convention), reopened when
--     the article's vetted-entity membership grows. Placed BEFORE the per-entity 72h
--     freshness gate: a backfilled old article still deserves extraction (events are
--     durable memory — the episode-backfill precedent), even when its entity's fresh
--     corpus count is 0.
--
-- Deploy order: apply BEFORE deploying the graph-stage cognition binary (the stage
-- name needs no pipeline_work change — stage has no CHECK constraint).

BEGIN;

CREATE TABLE IF NOT EXISTS public.graph_extractions (
    article_id      BIGINT PRIMARY KEY REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    input_hash      TEXT NOT NULL,
    prompt_version  TEXT NOT NULL,
    model_version   TEXT NOT NULL,
    parser_outcome  TEXT NOT NULL,
    relations_n     INTEGER NOT NULL DEFAULT 0,
    persons_n       INTEGER NOT NULL DEFAULT 0,
    extracted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT graph_extractions_outcome_check CHECK (
        parser_outcome IN ('extracted', 'failed_closed')
    )
);

COMMENT ON TABLE public.graph_extractions IS
    'Per-article bookkeeping for the graph extraction stage: debounce anchor '
    '(input_hash + prompt_version) and observability (outcome, counts). Bookkeeping, '
    'not memory — narrative_events/persons are the durable products.';

CREATE OR REPLACE FUNCTION public.enqueue_derive_on_vetted() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_count       integer;
    v_fp          text;
    v_ver         text;
    v_stages      text[] := ARRAY['narratives', 'vibe'];
    v_graph_count integer;
BEGIN
    -- Per-ARTICLE graph extraction (mig 165): one item per article, reopened when the
    -- article's vetted membership changes; the handler debounces on material hash, so
    -- reopens are cheap. BEFORE the freshness gate below — extraction of a backfilled
    -- old article is durable-memory work the 72h corpus gate must not suppress.
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

    -- Membership fingerprint over this entity's vetted, in-lookback (72h) links. Doubles as the
    -- mig-103 freshness gate: a stale backlog primary with no fresh vetted corpus yields count=0
    -- and enqueues nothing. One link row per (article, entity) by PK, so no DISTINCT needed;
    -- ORDER BY makes the aggregate deterministic.
    SELECT count(*),
           md5(string_agg(nae.article_id::text, ',' ORDER BY nae.article_id))
      INTO v_count, v_fp
      FROM public.news_article_entities nae
      JOIN public.news_articles a ON a.id = nae.article_id
     WHERE nae.entity_type = NEW.entity_type
       AND nae.entity_id   = NEW.entity_id
       AND nae.sport       = NEW.sport
       AND nae.vetted IS TRUE
       AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours');

    IF v_count = 0 THEN
        RETURN NEW;
    END IF;

    v_ver := v_count || ':' || v_fp;

    IF NEW.entity_type = 'team' THEN
        v_stages := ARRAY['transfers', 'narratives', 'vibe'];
    END IF;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT st, NEW.entity_type, NEW.entity_id, NEW.sport, 'pending', v_ver, NOW(), NOW()
      FROM unnest(v_stages) AS st
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Redundant with the mig-133 statement-level pipeline_work notify trigger, but kept for the
    -- minimal diff: pg_notify de-dups identical (channel,payload) within a txn, so this costs
    -- nothing extra.
    PERFORM pg_notify('pipeline_work_ready', '');

    RETURN NEW;
END;
$$;

INSERT INTO public.schema_migrations(version) VALUES ('165_graph_stage')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

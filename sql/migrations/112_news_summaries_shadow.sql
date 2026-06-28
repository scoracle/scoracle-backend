-- 112_news_summaries_shadow.sql  (Rust Cognition Harness L13 — offline narratives parity harness)
--
-- Standalone diagnostic table for the NARRATIVES port parity proof. It shadows
-- news_summaries without touching the live read path: the Rust harness and the Go
-- dump both write here, then the gate diffs deterministic axes only.
--
-- PARITY AXES (L2 finding):
--   built_prompt   — exact user prompt bytes assembled from a fixed corpus.
--   ollama_request — exact /api/generate body, including the verbatim n3 system prompt.
--   model_version  — configured EmotionalNews model.
--   prompt_version — n3.
--
-- Narrative prose is NOT a parity axis. The Rust live path may also dedup the
-- corpus with candle embed+cluster before building this prompt; that is a
-- deliberate value-add and is disabled in the offline parity harness by using a
-- Harness with embedder = None.

CREATE TABLE IF NOT EXISTS news_summaries_shadow (
    id                    bigserial   PRIMARY KEY,
    source                text        NOT NULL DEFAULT 'rust'
                                      CHECK (source IN ('rust', 'go')),
    entity_type           text        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id             integer     NOT NULL,
    sport                 text        NOT NULL,
    trigger_type          text        NOT NULL,
    trigger_payload       jsonb       NOT NULL DEFAULT 'null'::jsonb,

    -- Verdict columns — inspection only; model prose is not a temp-0 parity axis.
    narrative_title       text,
    body                  text,
    impact                smallint    CHECK (impact IS NULL OR impact BETWEEN 0 AND 100),
    impact_components     jsonb       NOT NULL DEFAULT '{}'::jsonb,
    input_news_ids        bigint[]    NOT NULL DEFAULT '{}'::bigint[],

    -- Corpus inspection. In the parity harness original = deduped because embedder is None.
    original_corpus_size  integer,
    deduped_corpus_size   integer,

    model_version         text,
    prompt_version        text        NOT NULL,

    temperature           real        NOT NULL,
    built_prompt          text,
    ollama_request        jsonb,
    generated_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_news_summaries_shadow_entity
    ON news_summaries_shadow (entity_type, entity_id, sport, source, generated_at DESC);

COMMENT ON TABLE news_summaries_shadow IS
    'Rust Cognition Harness L13 narratives parity harness — offline shadow of news_summaries; '
    'holds source=rust and source=go rows. Diff built_prompt + ollama_request + model_version + '
    'prompt_version. Drop after narratives cutover.';

-- 105_vibe_scores_shadow.sql  (Rust scrubber Phase 1 — offline vibe parity harness)
--
-- A standalone shadow/diagnostic table for the Rust scrubber's vibe-stage parity
-- proof (scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md §3 Phase 1).
--
-- WHY a separate table, not a column on vibe_scores: the parity harness must be
-- unable to perturb the live read path. It writes ONLY here; it never INSERTs into
-- vibe_scores and never claims pipeline_work. Go's drainVibe stays the live vibe
-- owner until parity is proven (per-stage cutover is Phase 2). This table carries
-- BOTH sides of the diff — source='rust' rows from the Rust harness and source='go'
-- rows from the Go temp-0 dumper — so the proof is a single self-join here.
--
-- Columns mirror vibe_scores (the parity contract) plus four harness-only columns:
--   source         — which implementation wrote the row ('rust' | 'go')
--   temperature    — the explicit sampling temperature used (0 for the parity run;
--                    NB both the Go and Rust Ollama clients OMIT temperature when
--                    <=0, which un-pins it to Ollama's non-deterministic default —
--                    the harness sends an EXPLICIT 0 so temp-0 diffs are exact)
--   built_prompt   — the exact user prompt sent to Ollama (for a byte-level diff)
--   ollama_request — the exact /api/generate request body (the wire is the only
--                    coupling per the plan §2; a byte-equal body ⇒ equal temp-0 output)
--
-- Deliberately NO foreign key to sports and NO trigger: this is throwaway scaffolding
-- for the Phase 1 soak, dropped once vibe is cut over. Keeping it constraint-light
-- guarantees it cannot fail-closed against live reference data or fire NOTIFYs.

CREATE TABLE IF NOT EXISTS vibe_scores_shadow (
    id              bigserial PRIMARY KEY,
    source          text        NOT NULL DEFAULT 'rust'
                                CHECK (source IN ('rust', 'go')),
    entity_type     text        NOT NULL
                                CHECK (entity_type = ANY (ARRAY['player', 'team'])),
    entity_id       integer     NOT NULL,
    sport           text        NOT NULL,
    trigger_type    text        NOT NULL,
    trigger_payload jsonb       NOT NULL DEFAULT '{}'::jsonb,
    sentiment       smallint    CHECK (sentiment IS NULL OR (sentiment >= 1 AND sentiment <= 100)),
    prompt          text,
    input_news_ids  bigint[]    NOT NULL DEFAULT '{}'::bigint[],
    model_version   text        NOT NULL,
    prompt_version  text        NOT NULL,
    temperature     real        NOT NULL,
    built_prompt    text,
    ollama_request  jsonb,
    generated_at    timestamptz NOT NULL DEFAULT now()
);

-- One natural lookup: latest row per (entity, source) — used by the diff self-join.
CREATE INDEX IF NOT EXISTS idx_vibe_scores_shadow_entity
    ON vibe_scores_shadow (entity_type, entity_id, sport, source, generated_at DESC);

COMMENT ON TABLE vibe_scores_shadow IS
    'Rust-scrubber Phase 1 vibe parity harness — offline shadow of vibe_scores; '
    'holds source=rust and source=go temp-0 rows for the parity diff. Drop after vibe cutover.';

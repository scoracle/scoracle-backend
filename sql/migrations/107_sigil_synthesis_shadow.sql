-- 107_sigil_synthesis_shadow.sql  (Rust Cognition Harness L3 — offline sigil parity harness)
--
-- A standalone shadow/diagnostic table for the Rust Cognition Harness's sigil-stage
-- parity proof (scoracle-wiki/wiki/Plan - Rust Cognition Harness build.md §3 gate, L3).
-- The twin of mig 105 (vibe_scores_shadow), pointed at the sigil convergence.
--
-- WHY a separate table, not columns on sigil_synthesis: the parity harness must be
-- unable to perturb the live read path. It writes ONLY here; it never INSERTs into
-- sigil_synthesis and never claims pipeline_work. Go's drainSigil stays the live sigil
-- owner until parity is proven (per-stage cutover is the next step). This table carries
-- BOTH sides of the diff — source='rust' rows from the Rust harness (bin/sigil_parity)
-- and source='go' rows from the Go temp-0 dumper (TestSigilParityDump) — so the proof
-- is a single self-join here.
--
-- Columns mirror the sigil_synthesis derivation contract plus four harness-only columns:
--   source         — which implementation wrote the row ('rust' | 'go')
--   temperature    — the explicit sampling temperature used (0 for the parity run;
--                    NB both clients OMIT temperature when <=0, un-pinning it to
--                    Ollama's non-deterministic default — the harness sends an EXPLICIT
--                    0 so temp-0 diffs are exact)
--   built_prompt   — the exact user prompt sent to Ollama (for a byte-level diff)
--   ollama_request — the exact /api/generate request body (the wire is the only
--                    coupling per the plan §2; a byte-equal body ⇒ equal temp-0 output)
--
-- input_hash is the 4th DETERMINISTIC parity axis (a pure SHA-256 of the canonical
-- pillar-component JSON — no model call), so it is compared byte-for-byte exactly like
-- built_prompt / ollama_request. SCORE/BLURB are NOT a regression signal across model
-- loads (local-model:tag temp-0 is not reliably deterministic — L2 FINDING); compare those
-- only within ONE model-load window.
--
-- previous_score is deliberately OMITTED — it is a display-only delta (last crown vs
-- this one), not part of the derivation, so it has no bearing on parity.
--
-- Deliberately NO foreign key to sports and NO trigger: throwaway scaffolding for the
-- L3 soak, dropped once sigil is cut over. Constraint-light so it cannot fail-closed
-- against live reference data or fire NOTIFYs.

CREATE TABLE IF NOT EXISTS sigil_synthesis_shadow (
    id              bigserial PRIMARY KEY,
    source          text        NOT NULL DEFAULT 'rust'
                                CHECK (source IN ('rust', 'go')),
    entity_type     text        NOT NULL
                                CHECK (entity_type = ANY (ARRAY['player', 'team'])),
    entity_id       integer     NOT NULL,
    sport           text        NOT NULL,
    season          integer,
    trigger_type    text        NOT NULL,
    trigger_payload jsonb       NOT NULL DEFAULT '{}'::jsonb,
    score           smallint    CHECK (score IS NULL OR (score >= 1 AND score <= 100)),
    blurb           text,
    input_components jsonb      NOT NULL DEFAULT '{}'::jsonb,
    input_hash      text,
    model_version   text        NOT NULL,
    prompt_version  text        NOT NULL,
    temperature     real        NOT NULL,
    built_prompt    text,
    ollama_request  jsonb,
    generated_at    timestamptz NOT NULL DEFAULT now()
);

-- One natural lookup: latest row per (entity-season, source) — used by the diff self-join.
CREATE INDEX IF NOT EXISTS idx_sigil_synthesis_shadow_entity
    ON sigil_synthesis_shadow (entity_type, entity_id, sport, season, source, generated_at DESC);

COMMENT ON TABLE sigil_synthesis_shadow IS
    'Rust Cognition Harness L3 sigil parity harness — offline shadow of sigil_synthesis; '
    'holds source=rust and source=go temp-0 rows for the parity diff. Drop after sigil cutover.';

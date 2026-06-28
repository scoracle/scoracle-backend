-- 110_transfer_rumors_shadow.sql  (Rust Cognition Harness L11 — offline transfers parity harness)
--
-- A standalone shadow/diagnostic table for the Rust Cognition Harness's transfers-stage parity
-- proof (scoracleWiki/wiki/Plan - Rust Cognition Harness build.md → "The Cutover Plan" Step 2, L11).
-- The triplet of mig 105 (vibe_scores_shadow) / 107 (sigil_synthesis_shadow), pointed at the
-- team-keyed transfer/trade vetting — keyed per (team, player) PAIR, the grain transfers runs at.
--
-- WHY a separate table, not columns on transfer_rumors: the parity harness must be unable to
-- perturb the live read path. It writes ONLY here; it never INSERTs into transfer_rumors and never
-- claims pipeline_work. Go's drainTransfers stays the live transfers owner until the cutover
-- (Step 3). This table carries BOTH sides of the diff — source='rust' rows from the Rust harness
-- (bin/transfer_parity) and source='go' rows from the Go dump (TestTransferParityDump) — so the
-- proof is a single self-join here.
--
-- THE PARITY AXES (the L2 finding — deterministic only, no model call needed):
--   built_prompt   — the exact USER prompt assembled (the machinery axis; SQL loaders + assembly).
--                    Byte-equal across rust/go ⇒ the loaders agree on candidate identity, the
--                    deterministic relationship, and the news set/order, AND build_transfer_prompt
--                    matches Go's buildTransferPrompt byte-for-byte.
--   ollama_request — the exact /api/generate body. Byte-equal EXCEPT the `system` field, which is
--                    the ONE intended divergence: Go ships the frozen t3 system prompt; Rust ships
--                    t4 (the roundup/listicle clause + never-invent-a-fee — the L9 false-heat root
--                    fix, single-homed in Rust). The diff is `ollama_request - 'system'`.
--   model_version  — the configured model (mistral:7b both sides).
--   prompt_version — go='t3', rust='t4' (the deliberate single-home bump).
--
-- The is_rumor/direction/stage/summary/confidence VERDICT columns are NOT a parity axis (the model
-- output is not reliably deterministic at temp 0 across loads — the L2 finding); they are recorded
-- for INSPECTION (eyeballing that t4 actually clears roundups / never invents a fee), never diffed.
-- confidence is `real` here (not the live table's numeric(3,2)) — it is display-only diagnostic.
--
-- Deliberately NO foreign key and NO trigger: throwaway scaffolding for the L11 soak, dropped once
-- transfers is cut over. Constraint-light so it cannot fail-closed against live reference data or
-- fire NOTIFYs.

CREATE TABLE IF NOT EXISTS transfer_rumors_shadow (
    id                 bigserial   PRIMARY KEY,
    source             text        NOT NULL DEFAULT 'rust'
                                   CHECK (source IN ('rust', 'go')),
    team_id            integer     NOT NULL,
    player_id          integer     NOT NULL,
    sport              text        NOT NULL,
    trigger_type       text        NOT NULL,
    trigger_payload    jsonb       NOT NULL DEFAULT '{}'::jsonb,

    -- Deterministic heat inputs (from compute_transfer_heat) — recorded for inspection.
    heat               smallint    CHECK (heat IS NULL OR heat BETWEEN 0 AND 100),
    heat_components    jsonb       NOT NULL DEFAULT '{}'::jsonb,

    -- Verdict columns — INSPECTION ONLY (never diffed; model output is not a temp-0 parity axis).
    is_rumor           boolean,
    direction          text,
    stage              text,
    gemma_summary      text,
    source_attribution text,
    confidence         real,

    input_news_ids     bigint[]    NOT NULL DEFAULT '{}'::bigint[],
    model_version      text,
    prompt_version     text        NOT NULL,

    -- Harness-only parity columns.
    temperature        real        NOT NULL,
    built_prompt       text,
    ollama_request     jsonb,
    generated_at       timestamptz NOT NULL DEFAULT now()
);

-- One natural lookup: latest row per (pair, source) — used by the diff self-join.
CREATE INDEX IF NOT EXISTS idx_transfer_rumors_shadow_pair
    ON transfer_rumors_shadow (team_id, player_id, sport, source, generated_at DESC);

COMMENT ON TABLE transfer_rumors_shadow IS
    'Rust Cognition Harness L11 transfers parity harness — offline shadow of transfer_rumors; '
    'holds source=rust (t4) and source=go (t3) rows. Diff built_prompt + (ollama_request - system). '
    'Drop after transfers cutover.';

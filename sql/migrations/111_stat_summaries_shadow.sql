-- 111_stat_summaries_shadow.sql  (Rust Cognition Harness L12 — offline rating parity harness)
--
-- A standalone shadow/diagnostic table for the Rust Cognition Harness's RATING stage parity proof
-- (scoracleWiki/wiki/Plan - Rust Cognition Harness build.md → "The Cutover Plan" Step 2, L12).
-- The quadruplet of mig 105 (vibe_scores_shadow) / 107 (sigil_synthesis_shadow) /
-- 110 (transfer_rumors_shadow), pointed at the stats-rail on-field-IDENTITY commentary — the
-- entity-keyed batch (cmd/statcommentary), NOT a pipeline_work queue stage.
--
-- WHY a separate table, not columns on stat_summaries: the parity harness must be unable to perturb
-- the live read path. It writes ONLY here; it never INSERTs into stat_summaries and never claims
-- pipeline_work. Go's cmd/statcommentary batch stays the live rating owner until the cutover (Step
-- 3, a Rust batch bin). This table carries BOTH sides of the diff — source='rust' rows from the Rust
-- harness (bin/rating_parity) and source='go' rows from the Go dump (TestRatingParityDump) — so the
-- proof is a single self-join here.
--
-- THE PARITY AXES (the L2 finding — deterministic only, no model call needed):
--   built_prompt   — the exact USER prompt assembled (the machinery axis; SQL loader + the
--                    deterministic notability/pctBand/trimFloat/ordered-facts assembly). Byte-equal
--                    across rust/go ⇒ the loader agrees on the rating profile AND build_stat_prompt
--                    matches Go's buildStatPrompt byte-for-byte (including the verbalized TIER labels
--                    pctBand emits — the L8 breakthrough: the model verbalizes the labeled tier, it
--                    never maps percentile→quality itself).
--   ollama_request — the exact /api/generate body. Byte-equal INCLUDING `system`: rating is a
--                    FAITHFUL port (no t4-style single-home divergence), so the s6 system prompt is
--                    carried verbatim. The whole jsonb must match (the cleaner gate vs transfers).
--   model_version  — the configured model (mistral:7b both sides; StatsLogic role).
--   prompt_version — 's6' both sides (verbatim port — no bump).
--   input_hash     — SHA-256(128-bit) of the canonical input-components JSON. The 5th axis transfers
--                    lacked: rating debounces on it (the nightly "work only on new data" gate), so it
--                    must reproduce Go's hashComponents byte-for-byte (Go encoding/json: sorted keys,
--                    HTML-escaped strings, shortest float form) or the cutover would spuriously regen.
--
-- The body / divined_peak / notability VERDICT columns are NOT a parity axis (the prose is not
-- reliably deterministic at temp 0 across model loads — the L2 finding); they are recorded for
-- INSPECTION (eyeballing that the model verbalizes the labeled tier / writes ~3 short paras), never
-- diffed. notability IS implicitly gated (it is rendered into built_prompt). input_components is the
-- jsonb form of the hash pre-image (debug; jsonb re-normalizes so it is NOT the byte-exact pre-image
-- — input_hash is).
--
-- Deliberately NO foreign key and NO trigger: throwaway scaffolding for the L12 soak, dropped once
-- rating is cut over. Constraint-light so it cannot fail-closed against live reference data or fire
-- NOTIFYs.

CREATE TABLE IF NOT EXISTS stat_summaries_shadow (
    id                    bigserial   PRIMARY KEY,
    source                text        NOT NULL DEFAULT 'rust'
                                      CHECK (source IN ('rust', 'go')),
    entity_type           text        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id             integer     NOT NULL,
    sport                 text        NOT NULL,
    season                integer,
    trigger_type          text        NOT NULL,
    trigger_payload       jsonb       NOT NULL DEFAULT '{}'::jsonb,

    -- Verdict columns — INSPECTION ONLY (never diffed; the prose is not a temp-0 parity axis).
    body                  text,
    divined_peak          text,
    notability            smallint    CHECK (notability IS NULL OR notability BETWEEN 0 AND 100),
    notability_components jsonb       NOT NULL DEFAULT '{}'::jsonb,

    -- The canonical input-components JSON (debug; the byte-exact pre-image of input_hash is the
    -- string hashed, NOT this jsonb — jsonb re-normalizes key order/float form).
    input_components      jsonb       NOT NULL DEFAULT '{}'::jsonb,
    -- PARITY AXIS: SHA-256(128-bit hex) of the canonical components JSON (Go's hashComponents).
    input_hash            text,

    model_version         text,
    prompt_version        text        NOT NULL,

    -- Harness-only parity columns.
    temperature           real        NOT NULL,
    built_prompt          text,
    ollama_request        jsonb,
    generated_at          timestamptz NOT NULL DEFAULT now()
);

-- One natural lookup: latest row per (entity-season, source) — used by the diff self-join.
CREATE INDEX IF NOT EXISTS idx_stat_summaries_shadow_entity
    ON stat_summaries_shadow (entity_type, entity_id, sport, season, source, generated_at DESC);

COMMENT ON TABLE stat_summaries_shadow IS
    'Rust Cognition Harness L12 rating parity harness — offline shadow of stat_summaries; holds '
    'source=rust and source=go rows. Diff built_prompt + ollama_request (whole jsonb) + model_version '
    '+ prompt_version + input_hash. Drop after rating cutover.';

-- 108_resolve_shadow.sql  (Rust Cognition Harness L5 — embedding-Resolve at-scale shadow)
--
-- The offline shadow for the hybrid embedding-Resolve gate (Plan §1.3, made real in
-- L4 — rust/src/resolve.rs). L4 proved the gate works end-to-end but on only 43
-- candidates; L5 runs it over the WHOLE labeled secondary-link corpus (thousands) and
-- records its verdict beside local model's production `vetted` here, so we can measure agreement
-- AT SCALE and settle the Álvarez current-club-lag FN BEFORE any live wiring. The cousin
-- of mig 105 (vibe_scores_shadow) / 107 (sigil_synthesis_shadow), pointed at the scrub gate.
--
-- WHY a separate table, not columns on news_article_entities: the shadow must be UNABLE to
-- perturb the live pipeline. The bin (bin/resolve_shadow) writes ONLY here; it never UPDATEs
-- news_article_entities.vetted (so the mig-103 `AFTER UPDATE OF vetted` enqueue trigger never
-- fires off it) and never claims pipeline_work. Go's maintenance scrub stays the live owner of
-- the vetted flag until the per-stage cutover (a later, flag-gated increment). READ-ONLY on the
-- pipeline is the whole point of this increment.
--
-- One row per (article, secondary candidate link) actually scored by the gate. Columns:
--   model_vetted    — the production label this is validated against (news_article_entities.vetted)
--   cosine          — the article↔identity-card cosine the cheap CPU pre-filter banded on
--   band            — the pre-filter decision: keep / drop (auto, no model) | ambiguous (→ local model)
--   auto_verdict    — what the band ALONE decides (keep⇒true; drop/ambiguous-placeholder⇒false).
--                     For keep/drop links this EQUALS the hybrid verdict with ZERO local model — so the
--                     auto-decide safety metric (auto-keep precision, auto-drop FN rate) is a
--                     full-universe number that needs no GPU.
--   hybrid_verdict  — the FINAL resolve_set verdict. = auto_verdict for keep/drop links; the local model
--                     adjudication for ambiguous links that were sent to the model; NULL for
--                     ambiguous links left un-adjudicated this run (decided_by='pending').
--   decided_by      — provenance of hybrid_verdict: keep_band | drop_band | model | pending
--   in_transfer_rumors / career_teams — the recent-mover diagnostics (transfer_rumors membership =
--                     active club flux; distinct player_stats teams = has-moved-ever). The Álvarez FN
--                     is a recent mover whose current_club identity lags the article; these slice the
--                     FN set so the next increment can choose: widen the drop band for movers, or
--                     refresh the identity card.
--   keep_threshold / drop_threshold — the COGNITION_RESOLVE_* band this run used (so re-runs at a
--                     different band are distinguishable and the bin can scope a fresh run by band).
--   embed_model     — the COGNITION_EMBED_MODEL the cosine came from.
--   model_version   — the adjudicator model id when decided_by='model' (else NULL).
--
-- Deliberately NO foreign key and NO trigger: throwaway scaffolding for the L5 scale soak,
-- dropped once the scrub gate is cut over. Constraint-light so it cannot fail-closed against
-- live reference data or fire NOTIFYs.

CREATE TABLE IF NOT EXISTS resolve_shadow (
    id                 bigserial   PRIMARY KEY,
    article_id         bigint      NOT NULL,
    sport              text        NOT NULL,
    entity_type        text        NOT NULL
                                   CHECK (entity_type = ANY (ARRAY['player', 'team'])),
    entity_id          integer     NOT NULL,
    name               text        NOT NULL DEFAULT '',
    model_vetted       boolean     NOT NULL,
    cosine             real        NOT NULL,
    band               text        NOT NULL
                                   CHECK (band IN ('keep', 'drop', 'ambiguous')),
    auto_verdict       boolean     NOT NULL,
    hybrid_verdict     boolean,
    decided_by         text        NOT NULL
                                   CHECK (decided_by IN ('keep_band', 'drop_band', 'model', 'pending')),
    in_transfer_rumors boolean     NOT NULL DEFAULT false,
    career_teams       smallint    NOT NULL DEFAULT 0,
    keep_threshold     real        NOT NULL,
    drop_threshold     real        NOT NULL,
    embed_model        text        NOT NULL,
    model_version      text,
    generated_at       timestamptz NOT NULL DEFAULT now()
);

-- Resumability / fresh-run scoping is by band config; the analysis groups by article + verdict.
CREATE INDEX IF NOT EXISTS idx_resolve_shadow_band
    ON resolve_shadow (keep_threshold, drop_threshold, article_id);

COMMENT ON TABLE resolve_shadow IS
    'Rust Cognition Harness L5 embedding-Resolve at-scale shadow — offline record of the hybrid '
    'resolve_set verdict beside local model''s vetted label, over the whole secondary-link corpus. '
    'Read-only on the pipeline (never touches news_article_entities.vetted). Drop after the scrub cutover.';

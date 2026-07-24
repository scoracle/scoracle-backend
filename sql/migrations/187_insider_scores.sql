-- 187_insider_scores.sql
--
-- Tarot deck Phase 4b (the Insider's card score — "the wire wrap"): a new per-entity table.
-- Nothing entity-keyed exists in the transfer product (transfer_rumors is pair-keyed), so the
-- Insider's 1-99 busyness verdict gets its own home. One row per wrap: an Oracle-shaped
-- end-of-drain call (is1, contract insider-score-v1) grounded by the entity's active vetted
-- board, debounced on input_hash (the sorted board content + prompt_version) — an unchanged
-- wire pays nothing. `score` is NOT NULL by design: an EMPTY board means no call and NO row
-- at all (never a marker) — the frontend's Veil comes from the empty rumor board itself, and
-- the transfers card renders EmptyCard, so a lingering score never displays. `read` is the
-- Insider's 1-2 sentence wrap — persisted for audit + the prompt's continuity memory, never
-- served. `previous_score` mirrors sigil_synthesis.previous_score (continuity audit).
--
-- ADDITIVE: the running binary ignores the new table; the new Rust daemon writes it and the
-- new Go API serves its latest score. Deploy order: migrate BEFORE the restarts.

BEGIN;

CREATE TABLE IF NOT EXISTS public.insider_scores (
    id             bigserial PRIMARY KEY,
    sport          text NOT NULL REFERENCES public.sports(id),
    entity_type    text NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id      integer NOT NULL,
    score          smallint NOT NULL CHECK (score BETWEEN 1 AND 99),
    previous_score smallint CHECK (previous_score IS NULL OR previous_score BETWEEN 1 AND 99),
    read           text,
    model_version  text,
    prompt_version text NOT NULL,
    input_hash     text,
    generated_at   timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.insider_scores IS
    'The Insider''s per-entity 1-99 wire-busyness verdict (tarot deck Phase 4b, mig 187). One '
    'row per wrap — an end-of-drain TransferLogic call after the pair loop, debounced on the '
    'active-board input_hash. No marker rows: an empty board writes nothing (the Veil comes '
    'from the empty board). Served as the Transfers card''s score (latest row, '
    'scope-independent); read/previous_score are audit + prompt memory only.';

COMMENT ON COLUMN public.insider_scores.read IS
    'The Insider''s 1-2 sentence wire wrap (audit + next wrap''s continuity memory; never served).';

COMMENT ON COLUMN public.insider_scores.input_hash IS
    'Debounce key: hash over prompt_version + the sorted active board '
    '(counterparty:heat:direction:stage per rumor). Content-keyed, not row-id-keyed.';

-- The serve/debounce path: latest wrap per entity (matches Harness::debounce_unchanged's
-- entity-scoped read and the Go card_score subquery).
CREATE INDEX IF NOT EXISTS insider_scores_entity_latest
    ON public.insider_scores (sport, entity_type, entity_id, generated_at DESC);

INSERT INTO public.schema_migrations(version) VALUES ('187_insider_scores')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

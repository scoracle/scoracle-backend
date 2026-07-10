-- 144_cognition_ledger.sql
--
-- Multi-Lens Cognition Panel Phase 2: durable diagnostic ledger for lens calls.
--
-- Product rows remain the served surface (`news_summaries`, `transfer_rumors`,
-- `vibe_scores`, `sigil_synthesis`, `stat_summaries`). This table is the
-- prunable evidence record: what the stage sent to the model, what evidence was
-- included/excluded, what product rows resulted, and which output contract the
-- parser expected. Start with narratives + transfers, then extend to the other
-- lenses after the shape proves useful.

BEGIN;

CREATE TABLE IF NOT EXISTS public.cognition_ledger (
    id                      bigserial PRIMARY KEY,
    stage                   text        NOT NULL,
    lens                    text        NOT NULL,
    role                    text        NOT NULL,
    entity_type             text        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id               integer     NOT NULL,
    sport                   text        NOT NULL,
    pair_entity_type        text        CHECK (pair_entity_type IS NULL OR pair_entity_type IN ('player', 'team')),
    pair_entity_id          integer,
    trigger_type            text        NOT NULL,
    trigger_payload         jsonb       NOT NULL DEFAULT 'null'::jsonb,
    product_table           text        NOT NULL,
    product_row_ids         bigint[]    NOT NULL DEFAULT '{}'::bigint[],
    model_version           text        NOT NULL,
    prompt_version          text        NOT NULL,
    output_contract_version text        NOT NULL,
    input_ids               bigint[]    NOT NULL DEFAULT '{}'::bigint[],
    input_hash              text,
    request_body            jsonb,
    built_prompt            text,
    included_evidence       jsonb       NOT NULL DEFAULT '[]'::jsonb,
    excluded_evidence       jsonb       NOT NULL DEFAULT '[]'::jsonb,
    context_budget          jsonb       NOT NULL DEFAULT '{}'::jsonb,
    parser_outcome          text        NOT NULL,
    generated_at            timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cognition_ledger_entity
    ON public.cognition_ledger (stage, entity_type, entity_id, sport, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_cognition_ledger_pair
    ON public.cognition_ledger (stage, entity_type, entity_id, pair_entity_type, pair_entity_id, sport, generated_at DESC)
    WHERE pair_entity_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_cognition_ledger_product_rows
    ON public.cognition_ledger USING gin (product_row_ids);

COMMENT ON TABLE public.cognition_ledger IS
    'Diagnostic ledger for model-backed cognition lens calls. Product tables are still the served surface; this table records request/evidence/parser provenance for post-hoc failure analysis and fixture capture.';
COMMENT ON COLUMN public.cognition_ledger.output_contract_version IS
    'Parser/output schema version, distinct from prompt_version. Use this when a reply shape changes without changing the input prompt contract.';
COMMENT ON COLUMN public.cognition_ledger.included_evidence IS
    'JSON summary of evidence sent to the model, including source ids and stage-specific context.';
COMMENT ON COLUMN public.cognition_ledger.excluded_evidence IS
    'JSON summary of evidence considered but excluded, with reason when known.';
COMMENT ON COLUMN public.cognition_ledger.context_budget IS
    'JSON budget telemetry, e.g. available generation tokens and evaluated tokens when a model call happened.';

INSERT INTO public.schema_migrations(version) VALUES ('144_cognition_ledger')
    ON CONFLICT DO NOTHING;

COMMIT;

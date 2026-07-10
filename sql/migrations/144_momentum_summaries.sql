-- 144_momentum_summaries.sql
--
-- First-class generated Momentum product. `momentum_scores` remains the deterministic numeric
-- backbone; this append-only table stores the model-written direction/blurb consumed by Sigil and
-- served alongside the numeric trajectory.

CREATE TABLE IF NOT EXISTS public.momentum_summaries (
    id BIGSERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    season INTEGER NOT NULL,
    trigger_type TEXT NOT NULL DEFAULT 'periodic',
    trigger_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    direction TEXT,
    score SMALLINT,
    blurb TEXT,
    input_components JSONB NOT NULL DEFAULT '{}'::jsonb,
    input_hash TEXT,
    model_version TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT momentum_summaries_entity_type_check CHECK (entity_type IN ('player', 'team')),
    CONSTRAINT momentum_summaries_direction_check CHECK (
        direction IS NULL OR direction IN ('rising', 'falling', 'steady')
    ),
    CONSTRAINT momentum_summaries_score_check CHECK (score IS NULL OR score BETWEEN -5 AND 5)
);

CREATE INDEX IF NOT EXISTS idx_momentum_summaries_entity_recent
    ON public.momentum_summaries (entity_type, entity_id, sport, season, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_momentum_summaries_input_hash
    ON public.momentum_summaries (entity_type, entity_id, sport, season, input_hash)
    WHERE input_hash IS NOT NULL;

COMMENT ON TABLE public.momentum_summaries IS
    'Generated Momentum trajectory cards. Numeric trajectory remains in momentum_scores; this table '
    'stores the first-class direction/score/blurb product consumed by Sigil.';

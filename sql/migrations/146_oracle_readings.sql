-- 146_oracle_readings.sql
--
-- The Oracle lens product (lens quality plan, "The Oracle Lens" 2026-07-12): the voiced,
-- persona-forward reading over the assembled card stack, downstream of Sigil. Append-only like
-- every cognition product. `reading` is the 2-4 sentence Oracle read (NULL = marker: no scored
-- Sigil synthesis to read); `omen` is COMPUTED in code from the consumed Sigil + pillar
-- directions (ascendant/steady/waning/crossroads), never a model output; `sigil_score` echoes
-- the consumed synthesis score so the served card is self-contained. `input_hash` is the
-- debounce key over the CONSUMED SIGIL GENERATION (score/blurb/panel fields + sigil's own
-- input_hash) — the Oracle regenerates only when Sigil's reading changed.
--
-- Deploy order: ADDITIVE — apply BEFORE the cognition-service deploy (the new binary's oracle
-- stage INSERTs here; the running binary never touches it).

BEGIN;

CREATE TABLE IF NOT EXISTS public.oracle_readings (
    id BIGSERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    season INTEGER NOT NULL,
    trigger_type TEXT NOT NULL DEFAULT 'periodic',
    trigger_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    reading TEXT,
    omen TEXT,
    sigil_score SMALLINT,
    input_components JSONB NOT NULL DEFAULT '{}'::jsonb,
    input_hash TEXT,
    model_version TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT oracle_readings_entity_type_check CHECK (entity_type IN ('player', 'team')),
    CONSTRAINT oracle_readings_omen_check CHECK (
        omen IS NULL OR omen IN ('ascendant', 'steady', 'waning', 'crossroads')
    ),
    CONSTRAINT oracle_readings_sigil_score_check CHECK (
        sigil_score IS NULL OR sigil_score BETWEEN 1 AND 100
    )
);

CREATE INDEX IF NOT EXISTS idx_oracle_readings_entity_recent
    ON public.oracle_readings (entity_type, entity_id, sport, season, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_oracle_readings_input_hash
    ON public.oracle_readings (entity_type, entity_id, sport, season, input_hash)
    WHERE input_hash IS NOT NULL;

COMMENT ON TABLE public.oracle_readings IS
    'Oracle lens readings: the persona-voiced 2-4 sentence read over the assembled cards, '
    'downstream of sigil_synthesis. reading NULL = marker (no scored Sigil to read). omen is '
    'computed in code, never a model output. input_hash debounces on the consumed Sigil generation.';

INSERT INTO public.schema_migrations(version) VALUES ('146_oracle_readings')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

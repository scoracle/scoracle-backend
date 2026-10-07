-- Coherent scored period cards and durable generation attempts. Apply before binaries.
BEGIN;
ALTER TABLE public.vibe_scores DROP CONSTRAINT IF EXISTS vibe_scores_sentiment_check;
ALTER TABLE public.vibe_scores ADD CONSTRAINT vibe_scores_sentiment_check
    CHECK (sentiment IS NULL OR sentiment BETWEEN 0 AND 100);
ALTER TABLE public.vibe_scores
    ADD COLUMN IF NOT EXISTS reporting_start timestamptz,
    ADD COLUMN IF NOT EXISTS reporting_end timestamptz,
    ADD COLUMN IF NOT EXISTS evidence_cutoff timestamptz,
    ADD COLUMN IF NOT EXISTS scoring_version text,
    ADD COLUMN IF NOT EXISTS source_references jsonb;
ALTER TABLE public.vibe_scores DROP CONSTRAINT IF EXISTS vibe_period_card_check;
ALTER TABLE public.vibe_scores ADD CONSTRAINT vibe_period_card_check CHECK (
    scoring_version IS NULL OR (
        sentiment IS NOT NULL AND hook IS NOT NULL AND length(btrim(hook)) BETWEEN 1 AND 140
        AND prompt IS NOT NULL AND length(btrim(prompt))>0
        AND reporting_start IS NOT NULL AND reporting_end IS NOT NULL AND reporting_start<reporting_end
        AND evidence_cutoff IS NOT NULL AND input_hash IS NOT NULL
        AND week_season IS NOT NULL AND week_no IS NOT NULL));
CREATE TABLE IF NOT EXISTS public.vibe_card_attempts (
    id bigserial PRIMARY KEY,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    input_hash text NOT NULL,
    model_version text NOT NULL,
    receipt jsonb NOT NULL,
    outcome text NOT NULL CHECK (outcome IN ('generated','failed','abstained','published')),
    product_id bigint REFERENCES public.vibe_scores(id),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS vibe_card_attempt_input ON public.vibe_card_attempts(input_hash,model_version,id DESC);
CREATE INDEX IF NOT EXISTS vibe_card_period ON public.vibe_scores(sport,entity_type,entity_id,week_season,week_no,generated_at DESC);
COMMENT ON COLUMN public.vibe_scores.scoring_version IS 'emotional-valence-v1: 0 distressing, 25 troubled, 50 mixed/balanced, 75 hopeful, 100 joyful. Unknown is absence.';
INSERT INTO public.schema_migrations(version) VALUES ('290_vibe_period_cards') ON CONFLICT DO NOTHING;
COMMIT;

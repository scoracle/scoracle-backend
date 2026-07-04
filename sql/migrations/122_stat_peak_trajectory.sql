-- 122_stat_peak_trajectory.sql
--
-- Add deterministic PEAK trajectory metadata to the stats rail. This tracks the
-- same raw z-score rating values used by the ranking engine (Composite +
-- Specialist/PEAK), without changing rating math or asking the model to infer
-- recent form.

BEGIN;

ALTER TABLE public.stat_summaries
    ADD COLUMN IF NOT EXISTS peak_trajectory TEXT
        CHECK (peak_trajectory IN ('rising', 'falling', 'steady')),
    ADD COLUMN IF NOT EXISTS peak_trajectory_label TEXT,
    ADD COLUMN IF NOT EXISTS peak_trajectory_components JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN public.stat_summaries.peak_trajectory IS
    'Deterministic recent-form marker for the stats rail z-score values: rising, falling, or steady.';
COMMENT ON COLUMN public.stat_summaries.peak_trajectory_label IS
    'Human label for the rating z-score trajectory, e.g. Composite and PEAK z-scores trending down over recent games.';
COMMENT ON COLUMN public.stat_summaries.peak_trajectory_components IS
    'Transparent PEAK trajectory inputs: recent Composite/PEAK z-score samples, slopes, and sample sizes.';

CREATE INDEX IF NOT EXISTS idx_stat_summaries_peak_trajectory
    ON public.stat_summaries (sport, peak_trajectory, generated_at DESC)
    WHERE body IS NOT NULL AND peak_trajectory IS NOT NULL;

COMMIT;

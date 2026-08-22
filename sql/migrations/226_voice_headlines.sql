-- 226_voice_headlines.sql
--
-- The uniform output contract, storage half (drop 1 of the headline/body plan): every voice
-- product gains a `headline` — the model-emitted card title that leaderboards will serve in
-- place of prose bodies. The Journalist already has news_summaries.narrative_title and the
-- Influencer already has vibe_scores.hook; the Editor already has packets.headline; the
-- Insider's per-pair model_summary is contractually one sentence and serves as its own
-- headline. These three seats are the gap: the Oracle's reading, the Analyst's blurb, and
-- the Scout's body had no short title field.
--
-- ADDITIVE + nullable, no backfill: pre-bump rows stay NULL until each seat naturally
-- regenerates (lazy invalidation is the repo norm — quiet entities keep NULL headlines,
-- which is correct: boards omit them and their freshness gates bound exposure).
--
-- Deploy order: additive ⇒ migrate BEFORE the release. Drop 1 ships no serving change;
-- the statements that read these columns land in drop 2.

BEGIN;

ALTER TABLE public.sigil_synthesis
    ADD COLUMN IF NOT EXISTS headline text;

ALTER TABLE public.momentum_summaries
    ADD COLUMN IF NOT EXISTS headline text;

ALTER TABLE public.stat_summaries
    ADD COLUMN IF NOT EXISTS headline text;

COMMENT ON COLUMN public.sigil_synthesis.headline IS
    'The Oracle''s model-emitted card title for this crown (extractive, evidence-traced). '
    'NULL = marker row or a pre-headline row → boards omit, profiles render reading alone. '
    'Carried forward verbatim with reading when the re-voice hysteresis skips the model '
    '(those rows are already identifiable by voiced_at < generated_at — no extra flag).';
COMMENT ON COLUMN public.momentum_summaries.headline IS
    'The Analyst''s model-emitted card title for this trajectory read (extractive, '
    'evidence-traced). NULL = a pre-headline row → momentum board omits until regen.';
COMMENT ON COLUMN public.stat_summaries.headline IS
    'The Scout''s model-emitted card title for this read (extractive, evidence-traced). '
    'NULL = insufficient-stats marker or a pre-headline row → the rating card renders body alone.';

INSERT INTO public.schema_migrations(version) VALUES ('226_voice_headlines')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

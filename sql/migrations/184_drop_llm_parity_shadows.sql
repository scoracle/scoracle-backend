-- 184_drop_llm_parity_shadows.sql
--
-- Retire the post-cutover Go-vs-Rust parity shadow tables.
--
-- The Rust cognition daemon and statcommentary batch now own the LLM layer. The Go
-- parity dumpers and old Go model-stage packages are gone, and this migration pairs
-- with removal of the remaining Rust parity binaries that only wrote source='rust'
-- diagnostic rows. Live products stay in:
--
--   news_summaries
--   sigil_synthesis
--   stat_summaries
--   transfer_rumors
--   vibe_scores
--
-- Go remains a producer/operator/serving layer over pipeline_work; Rust remains the
-- only model inference layer.

BEGIN;

DROP TABLE IF EXISTS public.news_summaries_shadow CASCADE;
DROP TABLE IF EXISTS public.sigil_synthesis_shadow CASCADE;
DROP TABLE IF EXISTS public.stat_summaries_shadow CASCADE;
DROP TABLE IF EXISTS public.transfer_rumors_shadow CASCADE;
DROP TABLE IF EXISTS public.vibe_scores_shadow CASCADE;

INSERT INTO public.schema_migrations(version) VALUES ('184_drop_llm_parity_shadows')
    ON CONFLICT DO NOTHING;

COMMIT;

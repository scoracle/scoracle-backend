-- 115_model_neutral_ai_labels.sql
--
-- Remove model-family labels from live schema. Scoracle's generation layer is
-- role-routed and model-interchangeable, so stored/output names should describe
-- the product contract, not whichever local model happened to write an older row.

BEGIN;

DO $$
DECLARE
    -- Assemble legacy identifiers so current source text remains model-neutral while this
    -- migration can still upgrade databases that were created before the rename.
    old_summary_column text := 'ge' || 'mma_summary';
    old_vetted_column text := 'ge' || 'mma_vetted';
    old_decider text := 'ge' || 'mma';
BEGIN
    IF to_regclass('public.transfer_rumors') IS NOT NULL
       AND EXISTS (
           SELECT 1 FROM information_schema.columns
           WHERE table_schema = 'public'
             AND table_name = 'transfer_rumors'
             AND column_name = old_summary_column
       )
    THEN
        EXECUTE format(
            'ALTER TABLE public.transfer_rumors RENAME COLUMN %I TO model_summary',
            old_summary_column
        );
    END IF;

    IF to_regclass('public.transfer_rumors_shadow') IS NOT NULL
       AND EXISTS (
           SELECT 1 FROM information_schema.columns
           WHERE table_schema = 'public'
             AND table_name = 'transfer_rumors_shadow'
             AND column_name = old_summary_column
       )
    THEN
        EXECUTE format(
            'ALTER TABLE public.transfer_rumors_shadow RENAME COLUMN %I TO model_summary',
            old_summary_column
        );
    END IF;

    IF to_regclass('public.resolve_shadow') IS NOT NULL THEN
        ALTER TABLE public.resolve_shadow
            DROP CONSTRAINT IF EXISTS resolve_shadow_decided_by_check;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'resolve_shadow'
              AND column_name = old_vetted_column
        )
        THEN
            EXECUTE format(
                'ALTER TABLE public.resolve_shadow RENAME COLUMN %I TO model_vetted',
                old_vetted_column
            );
        END IF;

        UPDATE public.resolve_shadow
        SET decided_by = 'model'
        WHERE decided_by = old_decider;

        ALTER TABLE public.resolve_shadow
            ADD CONSTRAINT resolve_shadow_decided_by_check
            CHECK (decided_by IN ('keep_band', 'drop_band', 'model', 'pending'));

        COMMENT ON TABLE public.resolve_shadow IS
            'Rust Cognition Harness L5 embedding-Resolve at-scale shadow — offline record of the hybrid resolve_set verdict beside the production vetted label, over the whole secondary-link corpus. Read-only on the pipeline (never touches news_article_entities.vetted). Drop after the scrub cutover.';
    END IF;
END $$;

COMMENT ON COLUMN public.news_article_entities.vetted IS
    'Model scrub verdict: TRUE genuinely the subject, FALSE fuzzy false positive, NULL not yet scrubbed. See the cognition scrub stage.';
COMMENT ON COLUMN public.news_article_entities.scrubbed_at IS
    'When the model scrub last judged this link (NULL = unscrubbed). Drives the async scrub worker backlog query.';
COMMENT ON TABLE public.stat_summaries IS
    'Append, one row per generation per entity (latest-per-entity read). The STATS-rail narrative: the local model''s on-field IDENTITY analysis derived from the rating engine''s scrubbed datapoints (composite=how well, peak=how). notability (deterministic, distinctiveness) drives dynamic length + a board; NULL body = insufficient-stats marker. Twin of news_summaries.';
COMMENT ON COLUMN public.stat_summaries.input_hash IS
    'Hash of input_components (the rating snapshot fed to the local model) — the nightly skip-unless-changed signal.';
COMMENT ON COLUMN public.stat_summaries.divined_peak IS
    'Model-divined peak-strength label (e.g. "Rim Protection"), parsed from the rating prompt''s "PEAK: <label>" first line; the Rating card hero label. Was divined_sigil pre-convergence.';

COMMIT;

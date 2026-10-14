BEGIN;
-- Native Classifier spectrum: one immutable row per (article, target). The spectrum is the
-- authoritative classification record; extraction and character gating are separate. The routing
-- scores are a projection of this row, never a replacement for it.
CREATE TABLE IF NOT EXISTS public.classifier_spectra (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_id bigint NOT NULL REFERENCES public.classifier_sources(id) ON DELETE CASCADE,
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL,
    target jsonb NOT NULL,
    model text NOT NULL,
    criteria_version text NOT NULL,
    scrub_version text NOT NULL,
    clean_text text NOT NULL,
    clean_sha256 text NOT NULL,
    spectrum jsonb NOT NULL,
    gate jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (article_id, sport, target, model, criteria_version)
);
CREATE INDEX IF NOT EXISTS classifier_spectra_target
    ON public.classifier_spectra(article_id, sport, target);
COMMENT ON TABLE public.classifier_spectra IS
    'Native Classifier spectrum and routing gate; immutable, versioned, one row per (article, target).';

INSERT INTO public.schema_migrations(version) VALUES ('299_classifier_spectra') ON CONFLICT DO NOTHING;
COMMIT;

BEGIN;
-- Per-character obligations derived from a spectrum's routing gate. One row per character that
-- fired; a character with no fired bucket gets no row (a complete measurement, no eligible signal).
CREATE TABLE IF NOT EXISTS public.classifier_spectrum_deliveries (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    spectrum_id bigint NOT NULL REFERENCES public.classifier_spectra(id) ON DELETE CASCADE,
    character text NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','used','abstained','superseded')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (spectrum_id, character)
);
CREATE INDEX IF NOT EXISTS classifier_spectrum_deliveries_pending
    ON public.classifier_spectrum_deliveries(character, status);
COMMENT ON TABLE public.classifier_spectrum_deliveries IS
    'Character obligations projected from a spectrum gate; the spectrum row remains authoritative.';

INSERT INTO public.schema_migrations(version) VALUES ('300_classifier_spectrum_deliveries') ON CONFLICT DO NOTHING;
COMMIT;

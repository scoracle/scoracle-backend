BEGIN;
-- Independent source snapshots and model-attempt receipts. No character publication.
CREATE TABLE IF NOT EXISTS public.classifier_sources (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL,
    input_hash text NOT NULL,
    body_sha256 text NOT NULL,
    source jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (article_id, sport, input_hash, body_sha256)
);
CREATE TABLE IF NOT EXISTS public.classifier_measurements (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_id bigint NOT NULL REFERENCES public.classifier_sources(id) ON DELETE CASCADE,
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL,
    request_hash text NOT NULL,
    status text NOT NULL CHECK (status IN ('source_bound_provisional','error')),
    receipt jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (receipt->>'status' = status),
    CHECK ((receipt->>'production_eligible')::boolean = false)
);
CREATE INDEX IF NOT EXISTS classifier_measurement_reuse
    ON public.classifier_measurements(article_id, sport, request_hash)
    WHERE status = 'source_bound_provisional';
COMMENT ON TABLE public.classifier_sources IS
    'Complete native publisher-source snapshots, independent of model admission.';
COMMENT ON TABLE public.classifier_measurements IS
    'Immutable model requests, replies, source-bound claims and failures; calibration is separate.';

INSERT INTO public.schema_migrations(version) VALUES ('291_classifier_plumbing') ON CONFLICT DO NOTHING;
COMMIT;

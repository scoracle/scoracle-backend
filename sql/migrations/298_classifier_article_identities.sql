BEGIN;
-- The shared mention index carries source ownership, never model scores or event conclusions.
ALTER TABLE public.news_article_entities ADD COLUMN IF NOT EXISTS classifier_source_id bigint
    REFERENCES public.classifier_sources(id) ON DELETE CASCADE;
CREATE INDEX IF NOT EXISTS classifier_article_identity_source
    ON public.news_article_entities(classifier_source_id) WHERE classifier_source_id IS NOT NULL;
COMMENT ON TABLE public.news_article_entities IS
    'Canonical article/entity mentions. Classifier acquisition replaces each article/sport set with unique current canonical-name matches over complete retained publisher text. classifier_source_id owns the immutable source and identity evidence. RSS candidates, model signals, roles, transfers and other event claims are not identity links. Historical pre-Classifier links remain provenance-null until reacquired.';
INSERT INTO public.schema_migrations(version) VALUES ('298_classifier_article_identities') ON CONFLICT DO NOTHING;
COMMIT;

BEGIN;
-- Native source-bound delivery retains its own trigger provenance on the served product.
ALTER TABLE public.news_summaries DROP CONSTRAINT IF EXISTS news_summaries_trigger_type_check;
ALTER TABLE public.news_summaries ADD CONSTRAINT news_summaries_trigger_type_check
    CHECK (trigger_type IN ('news_spike','periodic','manual','classifier'));
INSERT INTO public.schema_migrations(version) VALUES ('297_classifier_journalist_product') ON CONFLICT DO NOTHING;
COMMIT;

-- A model-copied score line is a review request, not a canonical fixture result.
-- Retain historical review rows and add an explicit unavailable outcome.
ALTER TABLE public.harvester_fixture_reviews
    DROP CONSTRAINT harvester_fixture_reviews_status_check;
ALTER TABLE public.harvester_fixture_reviews
    ADD CONSTRAINT harvester_fixture_reviews_status_check CHECK (status IN (
        'no_result', 'quote_not_found', 'invalid_result', 'team_unresolved',
        'already_correct', 'corrected', 'created', 'extraction_unavailable'
    ));
INSERT INTO public.schema_migrations(version)
VALUES ('288_graph_fixture_extraction_unavailable') ON CONFLICT (version) DO NOTHING;

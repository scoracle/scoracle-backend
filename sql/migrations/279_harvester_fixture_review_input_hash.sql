BEGIN;

ALTER TABLE public.harvester_fixture_reviews
    ADD COLUMN input_hash text NOT NULL DEFAULT '';

ALTER TABLE public.harvester_fixture_reviews
    DROP CONSTRAINT harvester_fixture_reviews_pkey;

ALTER TABLE public.harvester_fixture_reviews
    ADD PRIMARY KEY (article_id,sport,contract_version,input_hash);

COMMENT ON COLUMN public.harvester_fixture_reviews.input_hash IS
    'Graph material hash; a changed exact publisher context creates a new immutable review identity instead of overwriting the earlier fixture evidence.';

INSERT INTO public.schema_migrations(version)
VALUES ('279_harvester_fixture_review_input_hash') ON CONFLICT (version) DO NOTHING;

COMMIT;

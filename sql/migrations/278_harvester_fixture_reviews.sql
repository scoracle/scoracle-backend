BEGIN;

CREATE TABLE IF NOT EXISTS public.harvester_fixture_reviews (
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL REFERENCES public.sports(id),
    contract_version text NOT NULL,
    model_version text NOT NULL,
    result_line text NOT NULL DEFAULT '',
    source_quote text,
    status text NOT NULL CHECK (status IN (
        'no_result','quote_not_found','invalid_result','team_unresolved',
        'already_correct','corrected','created'
    )),
    fixture_id integer REFERENCES public.fixtures(id),
    reviewed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id,sport,contract_version)
);

COMMENT ON TABLE public.harvester_fixture_reviews IS
    'Claim-fenced Graph review of one exact Harvester source for a completed fixture result. A model-suggested result can nominate a fixture only when its line occurs verbatim in the retained publisher headline or hash-verified opening and both team names resolve uniquely. Rejected and empty reviews remain queryable.';

INSERT INTO public.schema_migrations(version)
VALUES ('278_harvester_fixture_reviews') ON CONFLICT (version) DO NOTHING;

COMMIT;

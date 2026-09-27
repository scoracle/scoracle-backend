-- The first Laya decision is made from the Google headline before publisher
-- acquisition. Keep even negative decisions so skipped fetches are auditable.
CREATE TABLE IF NOT EXISTS public.harvester_headline_gates (
    article_id bigint NOT NULL,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    contract_version text NOT NULL,
    headline text NOT NULL,
    input_hash text NOT NULL,
    model_revision text NOT NULL,
    choice text NOT NULL CHECK (choice IN ('relevant', 'irrelevant')),
    admitted boolean NOT NULL,
    policy_version text NOT NULL,
    read_threshold double precision NOT NULL CHECK (read_threshold >= 0 AND read_threshold <= 1),
    request jsonb NOT NULL,
    answer jsonb NOT NULL,
    model_provenance jsonb NOT NULL,
    raw_response jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id, entity_type, entity_id, sport,
                 contract_version, model_revision, input_hash, policy_version),
    FOREIGN KEY (article_id, entity_type, entity_id, sport)
        REFERENCES public.harvester_query_provenance(article_id, entity_type, entity_id, sport)
        ON DELETE CASCADE
);
COMMENT ON TABLE public.harvester_headline_gates IS
    'Versioned Laya headline-only entity decisions before publisher fetch; negative gates have no acquired body or character assignment.';

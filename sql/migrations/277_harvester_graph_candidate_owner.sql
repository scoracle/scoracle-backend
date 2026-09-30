BEGIN;

COMMENT ON TABLE public.entity_candidates IS
    'A source-grounded name awaiting Investigator adjudication. Legacy Editor resolver leftovers and Harvester Graph discoveries share the sport/name idempotency key. A Harvester discovery requires an exact name in the retained publisher headline or hash-verified opening; neither a model suggestion nor a candidate row is an authoritative identity link.';

COMMENT ON TABLE public.candidate_mentions IS
    'One row per candidate/article with a code-sliced exact source quote. Legacy Editor and Harvester Graph both add evidence under the same distinct-article counter; repeated extraction of one article cannot manufacture corroboration.';

INSERT INTO public.schema_migrations(version)
VALUES ('277_harvester_graph_candidate_owner') ON CONFLICT (version) DO NOTHING;

COMMIT;

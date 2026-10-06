-- Exact source-name matches are candidates for character/Graph identity review.
-- They do not write news_article_entities or claim article aboutness.
BEGIN;
CREATE TABLE IF NOT EXISTS public.harvester_entity_mentions (
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    entity_type text NOT NULL CHECK (entity_type IN ('team','player','person')),
    entity_id integer NOT NULL,
    sport text NOT NULL,
    matched_norm text NOT NULL,
    body_sha256 text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id, entity_type, entity_id, sport, matched_norm)
);
CREATE INDEX IF NOT EXISTS harvester_entity_mentions_entity_idx
    ON public.harvester_entity_mentions(entity_type, entity_id, sport, article_id);
COMMENT ON TABLE public.harvester_entity_mentions IS
    'Unique exact normalized name-surface matches in publisher text. Candidate identity evidence only; Google query and Laya routing do not write authoritative article links.';
INSERT INTO public.schema_migrations(version)
VALUES ('271_harvester_identity_candidates') ON CONFLICT DO NOTHING;
COMMIT;

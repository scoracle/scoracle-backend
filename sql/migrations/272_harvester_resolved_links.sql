-- Canonical-name identity resolution from the exact delivered publisher opening.
-- This table is additive while Editor keeps its own historical link writer.
BEGIN;
CREATE TABLE IF NOT EXISTS public.harvester_resolved_links (
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    entity_type text NOT NULL CHECK (entity_type IN ('team','player','person')),
    entity_id integer NOT NULL,
    sport text NOT NULL,
    matched_norm text NOT NULL,
    body_sha256 text NOT NULL,
    opening_sha256 text NOT NULL,
    resolution_method text NOT NULL CHECK (resolution_method = 'unique_canonical_name_v1'),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id, entity_type, entity_id, sport)
);
CREATE INDEX IF NOT EXISTS harvester_resolved_links_entity_idx
    ON public.harvester_resolved_links(entity_type, entity_id, sport, article_id);
COMMENT ON TABLE public.harvester_resolved_links IS
    'Authoritative identity links only for a unique canonical entity name present in the exact headline or publisher opening delivered to characters. Query provenance and Laya choices are never sufficient.';
INSERT INTO public.schema_migrations(version)
VALUES ('272_harvester_resolved_links') ON CONFLICT DO NOTHING;
COMMIT;

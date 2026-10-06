-- Harvester publishes only exact-source, uniquely resolved canonical identities
-- to the shared article/entity index. Editor remains the legacy writer while its
-- queue drains; query-team provenance and Laya signals never enter this table.
BEGIN;
COMMENT ON TABLE public.news_article_entities IS
    'Authoritative article/entity links. Editor historically cleared and rewrote its resolved links. Harvester now replaces each article/sport link set with only unique canonical-name links grounded in the retained publisher headline or opening, with body/offset provenance in harvester_resolved_links. Google query provenance and Laya decisions are not links.';
INSERT INTO public.schema_migrations(version)
VALUES ('284_harvester_shared_entity_links') ON CONFLICT DO NOTHING;
COMMIT;

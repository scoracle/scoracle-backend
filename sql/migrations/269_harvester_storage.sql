-- Harvester's additive source/query and classification records. Existing Editor
-- readers and full_text ownership remain intact during the staged cutover.
BEGIN;

CREATE TABLE IF NOT EXISTS public.harvester_query_provenance (
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    entity_type text NOT NULL CHECK (entity_type IN ('team', 'player', 'person')),
    entity_id integer NOT NULL,
    sport text NOT NULL,
    feed_rank integer,
    query_terms jsonb NOT NULL DEFAULT '[]'::jsonb,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id, entity_type, entity_id, sport)
);
CREATE INDEX IF NOT EXISTS harvester_query_provenance_entity_idx
    ON public.harvester_query_provenance(entity_type, entity_id, sport, feed_rank);
COMMENT ON TABLE public.harvester_query_provenance IS
    'Every Google entity query that returned a canonical URL; retrieval provenance only, never an authoritative entity link.';

CREATE TABLE IF NOT EXISTS public.harvester_acquisitions (
    article_id bigint PRIMARY KEY REFERENCES public.news_articles(id) ON DELETE CASCADE,
    status text NOT NULL CHECK (status IN ('acquired', 'duplicate', 'retryable_error', 'blocked', 'low_content', 'classification_error')),
    final_url text,
    final_domain text,
    body_sha256 text,
    body_bytes integer CHECK (body_bytes IS NULL OR body_bytes >= 0),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error text,
    updated_at timestamptz NOT NULL DEFAULT now()
);
COMMENT ON TABLE public.harvester_acquisitions IS
    'Acquisition state per canonical article. The unchanged publisher body has one owner, news_articles.full_text.';

CREATE TABLE IF NOT EXISTS public.harvester_classifications (
    id bigserial PRIMARY KEY,
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    entity_type text NOT NULL,
    entity_id integer NOT NULL,
    sport text NOT NULL,
    contract_version text NOT NULL,
    model_revision text NOT NULL,
    entity_choice text NOT NULL CHECK (entity_choice IN ('relevant', 'irrelevant')),
    body_sha256 text NOT NULL,
    headline text NOT NULL,
    model_input_start integer NOT NULL CHECK (model_input_start >= 0),
    model_input_end integer NOT NULL CHECK (model_input_end >= model_input_start),
    model_input_text text NOT NULL,
    context_start integer NOT NULL CHECK (context_start >= 0),
    context_end integer NOT NULL CHECK (context_end >= context_start),
    context_text text NOT NULL,
    distributions jsonb NOT NULL,
    model_provenance jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (article_id, entity_type, entity_id, sport, contract_version, model_revision, body_sha256),
    FOREIGN KEY (article_id, entity_type, entity_id, sport)
        REFERENCES public.harvester_query_provenance(article_id, entity_type, entity_id, sport)
);
CREATE INDEX IF NOT EXISTS harvester_classifications_lookup_idx
    ON public.harvester_classifications(entity_type, entity_id, sport, created_at DESC);
COMMENT ON TABLE public.harvester_classifications IS
    'Versioned Laya decisions and exact publisher text per article/query entity. No generated editorial packet; distributions are uncalibrated until validated.';

CREATE TABLE IF NOT EXISTS public.harvester_assignments (
    classification_id bigint NOT NULL REFERENCES public.harvester_classifications(id) ON DELETE CASCADE,
    plugin_id text NOT NULL CHECK (plugin_id IN (
        'scoracle.character.narrative', 'scoracle.character.vibe',
        'scoracle.character.transfers', 'scoracle.character.rating')),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN (
        'pending', 'used', 'relevant_but_unused', 'redundant',
        'irrelevant', 'abstained', 'error')),
    reason text,
    product_ref jsonb,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (classification_id, plugin_id)
);
COMMENT ON TABLE public.harvester_assignments IS
    'Advisory Harvester delivery and character-owned final disposition; pending assignments must be accounted for before cutover acceptance.';

INSERT INTO public.schema_migrations(version)
VALUES ('269_harvester_storage') ON CONFLICT DO NOTHING;
COMMIT;

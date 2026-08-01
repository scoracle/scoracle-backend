-- 205_investigator_substrate.sql
--
-- WHAT: the eight acquisition tables for THE INVESTIGATOR (entity discovery), backend-named
-- per the 2026-07-27 planning doc: entity_candidates, candidate_mentions, acquisition_runs,
-- source_documents, entity_aliases, entity_external_ids, entity_facts, entity_relationships
-- (PLAN-one-rail.md 1.6).
--
-- WHY: the Editor NOMINATES; the Investigator VERIFIES; search DISCOVERS; sources PROVE. A
-- model mention is never a database write — every accepted fact cites a source_documents row
-- (living-database doctrine). A candidate, an attempt, a source, a fact, and a relationship are
-- DIFFERENT THINGS: combining them is how provenance disappears, so each gets its own table.
--
-- Entity triples here are loose (entity_type, entity_id, sport) with NO cross-table FK — they
-- span players/teams/persons, and sport is nullable because persons.sport is (a NULL-sport
-- person's facts still need a home). created_at on the provenance tables is write-time;
-- valid_from/valid_to is world-time — the two are different clocks.
--
-- Deploy order: additive and INERT — nothing writes these until Phase 5.

BEGIN;

-- ---------------------------------------------------------------------------
-- 1. The candidate: a name the resolver could not place, waiting on judgment.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.entity_candidates (
    id                   bigserial PRIMARY KEY,
    idempotency_key      text NOT NULL UNIQUE,
    norm_name            text NOT NULL,
    kind_hint            text,
    sport                text REFERENCES public.sports(id),
    target_entity_type   text,
    target_entity_id     integer,
    mention_count        integer NOT NULL DEFAULT 0,
    state                text NOT NULL DEFAULT 'pending' CHECK (state IN
                         ('pending','accepted','rejected_not_sport',
                          'rejected_insufficient_evidence','rejected_out_of_scope',
                          'ambiguous','deferred_source_unavailable')),
    resolved_entity_type text,
    resolved_entity_id   integer,
    first_seen_at        timestamptz NOT NULL DEFAULT now(),
    last_seen_at         timestamptz NOT NULL DEFAULT now(),
    decided_at           timestamptz
);

COMMENT ON TABLE public.entity_candidates IS
    'A nominated name awaiting the Investigator''s verdict. Nominated by the Editor''s resolver '
    '(unresolved names[] per the 5.2 rule; refused ties always nominate) — never written '
    'directly from a model mention. The queue item is pipeline_work stage investigate_entity, '
    'entity_type ''candidate'', entity_id = this id.';
COMMENT ON COLUMN public.entity_candidates.idempotency_key IS
    'Resolver-defined dedup key so one mystery name is one candidate across articles and days. '
    'Shape owned by Phase 5 (e.g. kind_hint:sport:norm_name).';
COMMENT ON COLUMN public.entity_candidates.norm_name IS 'public.nrm() of the mentioned name.';
COMMENT ON COLUMN public.entity_candidates.kind_hint IS
    'The Editor''s ep1 kind_hint at nomination: person|club|national_team|other.';
COMMENT ON COLUMN public.entity_candidates.target_entity_type IS
    'Optional pointer (with target_entity_id) to an existing entity this candidate may '
    'duplicate — set when the refusal was a near-match rather than a blank.';
COMMENT ON COLUMN public.entity_candidates.resolved_entity_type IS
    'Set with decided_at by the accept/reconcile path: the entity this candidate became or '
    'was linked to (person/player/team + id).';
COMMENT ON COLUMN public.entity_candidates.state IS
    'Decided candidates re-arm on new mentions (5.6): maintenance is demand-led, like growth.';

-- ---------------------------------------------------------------------------
-- 2. The mention: where and how the name appeared.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.candidate_mentions (
    candidate_id      bigint NOT NULL REFERENCES public.entity_candidates(id) ON DELETE CASCADE,
    article_id        bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    quote             text,
    editor_descriptor text,
    observed_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (candidate_id, article_id)
);

COMMENT ON TABLE public.candidate_mentions IS
    'One row per (candidate, article): the evidence trail behind mention_count. quote is '
    'code-sliced ±160 chars around the name''s first occurrence in the STORED full_text — the '
    'model never emits quotes; editor_descriptor is the ep1 ≤6-word descriptor from the text.';

-- ---------------------------------------------------------------------------
-- 3. The attempt: one discovery run against one candidate.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.acquisition_runs (
    id             bigserial PRIMARY KEY,
    candidate_id   bigint NOT NULL REFERENCES public.entity_candidates(id) ON DELETE CASCADE,
    status         text NOT NULL,
    query_plan     jsonb,
    outcome        text,
    rejection_reason text,
    model_version  text,
    parser_version text,
    started_at     timestamptz NOT NULL DEFAULT now(),
    finished_at    timestamptz
);

COMMENT ON TABLE public.acquisition_runs IS
    'One Investigator attempt at one candidate: the query plan it budgeted, what came back, and '
    'the versions that judged it. A candidate can accrue many runs (deferred sources retry); '
    'the candidate row holds the standing verdict, the run holds the story of each try.';

CREATE INDEX IF NOT EXISTS idx_acquisition_runs_candidate
    ON public.acquisition_runs (candidate_id);

-- ---------------------------------------------------------------------------
-- 4. The source: the page that proves it.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.source_documents (
    id               bigserial PRIMARY KEY,
    url              text NOT NULL,
    final_url        text,
    domain           text,
    fetched_at       timestamptz NOT NULL DEFAULT now(),
    content_hash     text,
    http_status      integer,
    title            text,
    retained_excerpt text,
    headers          jsonb NOT NULL DEFAULT '{}'::jsonb
);

COMMENT ON TABLE public.source_documents IS
    'The fetched page a fact cites: canonical url, redirect target, hash, and a retained '
    'excerpt. Same URL fetched twice is TWO rows — provenance is a point in time, not a link. '
    'Rows are never deleted while anything cites them (facts/relationships FK here without '
    'CASCADE, so a delete that would orphan provenance fails).';

-- ---------------------------------------------------------------------------
-- 5. The alias: append-only name surfaces earned by verification.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.entity_aliases (
    id                 bigserial PRIMARY KEY,
    entity_type        text NOT NULL,
    entity_id          integer NOT NULL,
    sport              text REFERENCES public.sports(id),
    alias              text NOT NULL,
    norm_alias         text NOT NULL,
    source_document_id bigint REFERENCES public.source_documents(id),
    state              text,
    created_at         timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.entity_aliases IS
    'APPEND-ONLY (enforced by trigger): a superseding decision is a new row, never an UPDATE. '
    'The provenance ledger behind operational alias copies (persons.search_aliases et al.), '
    'which mirror into entity_name_surfaces via refresh_entity_name_surfaces().';

CREATE OR REPLACE FUNCTION public.entity_aliases_no_update() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'entity_aliases is append-only: supersede with a new row, never UPDATE '
                    '(PLAN-one-rail.md 1.6)';
END;
$$;

DROP TRIGGER IF EXISTS entity_aliases_append_only ON public.entity_aliases;
CREATE TRIGGER entity_aliases_append_only
    BEFORE UPDATE ON public.entity_aliases
    FOR EACH ROW
    EXECUTE FUNCTION public.entity_aliases_no_update();

CREATE INDEX IF NOT EXISTS idx_entity_aliases_entity
    ON public.entity_aliases (entity_type, entity_id, sport);

-- ---------------------------------------------------------------------------
-- 6. The external id: wikidata/wikipedia/provider handles.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.entity_external_ids (
    id                 bigserial PRIMARY KEY,
    entity_type        text NOT NULL,
    entity_id          integer NOT NULL,
    sport              text REFERENCES public.sports(id),
    namespace          text NOT NULL,
    external_id        text NOT NULL,
    source_document_id bigint REFERENCES public.source_documents(id),
    created_at         timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.entity_external_ids IS
    'Stable external handles (namespace + id, e.g. wikidata:Q615) for an entity triple. The '
    'reconciliation hook D-2 names: a person-kind player later appearing in a box score '
    'reconciles by alias/external id (5.5).';

CREATE INDEX IF NOT EXISTS idx_entity_external_ids_entity
    ON public.entity_external_ids (entity_type, entity_id, sport);
CREATE INDEX IF NOT EXISTS idx_entity_external_ids_ns
    ON public.entity_external_ids (namespace, external_id);

-- ---------------------------------------------------------------------------
-- 7. The fact: one claim about one entity, with two clocks and a state.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.entity_facts (
    id                 bigserial PRIMARY KEY,
    entity_type        text NOT NULL,
    entity_id          integer NOT NULL,
    sport              text REFERENCES public.sports(id),
    fact_type          text NOT NULL,
    value_text         text,
    value_jsonb        jsonb,
    valid_from         timestamptz,
    valid_to           timestamptz,
    source_document_id bigint NOT NULL REFERENCES public.source_documents(id),
    state              text NOT NULL DEFAULT 'active'
                       CHECK (state IN ('active','superseded','conflicted')),
    created_at         timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.entity_facts IS
    'One sourced claim about one entity. source_document_id is NOT NULL — every accepted fact '
    'cites a source (living-database doctrine); gemma DESCRIBES the page, code GATES the write. '
    'Re-verification supersedes with dated validity rather than editing in place (§4 '
    'demand-led maintenance).';

CREATE INDEX IF NOT EXISTS idx_entity_facts_entity
    ON public.entity_facts (entity_type, entity_id, sport, fact_type);

-- ---------------------------------------------------------------------------
-- 8. The relationship: subject —predicate→ object, sourced and dated.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.entity_relationships (
    id                  bigserial PRIMARY KEY,
    subject_entity_type text NOT NULL,
    subject_entity_id   integer NOT NULL,
    subject_sport       text REFERENCES public.sports(id),
    predicate           text NOT NULL,
    object_entity_type  text NOT NULL,
    object_entity_id    integer NOT NULL,
    object_sport        text REFERENCES public.sports(id),
    valid_from          timestamptz,
    valid_to            timestamptz,
    source_document_id  bigint NOT NULL REFERENCES public.source_documents(id),
    state               text NOT NULL DEFAULT 'active'
                        CHECK (state IN ('active','superseded','conflicted')),
    created_at          timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.entity_relationships IS
    'Typed edge between two entity triples (coach_of, agent_of, ...), sourced and dated like '
    'entity_facts. Distinct from the graph layer''s narrative_events/narrative_persons — this '
    'is verified world structure, that is story structure.';

CREATE INDEX IF NOT EXISTS idx_entity_relationships_subject
    ON public.entity_relationships (subject_entity_type, subject_entity_id, subject_sport);
CREATE INDEX IF NOT EXISTS idx_entity_relationships_object
    ON public.entity_relationships (object_entity_type, object_entity_id, object_sport);

INSERT INTO public.schema_migrations(version) VALUES ('205_investigator_substrate')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

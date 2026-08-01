-- 202_one_rail_packets.sql
--
-- WHAT: `packets` — the compiled brief, append-only (PLAN-one-rail.md §1c).
--
-- WHY: the packet is the product: one storyline, multi-tagged, entity-indexed, snapshotted at
-- compile time. Compiled IN CODE from member editor_reads (zero model tokens — C5: the Editor's
-- output budget is coverage, not summarizing summaries). A new packet supersedes the prior one;
-- packets are NEVER edited (archive-as-moat). Contested state is preserved in `claims` —
-- "agreement reached" and "yet to reach agreement" both stay, attributed (T3/D6: the
-- disagreement IS the story).
--
-- Deploy order: additive and INERT — nothing compiles packets until Phase 6.

BEGIN;

CREATE TABLE IF NOT EXISTS public.packets (
    id                   bigserial PRIMARY KEY,
    storyline_id         bigint NOT NULL REFERENCES public.storylines(id),
    day                  date NOT NULL,
    sport                text NOT NULL REFERENCES public.sports(id),
    compiled_at          timestamptz NOT NULL DEFAULT now(),
    headline             text,
    story_types          text[] NOT NULL DEFAULT '{}',
    register             text,
    register_phrase      text,
    claims               jsonb NOT NULL DEFAULT '[]'::jsonb,
    facts                jsonb NOT NULL DEFAULT '{}'::jsonb,
    quotes               jsonb NOT NULL DEFAULT '[]'::jsonb,
    routing_tags         text[] NOT NULL DEFAULT '{}',
    slice_fingerprints   jsonb NOT NULL DEFAULT '{}'::jsonb,
    unresolved_mentions  jsonb NOT NULL DEFAULT '[]'::jsonb,
    supersedes_packet_id bigint REFERENCES public.packets(id),
    contract_version     text NOT NULL
);

COMMENT ON TABLE public.packets IS
    'A snapshot of a storyline at compile time (contract pk1 — PLAN-one-rail.md §1c). Compiled '
    'in CODE from member editor_reads; zero model tokens. Append-only: a new packet supersedes '
    'the prior one via supersedes_packet_id; rows are never edited.';
COMMENT ON COLUMN public.packets.headline IS
    'Best member title — lowest feed_rank among member articles.';
COMMENT ON COLUMN public.packets.register IS
    'Strongest non-neutral register among members (ep1 enum), with its phrase alongside. '
    'The Influencer owns the score; the packet only carries the Editor''s description.';
COMMENT ON COLUMN public.packets.claims IS
    '[{article_id, source, fact, published_at}] — contested state preserved, attributed '
    '(T3/D6: 0.5–0.75 similarity attaches, never collapses).';
COMMENT ON COLUMN public.packets.facts IS
    'Thin, structured only: {story_types, result_line?, entities}. The Scout never reads packet '
    'prose (T4); confirmed facts reach it by other roads (§4).';
COMMENT ON COLUMN public.packets.quotes IS
    'Code-sliced ±160 chars around first name occurrence in STORED full_text — the model never '
    'emits quotes.';
COMMENT ON COLUMN public.packets.slice_fingerprints IS
    'Per-voice hash of that voice''s slice (E2), keyed by STAGE string (e.g. narratives, '
    'transfers, vibe) — the mig-206 fan-out reads slice_fingerprints ->> stage. A packet re-fans '
    'only to voices whose slice moved.';
COMMENT ON COLUMN public.packets.unresolved_mentions IS
    'B3''s census, rolled up from member editor_reads.resolved.unresolved.';
COMMENT ON COLUMN public.packets.contract_version IS
    'pk1, pk2, ... — a cache key, not a label (T1).';

CREATE INDEX IF NOT EXISTS idx_packets_storyline
    ON public.packets (storyline_id, compiled_at DESC);
CREATE INDEX IF NOT EXISTS idx_packets_day_sport
    ON public.packets (day, sport);
CREATE INDEX IF NOT EXISTS idx_packets_routing_tags
    ON public.packets USING gin (routing_tags);

INSERT INTO public.schema_migrations(version) VALUES ('202_one_rail_packets')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

-- 201_one_rail_editor_reads.sql
--
-- WHAT: `editor_reads` — one row per article, the Editor's read, contract ep1
-- (PLAN-one-rail.md §1a).
--
-- WHY: the greenfield successor to `news_article_readings`. The legacy `article_read` stage
-- keeps writing that table until cutover, and two writers must never share one table — so the
-- Editor gets its own. Column-for-column it mirrors the legacy card where the meaning is
-- unchanged (status taxonomy, fetch provenance, parser_outcome) and diverges where the
-- contract does:
--
--   * `contract_version` ('ep1') replaces prompt_version — it is a CACHE KEY, not a label (T1).
--   * `read` holds the FULL ep1 model envelope (extraction before anything derived).
--   * `resolved` is written by CODE, never the model: exact nrm() surface links, unresolved
--     names for the Investigator, refused ambiguities (T9).
--   * `not_sport` joins the status taxonomy; `not_allowlisted` / `no_vetted_entities` do not
--     carry over — the allowlist and scrub-as-judge die with the legacy rail.
--
-- Deploy order: additive and INERT — no code writes this table until Phase 3 ships the Editor.

BEGIN;

CREATE TABLE IF NOT EXISTS public.editor_reads (
    article_id       bigint PRIMARY KEY REFERENCES public.news_articles(id) ON DELETE CASCADE,
    status           text NOT NULL CHECK (status IN
                     ('success','irrelevant','paywall','blocked','empty_body',
                      'fetch_failed','parse_failed','duplicate','not_sport')),
    contract_version text NOT NULL,
    model_version    text,
    parser_outcome   text NOT NULL DEFAULT 'no_call',
    last_error       text,
    final_url        text,
    final_domain     text,
    content_hash     text,
    extracted_words  integer NOT NULL DEFAULT 0 CHECK (extracted_words >= 0),
    read             jsonb NOT NULL DEFAULT '{}'::jsonb,
    resolved         jsonb NOT NULL DEFAULT '{}'::jsonb,
    storyline_id     bigint REFERENCES public.storylines(id) ON DELETE SET NULL,
    fetched_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.editor_reads IS
    'The Editor''s read of one article (greenfield rail, contract ep1 — PLAN-one-rail.md §1a). '
    'Successor to news_article_readings; the two are never co-written — article_read keeps its '
    'table until cutover, the editor stage writes only this one.';
COMMENT ON COLUMN public.editor_reads.contract_version IS
    'ep1, ep2, ... — a cache key, not a label (T1): bumping it is what reopens work.';
COMMENT ON COLUMN public.editor_reads.read IS
    'The full ep1 model envelope in schema order: source_language, page_kind, names[] (the '
    'discovery channel — name/kind_hint/descriptor), entity_roles[], story_type, result_line '
    '(verbatim-or-empty), register_phrase, register, key_facts[], caveats, evidence_blurb. '
    'The model DESCRIBES; every judgment is derived in code (T2).';
COMMENT ON COLUMN public.editor_reads.resolved IS
    'Resolver outcome, written by CODE: {links:[{entity_type,entity_id,sport,via_surface}], '
    'unresolved:[{name,kind_hint,descriptor}], refused_ambiguous:[...]}. Exact nrm() surface '
    'match is the only automatic link path (T9); ambiguity is refused and nominated.';
COMMENT ON COLUMN public.editor_reads.storyline_id IS
    'Attach result — Phase 6''s Desk writes it after scoring open storylines (§1b).';

CREATE INDEX IF NOT EXISTS idx_editor_reads_status_updated
    ON public.editor_reads (status, updated_at);
CREATE INDEX IF NOT EXISTS idx_editor_reads_resolved
    ON public.editor_reads USING gin (resolved jsonb_path_ops);

INSERT INTO public.schema_migrations(version) VALUES ('201_one_rail_editor_reads')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

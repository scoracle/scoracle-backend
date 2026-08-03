-- 210_investigator_review_views.sql
--
-- WHAT: the Investigator's review surface (PLAN-one-rail 5.8): two SQL views.
-- investigator_review_accepted — the latest accepted candidates WITH their sources, for
-- the 20-row hand-check (one false merge is a stop-the-line event). investigator_funnel —
-- counts by state/kind/day, the health dial for nominations → decisions.
--
-- WHY views: review is a read pattern, not a table; the acquisition tables (mig 205) stay
-- the single source of truth and the views stay cheap to rebuild as the funnel evolves.
--
-- Deploy order: ADDITIVE (CREATE OR REPLACE VIEW), reads only mig-205 tables.

BEGIN;

CREATE OR REPLACE VIEW public.investigator_review_accepted AS
SELECT c.id            AS candidate_id,
       c.norm_name,
       c.kind_hint,
       c.sport,
       c.resolved_entity_type,
       c.resolved_entity_id,
       c.mention_count,
       c.decided_at,
       r.outcome,
       r.query_plan,
       sd.id           AS source_document_id,
       sd.url          AS source_url,
       sd.title        AS source_title,
       sd.fetched_at   AS source_fetched_at
FROM public.entity_candidates c
LEFT JOIN LATERAL (
    SELECT outcome, query_plan FROM public.acquisition_runs
    WHERE candidate_id = c.id ORDER BY finished_at DESC NULLS LAST LIMIT 1
) r ON true
LEFT JOIN LATERAL (
    SELECT sd.* FROM public.entity_aliases ea
    JOIN public.source_documents sd ON sd.id = ea.source_document_id
    WHERE ea.entity_type = c.resolved_entity_type
      AND ea.entity_id = c.resolved_entity_id
    ORDER BY ea.created_at DESC LIMIT 1
) sd ON true
WHERE c.state = 'accepted'
ORDER BY c.decided_at DESC
LIMIT 50;

COMMENT ON VIEW public.investigator_review_accepted IS
    'The 5.8 hand-check surface: latest 50 accepted candidates with their most recent '
    'acquisition run and the source document their aliases cite. One false merge here is '
    'a stop-the-line event — it blocks widening any gate until explained and '
    'regression-fixtured.';

CREATE OR REPLACE VIEW public.investigator_funnel AS
SELECT date(c.last_seen_at) AS day,
       c.state,
       COALESCE(c.kind_hint, 'unknown') AS kind_hint,
       c.sport,
       count(*)             AS candidates,
       sum(c.mention_count) AS mentions
FROM public.entity_candidates c
GROUP BY 1, 2, 3, 4;

COMMENT ON VIEW public.investigator_funnel IS
    'Nomination → decision funnel by day/state/kind/sport (PLAN-one-rail 5.8). The census '
    'classes (rejected_out_of_scope clubs/NTs, D-3) appear here and nowhere else.';

INSERT INTO public.schema_migrations(version) VALUES ('210_investigator_review_views')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

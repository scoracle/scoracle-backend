-- One bounded cohort producer. No dirty-marker acknowledgement or cognition fanout.
BEGIN;
CREATE TABLE public.analytics_cohort_publication (
 sport text NOT NULL REFERENCES public.sports(id),
 entity_type text NOT NULL CHECK (entity_type IN ('player','team')),
 season integer NOT NULL,
 as_of timestamptz NOT NULL,
 captured_at timestamptz NOT NULL,
 input_hash text NOT NULL,
 result_hash text NOT NULL,
 formula text NOT NULL,
 mvcc_snapshot text NOT NULL,
 row_count integer NOT NULL CHECK (row_count >= 0),
 published_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY (sport,entity_type,season)
);
COMMENT ON TABLE public.analytics_cohort_publication IS
 'Current complete cohort publication receipt. analytics_entity_context is its direct serving projection; both replace atomically. Content hash covers both input seasons including NULLs and deletions.';
INSERT INTO public.schema_migrations(version) VALUES ('262_analytics_cohort_publication') ON CONFLICT DO NOTHING;
COMMIT;

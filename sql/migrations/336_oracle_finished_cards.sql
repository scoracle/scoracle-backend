BEGIN;
SET LOCAL lock_timeout = '2s';
CREATE OR REPLACE FUNCTION public.coverage_dependencies(task text) RETURNS text[]
LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN task='sigil' AND (SELECT enabled FROM public.coverage_schedule)
            THEN ARRAY['narratives','rating','vibe','momentum','transfers']
        WHEN task='momentum' THEN ARRAY['rating','vibe']
        ELSE ARRAY[]::text[]
    END
$$;
INSERT INTO public.schema_migrations(version) VALUES ('336_oracle_finished_cards') ON CONFLICT DO NOTHING;
COMMIT;

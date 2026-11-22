BEGIN;
UPDATE public.coverage_schedule SET enabled=false;
DO $$ BEGIN
    IF cardinality(public.coverage_dependencies('sigil')) <> 0 OR
       public.coverage_dependencies('momentum') <> ARRAY['rating','vibe'] THEN
        RAISE EXCEPTION 'Continuous Oracle may use finished cards while Analyst follows its producers';
    END IF;
END $$;
UPDATE public.coverage_schedule SET enabled=true, enrolled_at=now();
DO $$ BEGIN
    IF public.coverage_dependencies('sigil') <> ARRAY['narratives','rating','vibe','momentum','transfers'] THEN
        RAISE EXCEPTION 'A scheduled Oracle edition must wait for its upstream stages';
    END IF;
END $$;
ROLLBACK;

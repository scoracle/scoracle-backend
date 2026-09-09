-- Run after migration 251. All test writes roll back.
\set ON_ERROR_STOP on
BEGIN;
SET LOCAL lock_timeout = '5s';

DO $$
DECLARE
    v_id integer;
    v_sport text;
BEGIN
    SELECT id, sport INTO STRICT v_id, v_sport
    FROM public.teams ORDER BY sport, id LIMIT 1;

    UPDATE public.teams
    SET primary_color = '#123456', secondary_color = '#abcdef',
        color_source = 'constraint test, rolled back'
    WHERE id = v_id AND sport = v_sport;

    BEGIN
        UPDATE public.teams SET primary_color = 'red'
        WHERE id = v_id AND sport = v_sport;
        RAISE EXCEPTION 'Invalid primary color was accepted';
    EXCEPTION WHEN check_violation THEN NULL;
    END;

    BEGIN
        UPDATE public.teams SET secondary_color = '#xyzxyz'
        WHERE id = v_id AND sport = v_sport;
        RAISE EXCEPTION 'Invalid secondary color was accepted';
    EXCEPTION WHEN check_violation THEN NULL;
    END;

    BEGIN
        UPDATE public.teams SET secondary_color = NULL
        WHERE id = v_id AND sport = v_sport;
        RAISE EXCEPTION 'Partial palette was accepted';
    EXCEPTION WHEN check_violation THEN NULL;
    END;

    BEGIN
        UPDATE public.teams SET color_source = ' '
        WHERE id = v_id AND sport = v_sport;
        RAISE EXCEPTION 'Palette without provenance was accepted';
    EXCEPTION WHEN check_violation THEN NULL;
    END;

    UPDATE public.teams
    SET primary_color = NULL, secondary_color = NULL, color_source = NULL
    WHERE id = v_id AND sport = v_sport;
END;
$$;

ROLLBACK;
SELECT 'Team palette constraints passed; all writes rolled back' AS result;

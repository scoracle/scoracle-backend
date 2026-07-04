-- 123_fix_autofill_refresh_conflict_target.sql
--
-- Fix complete_sport_autofill_refresh under PL/pgSQL name resolution. The
-- RETURNS TABLE column `sport` can make `ON CONFLICT (sport)` ambiguous, so use
-- the named primary-key constraint as the conflict target.

BEGIN;

CREATE OR REPLACE FUNCTION public.complete_sport_autofill_refresh(
    p_sport TEXT,
    p_total_entities INTEGER,
    p_reason TEXT DEFAULT NULL
)
RETURNS TABLE (sport TEXT, version BIGINT, generated_at TIMESTAMPTZ, total_entities INTEGER, status TEXT, reason TEXT) AS $$
BEGIN
    IF p_sport NOT IN ('NBA', 'NFL', 'FOOTBALL') THEN
        RAISE EXCEPTION 'unsupported sport for autofill refresh: %', p_sport;
    END IF;

    RETURN QUERY
    INSERT INTO public.sport_autofill_versions AS sav (
        sport, version, generated_at, total_entities, status, reason
    )
    VALUES (p_sport, 1, NOW(), p_total_entities, 'ready', p_reason)
    ON CONFLICT ON CONSTRAINT sport_autofill_versions_pkey DO UPDATE SET
        version = sav.version + 1,
        generated_at = NOW(),
        total_entities = EXCLUDED.total_entities,
        status = 'ready',
        reason = EXCLUDED.reason
    RETURNING sav.sport, sav.version, sav.generated_at, sav.total_entities, sav.status, sav.reason;
END;
$$ LANGUAGE plpgsql;

COMMIT;

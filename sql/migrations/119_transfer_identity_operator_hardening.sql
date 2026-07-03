-- 119_transfer_identity_operator_hardening.sql
--
-- Production hardening for applied-transfer current identity:
--   * keep transfer application/revert writes inside the DB functions
--   * move materialized-view refresh work to callers that can run
--     REFRESH MATERIALIZED VIEW CONCURRENTLY outside a function transaction
--   * preserve sport-scoped autofill version/status observability

BEGIN;

CREATE OR REPLACE FUNCTION public.request_sport_autofill_refresh(
    p_sport TEXT,
    p_reason TEXT DEFAULT NULL
)
RETURNS VOID AS $$
BEGIN
    IF p_sport NOT IN ('NBA', 'NFL', 'FOOTBALL') THEN
        RAISE EXCEPTION 'unsupported sport for autofill refresh: %', p_sport;
    END IF;

    INSERT INTO public.sport_autofill_versions (sport, status, reason, generated_at)
    VALUES (p_sport, 'refreshing', p_reason, NOW())
    ON CONFLICT (sport) DO UPDATE SET
        status = 'refreshing',
        reason = EXCLUDED.reason,
        generated_at = NOW();
END;
$$ LANGUAGE plpgsql;

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
    ON CONFLICT (sport) DO UPDATE SET
        version = sav.version + 1,
        generated_at = NOW(),
        total_entities = EXCLUDED.total_entities,
        status = 'ready',
        reason = EXCLUDED.reason
    RETURNING sav.sport, sav.version, sav.generated_at, sav.total_entities, sav.status, sav.reason;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION public.fail_sport_autofill_refresh(
    p_sport TEXT,
    p_reason TEXT
)
RETURNS VOID AS $$
BEGIN
    IF p_sport NOT IN ('NBA', 'NFL', 'FOOTBALL') THEN
        RAISE EXCEPTION 'unsupported sport for autofill refresh: %', p_sport;
    END IF;

    INSERT INTO public.sport_autofill_versions (sport, status, reason, generated_at)
    VALUES (p_sport, 'failed', p_reason, NOW())
    ON CONFLICT (sport) DO UPDATE SET
        status = 'failed',
        reason = EXCLUDED.reason,
        generated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Compatibility shim for the Phase 1 DB functions. They call this while still
-- inside the apply/revert transaction; the Rust stage and operator CLI finish
-- the refresh with REFRESH MATERIALIZED VIEW CONCURRENTLY after the function
-- call returns.
CREATE OR REPLACE FUNCTION public.refresh_sport_autofill(
    p_sport TEXT,
    p_reason TEXT DEFAULT NULL
)
RETURNS VOID AS $$
BEGIN
    PERFORM public.request_sport_autofill_refresh(p_sport, p_reason);
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION public.refresh_sport_autofill(TEXT, TEXT) IS
  'Transaction-safe autofill invalidation shim. Marks sport_autofill_versions refreshing; callers must run REFRESH MATERIALIZED VIEW CONCURRENTLY outside the DB function and then call complete_sport_autofill_refresh().';

INSERT INTO public.schema_migrations(version)
VALUES ('119_transfer_identity_operator_hardening')
ON CONFLICT (version) DO NOTHING;

COMMIT;

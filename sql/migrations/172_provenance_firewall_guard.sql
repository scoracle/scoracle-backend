-- 172_provenance_firewall_guard.sql
--
-- Cognition refactor, Phase 0 (groundwork, no behavior change): make the provenance
-- partition STRUCTURAL, not just conventional (plan decision 1).
--
-- Context: mig 170 already established the partition. Junction verdicts are banked into
-- narrative_events tagged origin='junction' (transfer.rs::bank_transfer_junction_event),
-- and every MEASUREMENT consumer of narrative_events filters origin='extraction' so the
-- model's own conclusions can never re-enter the numeric loop (the echo-chamber rule). The
-- two live consumers are refresh_typed_links (events -> typed links) and
-- score_transfer_likelihood (events -> likelihood language input). Episodes derive from
-- links (already filtered) or backfill from the raw news rail, so they are transitively
-- protected and read no junction events directly.
--
-- This migration installs assert_provenance_firewall(): a reusable guard that RAISEs if any
-- named measurement consumer's CURRENT body reads narrative_events without an
-- origin='extraction' filter. It is called once here (fail-closed on today's schema) and is
-- meant to be re-invoked (PERFORM public.assert_provenance_firewall();) by any FUTURE
-- migration that rebuilds a measurement consumer -- so drift is caught at apply time, the
-- moment it is introduced, exactly like the 045 smoke-gate discipline.
--
-- To register a new measurement consumer, add its name to v_consumers below (rebuild the
-- function) in the same migration that introduces it.
--
-- ADDITIVE / idempotent: installs a function and asserts. No table or data change.

BEGIN;

CREATE OR REPLACE FUNCTION public.assert_provenance_firewall()
RETURNS void
LANGUAGE plpgsql
AS $fn$
DECLARE
    -- The measurement-side readers of narrative_events. Each MUST filter origin='extraction'
    -- so junction-authored events (mig 170) stay invisible to the numeric feedback loop.
    v_consumers text[] := ARRAY['refresh_typed_links', 'score_transfer_likelihood'];
    v_name  text;
    v_body  text;
    v_norm  text;
    v_missing text[] := ARRAY[]::text[];
    v_absent  text[] := ARRAY[]::text[];
BEGIN
    FOREACH v_name IN ARRAY v_consumers LOOP
        -- Concatenate all overloads' definitions (there is normally one live overload).
        SELECT string_agg(pg_get_functiondef(p.oid), E'\n')
          INTO v_body
          FROM pg_proc p
          JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'public' AND p.proname = v_name;

        IF v_body IS NULL THEN
            v_absent := array_append(v_absent, v_name);
            CONTINUE;
        END IF;

        -- A consumer that no longer reads narrative_events at all is not a leak; only flag
        -- one that reads the table WITHOUT the extraction filter.
        v_norm := regexp_replace(lower(v_body), '\s+', '', 'g');
        IF position('narrative_events' IN v_norm) > 0
           AND position('origin=''extraction''' IN v_norm) = 0 THEN
            v_missing := array_append(v_missing, v_name);
        END IF;
    END LOOP;

    IF array_length(v_absent, 1) > 0 THEN
        RAISE EXCEPTION 'provenance firewall: measurement consumer(s) not found in public schema: %',
            array_to_string(v_absent, ', ')
            USING HINT = 'assert_provenance_firewall() names a function that no longer exists; update v_consumers if it was intentionally removed/renamed.';
    END IF;

    IF array_length(v_missing, 1) > 0 THEN
        RAISE EXCEPTION 'provenance firewall breached: %() read narrative_events without an origin=''extraction'' filter',
            array_to_string(v_missing, ', ')
            USING HINT = 'A junction-authored event (origin=''junction'', mig 170) could re-enter the numeric loop. Re-add the origin=''extraction'' filter to the consumer''s narrative_events scan.';
    END IF;
END;
$fn$;

COMMENT ON FUNCTION public.assert_provenance_firewall() IS
    'Provenance firewall guard (mig 172, plan decision 1): RAISEs if any measurement consumer '
    'of narrative_events (refresh_typed_links, score_transfer_likelihood) reads the table without '
    'an origin=''extraction'' filter, which would let junction-authored events (mig 170) re-enter '
    'the numeric loop. Call PERFORM public.assert_provenance_firewall() at the end of any future '
    'migration that rebuilds a measurement consumer.';

-- Fail closed on today's schema: this must pass against mig 170's definitions.
SELECT public.assert_provenance_firewall();

INSERT INTO public.schema_migrations(version) VALUES ('172_provenance_firewall_guard')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

-- 206_packet_routing.sql
--
-- WHAT: `enqueue_voices_on_packet` — AFTER INSERT ON packets, fan work out to the voices
-- (PLAN-one-rail.md 1.7).
--
-- WHY: when the Desk compiles a packet, the voices whose slice moved should wake. Two arms:
--
--   1. Tag-subscribed voices: each routing_tag joins stage_routing_subscriptions (mig 197's
--      table — routing stays DATA), and every ACTIVE storyline participant of that stage's
--      grain gets a work item.
--   2. The Journalist reads everything: an unconditional `narratives` fan-out per active
--      player/team participant, no subscription row needed.
--
-- input_version is 'pk:' || the packet's slice fingerprint for that stage (E2). A superseding
-- packet whose slice did not move produces the SAME input_version, and mig 197's ON CONFLICT
-- guard debounces it — unchanged slices do not reopen work. slice_fingerprints keys are STAGE
-- strings (see mig 202's column comment; §1c's {journalist: ...} example is character
-- shorthand — the Phase 6 compiler writes stage-keyed entries). A MISSING fingerprint falls
-- back to 'pk:' || packet id, which re-fans on every packet: fail-open — a voice may read an
-- unchanged slice twice, but never starves.
--
-- The narratives arm skips 'person' participants: voices are player/team-grain, and
-- pipeline_work's CHECK deliberately does not admit 'person' (1.5 — only what v1 writes).
--
-- ## Inert on arrival, doubly
--
-- Ships live but fires into an EMPTY subscription table (arm 1 fans to nobody) and ZERO
-- packets (the trigger never fires at all until Phase 6 compiles one). Do NOT seed
-- subscriptions here — that is Phase 7, deliberately (the mig-197 churn-loop lesson: seeding
-- alongside a live legacy enqueue path alternates fingerprints and reopens work forever).
--
-- Deploy order: additive and inert as above.

BEGIN;

CREATE OR REPLACE FUNCTION public.enqueue_voices_on_packet() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Arm 1: tag-subscribed voices. DISTINCT because two tags can reach the same (stage,
    -- entity) pair — input_version is stage-keyed, so the rows are identical and collapse
    -- (without it, ON CONFLICT would hit "cannot affect row a second time").
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT DISTINCT s.stage, se.entity_type, se.entity_id, se.sport, 'pending',
           'pk:' || COALESCE(NEW.slice_fingerprints ->> s.stage, NEW.id::text),
           NOW(), NOW()
      FROM unnest(NEW.routing_tags) AS t(tag)
      JOIN public.stage_routing_subscriptions s ON s.tag = t.tag
      JOIN public.storyline_entities se
        ON se.storyline_id = NEW.storyline_id
       AND se.entity_type  = s.entity_type
       AND se.left_at IS NULL
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Arm 2: the Journalist reads everything (§1c — an unconditional narratives fan-out per
    -- active participant, at the player/team grain the voices are keyed on).
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT 'narratives', se.entity_type, se.entity_id, se.sport, 'pending',
           'pk:' || COALESCE(NEW.slice_fingerprints ->> 'narratives', NEW.id::text),
           NOW(), NOW()
      FROM public.storyline_entities se
     WHERE se.storyline_id = NEW.storyline_id
       AND se.left_at IS NULL
       AND se.entity_type IN ('player','team')
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- The statement-level mig-133 notify trigger on pipeline_work already covers the wake-up;
    -- identical (channel, payload) notifications coalesce within a transaction, so this
    -- explicit notify is free — kept because it documents the wake-up contract at the seam.
    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS enqueue_voices_on_packet ON public.packets;
CREATE TRIGGER enqueue_voices_on_packet
    AFTER INSERT ON public.packets
    FOR EACH ROW
    EXECUTE FUNCTION public.enqueue_voices_on_packet();

INSERT INTO public.schema_migrations(version) VALUES ('206_packet_routing')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

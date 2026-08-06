-- 212_packet_narratives_subscription_gate.sql
--
-- WHAT: rebuild `enqueue_voices_on_packet` (mig 206) so ARM 2 — the Journalist's `narratives`
-- fan-out — is subscription-gated, exactly as arm 1 already is. Arm 1 is untouched; the
-- function is otherwise byte-identical to the current prod definition (derived from
-- pg_get_functiondef, per the template).
--
-- WHY: this is D-T14 resolution (b), and it is the precondition for compiling a packet at all.
-- Mig 206 arm 2 fans `narratives` UNCONDITIONALLY on every packet insert, at the same
-- (stage, entity_type, entity_id, sport) key the legacy `article_read` seat enqueues with
-- `n:<hash>` (article_reader/mod.rs:1319). Turn COGNITION_PACKET_COMPILE on under RAIL=legacy
-- and the two writers alternate `pk:` and `n:` on one pipeline_work row forever — the mig-197
-- churn loop. That is why zero packets have ever been compiled, and why the packet branches
-- have never executed in production. With arm 2 gated and the subscription table EMPTY, the
-- whole trigger becomes a no-op and packets can compile in shadow: the compiler, the storyline
-- assembler and the renderer all run on real data, and no pipeline_work row is touched.
--
-- ## What the gate is, precisely
--
-- `EXISTS (subscription row with stage='narratives' AND entity_type = se.entity_type)`.
--
-- Arm 2 keeps §1c's contract — the Journalist reads EVERY packet, not only tagged ones, and
-- still at player/team grain per active participant. The gate adds no tag condition: the row's
-- `tag` column is NOT consulted by arm 2. This is the opposite of the '*' trap in D-T15 (where
-- a row was expected to match every tag and matched none) and must not be read as a wildcard —
-- one narratives row at a grain arms that grain unconditionally.
--
-- ## Consequence for the cutover, do not lose it
--
-- Under RAIL=packet the Journalist now has NO waker until a narratives subscription exists, the
-- same way 7.6 left the Influencer. The Phase 8 seed therefore needs FOUR rows, not two:
-- ('charged','vibe',player|team) AND ('narratives','narratives',player|team). Seeding, dropping
-- mig 175's trigger and flipping RAIL stay ONE act — the seed also arms mig 197's live
-- article-grain trigger (D-T15), so it cannot land before the legacy writers stop.
--
-- Deploy order: additive (CREATE OR REPLACE of an inert trigger function; 0 packets exist).
-- Safe to apply before any restart. Mirrored into sql/platform.sql alongside mig 206.

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
    -- active participant, at the player/team grain the voices are keyed on) — but only once a
    -- narratives subscription exists at that grain (mig 212, D-T14 (b)). The tag column is NOT
    -- read here: this is an existence gate, not a tag join. Empty table = inert trigger =
    -- packets compile in shadow without touching pipeline_work.
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT 'narratives', se.entity_type, se.entity_id, se.sport, 'pending',
           'pk:' || COALESCE(NEW.slice_fingerprints ->> 'narratives', NEW.id::text),
           NOW(), NOW()
      FROM public.storyline_entities se
     WHERE se.storyline_id = NEW.storyline_id
       AND se.left_at IS NULL
       AND se.entity_type IN ('player','team')
       AND EXISTS (
           SELECT 1
             FROM public.stage_routing_subscriptions s
            WHERE s.stage       = 'narratives'
              AND s.entity_type = se.entity_type)
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

COMMENT ON FUNCTION public.enqueue_voices_on_packet() IS 'AFTER INSERT ON packets: fan work to the voices (PLAN-one-rail 1.7, mig 206). Arm 1 = tag-subscribed stages via stage_routing_subscriptions joined on tag, at that stage''s grain. Arm 2 = the Journalist''s narratives fan-out over every active player/team participant, tag-independent, gated since mig 212 (D-T14 resolution (b)) on the EXISTENCE of a stage_routing_subscriptions row with stage=''narratives'' at that entity_type — an existence gate, NOT a tag join, and NOT a wildcard (contrast D-T15''s ''*'' trap). Both arms carry input_version ''pk:'' || the packet''s slice fingerprint for that stage, falling back to the packet id (fail-open: re-read, never starve). With the subscription table empty the whole trigger is inert, which is what lets packets compile in shadow under RAIL=legacy without fighting the legacy article_read enqueue over one pipeline_work row.';

-- Self-record INSIDE the transaction so apply + record are atomic.
INSERT INTO public.schema_migrations(version) VALUES ('212_packet_narratives_subscription_gate')
    ON CONFLICT DO NOTHING;

COMMIT;

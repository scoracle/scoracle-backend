-- 229_player_availability.sql
--
-- The availability record — the missing step C of Scott's Editor→Scout chain (2026-08-23:
-- "Editor identifies that an event took place → Investigator scrapes → SQL updates → Scout is
-- enqueued... I think we should use this same strategy for injuries/suspensions").
--
-- For a box score, step C is the stats write. For an injury there was NO step C: no injury or
-- suspension table or column existed anywhere in the ledger, and `load_personnel_changes` —
-- the query that IS the Scout's personnel block — selects from `transfer_identity_applications`
-- only. So an injury-triggered Scout would have woken to a card containing zero availability
-- facts, and his s21 rule ("Availability is part of the profile... never speculate past what is
-- recorded") correctly forbids him from inventing any. The trigger is the LAST step of that
-- chain, not the first. This is the first.
--
-- ## Two consumers, and the second one fixes the shape
--
-- Scott, same day: "It'll also let us know if an entity has an injury propensity." That is why
-- this is an EVENT LOG WITH CLOSED SPANS and not a status flag on `players` — a flag has no
-- history to count. The two reads are deliberately different seats' material:
--
--   * CURRENT availability  → the Scout's personnel block (the delta since his last read)
--   * PROPENSITY            → the cross-season memory card (the slow arc)
--
-- `load_personnel_changes`'s own comment already drew that line: "The memory card keeps its
-- slow cross-season arc lines; this block is the delta." Both reads ride `with_enrichment`,
-- prompt-only and OUTSIDE `input_components`/`input_hash`, so neither re-mints the fleet's
-- hash pre-image — the trap that a fleet-wide regeneration waits behind.
--
-- ## The one place copying `transfer_identity_applications` verbatim is WRONG
--
-- That table has `reverted_at` meaning THE ADJUDICATION WAS WRONG. An injury needs that too,
-- but it also needs THE PLAYER CAME BACK, which is a real-world outcome and not a correction.
-- They are separate columns here (`reverted_at` vs `returned_at`) because collapsing them
-- corrupts propensity at the root: a retracted false report and a genuine three-week absence
-- would be indistinguishable, and days-out would be computed off both. `expected_return` is a
-- THIRD thing again — the prognosis as reported, fine to render on a card, never ground truth.
--
-- ## Deploy order
--
-- Additive, no backfill, nothing reads it yet ⇒ migrate BEFORE the release, the 226 pattern.
-- This migration deliberately ships NO trigger and NO `stage_routing_subscriptions` row: the
-- Scout's enqueue is Rust (`rating_work_input_version_for_availability`), because the packet
-- trigger hardcodes a `'pk:'` input_version that can neither carry the debounce-bypass marker
-- nor collapse per-day. See progress_docs/2026-08-23_editor-to-scout-availability-plan.md.

BEGIN;

CREATE TABLE IF NOT EXISTS public.player_availability (
    id                bigserial PRIMARY KEY,
    sport             text    NOT NULL,
    player_id         integer NOT NULL,
    -- The club the player was at when the event happened. Nullable for the same reason the
    -- transfer table's `old_team_id` is: the resolver may not have a club yet, and an
    -- unattached player can still be suspended.
    team_id           integer,

    kind              text    NOT NULL,
    status            text    NOT NULL DEFAULT 'pending',

    -- THE DAY. A date, never a timestamp: the Scout's enqueue marker is keyed on this value,
    -- and every injury for an entity on one date must produce ONE input_version so
    -- `work::enqueue`'s conflict clause collapses them into a single run (Scott: "on an event
    -- day, the Scout is enqueued one time instead of multiple"). A timestamp — or a date
    -- rendered from a local zone rather than a fixed one — splits one event day across two
    -- versions and the collapse silently stops collapsing.
    event_date        date    NOT NULL,

    -- The prognosis AS REPORTED at event time. A claim, not an outcome.
    expected_return   date,
    -- Availability actually resumed. The propensity denominator; NULL = still out (an OPEN
    -- span, which every row will be on day one — propensity has nothing to say until this
    -- column has values).
    returned_at       date,

    -- This RECORD was wrong — a correction, NOT a return. Never compute days-out from it.
    reverted_at       timestamptz,
    reverted_by       text,
    revert_reason     text,

    -- Recurrence — the propensity signal people actually want — needs this, but the Editor
    -- cannot supply it today: `story_type` is a single enum value with no structured detail,
    -- so extracting a body part means a new field on his contract, which is a prompt change
    -- governed by the prompt-vs-guard law. The column ships EMPTY and is NEVER guessed; a
    -- later backfill is then data rather than a migration on a live table. Propensity v1 is
    -- frequency and days-out, not "hamstring recurrence".
    body_part         text,

    source_article_id bigint,
    applied_at        timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT player_availability_kind_check
        CHECK (kind = ANY (ARRAY['injury'::text, 'suspension'::text])),
    CONSTRAINT player_availability_status_check
        CHECK (status = ANY (ARRAY['pending'::text, 'applied'::text, 'rejected'::text])),
    -- A span cannot close before it opens. Cheap, and it is the invariant every propensity
    -- number depends on.
    CONSTRAINT player_availability_returned_after_event
        CHECK (returned_at IS NULL OR returned_at >= event_date),
    CONSTRAINT player_availability_expected_after_event
        CHECK (expected_return IS NULL OR expected_return >= event_date)
);

-- The natural key: one row per (player, kind, day). Five outlets reporting the same Saturday
-- hamstring is ONE event, and the adjudicator upserts onto this key to enrich (fill
-- `expected_return`, later `body_part`) rather than accumulating duplicates that would each
-- read as a separate injury and inflate propensity. `kind` is in the key so an injury and a
-- suspension on the same day remain two facts, which they are.
CREATE UNIQUE INDEX IF NOT EXISTS player_availability_natural_key
    ON public.player_availability (sport, player_id, kind, event_date);

-- The Scout's player-grain delta read: "what moved since my last card", which filters on
-- applied_at / reverted_at against a `since` bound.
CREATE INDEX IF NOT EXISTS player_availability_player_applied
    ON public.player_availability (sport, player_id, applied_at DESC)
    WHERE status = 'applied';

-- The team-grain arm of the same read — a club needs to see its own absentees, the way
-- `load_personnel_changes` matches a team against both sides of a transfer.
CREATE INDEX IF NOT EXISTS player_availability_team_applied
    ON public.player_availability (sport, team_id, applied_at DESC)
    WHERE status = 'applied' AND team_id IS NOT NULL;

-- Open spans: who is out RIGHT NOW, and the rows a return-scraper has to close.
CREATE INDEX IF NOT EXISTS player_availability_open
    ON public.player_availability (sport, player_id, event_date DESC)
    WHERE status = 'applied' AND returned_at IS NULL AND reverted_at IS NULL;

COMMENT ON TABLE public.player_availability IS
    'Adjudicated injury and suspension events — the availability half of the Scout''s personnel '
    'record, alongside transfer_identity_applications (mig 229). An EVENT LOG, not a status '
    'flag: current availability feeds the Scout''s personnel block (the delta since his last '
    'read) and closed spans feed injury propensity on the cross-season memory card (the arc). '
    'T4 holds — every column here is a date, an id, or an enum; no prose from the Editor '
    'reaches the Scout through this table.';

COMMENT ON COLUMN public.player_availability.event_date IS
    'The DAY the player became unavailable. A date, not a timestamp: the Scout''s enqueue '
    'marker (rating_work_input_version_for_availability) keys on it so every event for an '
    'entity on one day collapses to a single work row — Scott''s once-per-event-day rule.';
COMMENT ON COLUMN public.player_availability.expected_return IS
    'The prognosis AS REPORTED at event time. A claim, renderable on the card ("expected back '
    'Sep 02"); NEVER ground truth — propensity is computed from returned_at only.';
COMMENT ON COLUMN public.player_availability.returned_at IS
    'Availability actually resumed — a real-world outcome. NULL = still out (open span). This '
    'is the propensity denominator, and it is deliberately NOT reverted_at.';
COMMENT ON COLUMN public.player_availability.reverted_at IS
    'This RECORD was wrong and has been undone — a correction, not a return. Kept distinct from '
    'returned_at because conflating them would make a retracted false report and a genuine '
    'absence indistinguishable, corrupting every days-out number built on this table.';
COMMENT ON COLUMN public.player_availability.body_part IS
    'Ships EMPTY and is never guessed. Recurrence needs it, but the Editor emits story_type as a '
    'bare enum with no structured detail, so populating it requires a new field on his contract. '
    'Present now so the later backfill is data rather than a migration on a live table.';

INSERT INTO public.schema_migrations(version) VALUES ('229_player_availability')
    ON CONFLICT DO NOTHING;

COMMIT;

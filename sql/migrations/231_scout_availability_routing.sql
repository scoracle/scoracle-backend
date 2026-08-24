-- 231_scout_availability_routing.sql
--
-- The Editor's TAG. Scott, 2026-08-23: "Editor notices injury/suspension and tags the Scout →
-- the Scout decides the legitimacy of the report → event is included in the report… I'd like to
-- empower each model. Guards over evals. We can let the model do the work versus trying to
-- engineer a rigid process."
--
-- ## Why this INSERT is correct NOW, when the same INSERT was judged wrong three days ago
--
-- The 2026-08-23 availability plan rejected the subscription route on two measured grounds, and
-- BOTH were consequences of one missing thing rather than of the route itself:
--
--   1. "It cannot mint the marker, so `skip_unchanged` stays on and NOTHING runs."
--   2. "There is no `rating` slice fingerprint, so it falls back to the packet id and every
--      injury packet that day is a SEPARATE enqueue" — breaking the once-per-event-day rule.
--
-- The missing thing was the `rating` key in `packets.slice_fingerprints`. It exists as of this
-- release (`editor/packet.rs`), hashing the injury- and suspension-typed claims and nothing else.
-- With it present:
--
--   * mig 225's `enqueue_voices_on_packet` mints `'pk:' || slice_fingerprints->>'rating'`, so the
--     work row's `input_version` IS the claim hash. Five outlets reporting one knock produce one
--     claim set, one fingerprint, ONE enqueue — Scott's once-per-event-day rule met by CONTENT
--     rather than by a calendar key, which also holds across days when nothing new is said and
--     correctly re-fires the moment a new fact (a return, a longer prognosis) lands.
--   * `rating_work_bypasses_debounce` now treats a `pk:` version as non-statistical, so the
--     reopened row reaches the model call instead of being short-circuited. That was objection 1.
--
-- So this supersedes the Rust-side enqueue helpers built for the adjudicated path
-- (`rating_work_input_version_for_availability`, `enqueue_rating_for_applied_availability`).
-- They are left in place and still work — an APPLIED `player_availability` row is a different,
-- stronger trigger than a report, and mig 229's record remains the propensity substrate. What
-- changes is that a report no longer has to become an adjudicated row before the Scout may see
-- it, which is the rigid process the ruling rejected.
--
-- ## Two rows, not one
--
-- Neither trigger reads '*' as a wildcard (D-T15), so player and team grain each need their own
-- row — the same reason `charged`/`vibe` and `narratives` ship as pairs.
--
-- ## One tag covers both causes
--
-- The routing tag is `injury`, and the Editor deliberately tags a suspension with it too: an
-- injury and a suspension are different causes with the same consequence — the player is
-- unavailable — and the Scout's slice admits both `story_type`s. A separate `suspension` tag
-- would split one squad fact across two wakeups.
--
-- Deploy order: ships WITH the release that adds the `rating` slice. Applied before it, the
-- fingerprint key is absent, the version falls back to the packet id, and objection 2 above is
-- live again — noisy, not corrupting, but pointless.

BEGIN;

INSERT INTO public.stage_routing_subscriptions (tag, stage, entity_type, note)
VALUES
    ('injury', 'rating', 'player',
     'PLAN-one-rail — the Scout''s availability tag (Scott 2026-08-23: the Editor tags, the '
     'Scout judges legitimacy). Fires off slice_fingerprints->>''rating'', which hashes the '
     'injury/suspension claims, so the enqueue collapses per CHANGE OF FACT rather than per '
     'packet. Carries suspensions too — one tag, one consequence.'),
    ('injury', 'rating', 'team',
     'PLAN-one-rail — same, at team grain. Two rows because neither trigger reads ''*'' as a '
     'wildcard (D-T15). A club needs to see its own absentees.')
ON CONFLICT DO NOTHING;

-- Assert the tag actually routes, rather than trusting the INSERT (the 045 parity-gate habit).
DO $$
DECLARE
    n integer;
BEGIN
    SELECT count(*) INTO n
      FROM public.stage_routing_subscriptions
     WHERE stage = 'rating' AND tag = 'injury'
       AND entity_type IN ('player', 'team');
    IF n <> 2 THEN
        RAISE EXCEPTION 'scout availability routing did not land: % of 2 rows', n;
    END IF;
END $$;

INSERT INTO public.schema_migrations(version) VALUES ('231_scout_availability_routing')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

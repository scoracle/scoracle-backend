-- 233_import_rail_identity.sql
--
-- The fantasy-data rail, part 1 of N (PLAN-weekly-fantasy-rail.md rev 2): identity
-- plumbing for the gap-driven importer, plus the one piece of box-score demolition
-- that cannot wait.
--
-- The importer (go/internal/dataimport, pipeline -mode data) resolves every external
-- handle — an nflverse gsis id, a team abbreviation, a schedule game_id — through
-- entity_external_ids, the table the Investigator already writes and whose comment
-- names exactly this hook: "a person-kind player later appearing in a box score
-- reconciles by alias/external id (5.5)". No new identity table; the import sources
-- are just three more namespaces. What was missing:
--
--   1. A conflict target. The table has only a lookup index on (namespace,
--      external_id); the importer needs idempotent upserts. Partial unique over the
--      import namespaces only, so nothing the Investigator writes (wikidata etc.,
--      historically unconstrained) can start failing.
--
--   2. players.id cannot mint ids — no sequence, no default. Every existing row's id
--      was assigned by the demolished Python seeder, and nothing in the live system
--      creates players at all (the Investigator creates persons, not players). The
--      importer must create players (roster arrivals, call-ups), so players.id gets
--      the ordinary serial treatment, seeded past MAX(id).
--
--      Deliberately NOT a voice-work trigger anywhere on this path: players/teams/
--      team_rosters have no enqueue triggers (verified 2026-09-04), and it must stay
--      that way — entity EXISTENCE comes from data, entity STORIES stay news-driven.
--
--   3. Demolition brought forward: fixture_boxscore_enqueue_on_final. The
--      fixture_boxscore stage has been absent from COGNITION_STAGES since the 08-23
--      turn and its parser is hardcoded not_supported — every row this trigger
--      enqueues is dead on arrival (159 queued today, none will ever drain). The
--      importer is about to mark hundreds of fixtures completed/seeded, each of
--      which would fire it. Drop the trigger, its two functions, and the dead queue
--      rows now; the rest of the substrate (boxscore_sources,
--      fixture_boxscore_fetches, boxscore.rs) falls in the full demolition once the
--      rail carries all three sports. fixture_boxscore_input_version() stays for
--      that migration — boxscore.rs still references it and the daemon must keep
--      compiling until then.

BEGIN;

-- (1) Idempotent upsert target for the import namespaces.
CREATE UNIQUE INDEX IF NOT EXISTS uq_entity_external_ids_import
    ON public.entity_external_ids (namespace, entity_type, external_id)
    WHERE namespace IN ('nflverse', 'nba', 'fpl');

-- (2) players.id mints its own ids from here on.
CREATE SEQUENCE IF NOT EXISTS public.players_id_seq OWNED BY public.players.id;
SELECT setval('public.players_id_seq',
              COALESCE((SELECT MAX(id) FROM public.players), 0) + 1,
              false);
ALTER TABLE public.players
    ALTER COLUMN id SET DEFAULT nextval('public.players_id_seq');

-- (3) The dead enqueue path.
DROP TRIGGER IF EXISTS fixture_boxscore_enqueue_on_final ON public.fixtures;
DROP FUNCTION IF EXISTS public.enqueue_fixture_boxscore_on_final();
DROP FUNCTION IF EXISTS public.enqueue_fixture_boxscore(integer);
DELETE FROM public.pipeline_work WHERE stage = 'fixture_boxscore';

COMMIT;

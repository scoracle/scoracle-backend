-- 203_persons.sql
--
-- WHAT: `persons` — the identity table for people who are not stats-platform players
-- (PLAN-one-rail.md 1.4, Appendix B D-2).
--
-- WHY: the Editor's names[] channel surfaces coaches, executives, agents, owners — and
-- story-relevant PLAYERS outside the stats platform (rookies pre-debut, retired legends,
-- foreign-league players; `player` added to the kind enum by Scott, 2026-08-01). Today those
-- names either fuzzy-match a real player (T9's spain -> team 394 failure class) or vanish.
-- persons gives the Investigator somewhere verified people can LAND, and mig 207 folds them
-- into entity_name_surfaces so tomorrow's resolver links the name (the living database
-- compounds).
--
-- The `players` table stays box-score-owned: the Investigator NEVER auto-inserts players rows.
-- If a person-kind player later appears in a box score, the two identities reconcile by
-- alias/external id (D-2, Phase 5.5). Kinds are a superset of narrative_persons.kind; that
-- graph-layer table is unaffected and reconciles later (Appendix B).
--
-- Deploy order: additive and INERT — nothing writes persons until Phase 5.

BEGIN;

CREATE TABLE IF NOT EXISTS public.persons (
    id             serial PRIMARY KEY,
    sport          text REFERENCES public.sports(id),
    full_name      text NOT NULL,
    kind           text NOT NULL CHECK (kind IN
                   ('player','coach','executive','owner','agent','family','official','other')),
    team_id        integer,
    search_aliases text[] NOT NULL DEFAULT '{}',
    meta           jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at     timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.persons IS
    'People the rail talks about who are NOT stats-platform players: coaches, executives, '
    'owners, agents, officials — and person-kind ''player'' for story-relevant players OUTSIDE '
    'the stats platform (D-2). Written only by the Investigator behind its code gates; a model '
    'mention is never a database write, and every accepted person cites source_documents '
    'provenance via the mig-205 acquisition tables.';
COMMENT ON COLUMN public.persons.kind IS
    'D-2 v1: player/coach/executive/owner/agent/official are auto-writable by the accept gate; '
    '''family'' exists in the enum but is NEVER auto-written; ''other'' is the census bucket. '
    '''player'' = outside the stats platform — the players table stays box-score-owned.';
COMMENT ON COLUMN public.persons.team_id IS
    'Current-affiliation hint, bare integer — teams'' PK is composite (id, sport), so this '
    'carries no FK. Authority on affiliation lives in entity_facts/entity_relationships with '
    'dated validity; this is the convenience copy.';
COMMENT ON COLUMN public.persons.sport IS
    'Nullable: a person can be sport-scoped (most are) or not yet placed. NULL-sport persons '
    'carry no entity_name_surfaces rows (resolution is sport-scoped) until a sport is known.';

INSERT INTO public.schema_migrations(version) VALUES ('203_persons')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

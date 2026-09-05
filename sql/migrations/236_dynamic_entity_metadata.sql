-- 236_dynamic_entity_metadata.sql
--
-- Scott, 2026-09-04: "Rather than making only some entities eligible for meta
-- change, we should treat all entity types as dynamic… give the model a guide and
-- empower it to update the db to better tell the evolving story."
--
-- The mechanisms all existed; what was missing was permission and a clock.
-- entity_facts is generic triples with supersession; provenance containment
-- (`provenance_holds`) already forbids any fact the source text does not literally
-- carry; the identity-adjudication tier is the precedent for canonical mutations.
-- But enrichment ran for players only, at accept time only, and nothing ever
-- re-investigated an accepted entity — the graph froze at first contact.
--
-- Two pieces:
--
--   1. `entity_fact_policy` — the guide, as data. A (entity_type, fact_type) row
--      grants the Investigator write permission at a tier:
--        * evidenced   — writable with provenance containment + supersession
--                        (a correction is a revision, never an overwrite).
--        * adjudicated — canonical identity; writable only when the stronger
--                        discriminator agrees (for team_affiliation: the career
--                        claim set must contain the team — the same agreement the
--                        accept gate already demands).
--      ABSENT = FROZEN. A team's city, founding year, sport, or name is not in the
--      table, so no model path can touch it — the guard Scott asked for, enforced
--      by a lookup rather than a regex. The Investigator remains the ONLY junction
--      that mutates the world; the storytelling voices read it and never edit it.
--
--   2. `refresh_dynamic_entities(sport, limit)` — the clock, nightly on the
--      narrative-links chain. Evidence-driven and leisurely: an entity earns a
--      refresh only when the news mentioned it this week AND its last look is >30
--      days old, capped per class per night so the investigate_entity drain eats
--      it at its own pace.
--        * persons  → their accepted candidate reopens to 'pending' and re-enters
--          investigate_entity through the CANDIDATE grain (the person fence on
--          pipeline_work stands untouched); re-accept refreshes kind/team/facts.
--        * players  → an investigate_entity/player item (the existing enrichment
--          grain, now on a clock instead of accept-time-only).
--        * teams    → an investigate_entity/team item (new grain, same release:
--          enrich_team reads the team's KNOWN wikidata QID — bootstrapped by the
--          team resolver — so there is no search/disambiguation surface at all).
--      Cooldown stamps: candidates.decided_at for persons; meta->>'investigated_at'
--      on players/teams, stamped at ATTEMPT start so a refusal also waits its turn.

BEGIN;

CREATE TABLE public.entity_fact_policy (
    entity_type text NOT NULL,
    fact_type   text NOT NULL,
    tier        text NOT NULL CHECK (tier IN ('evidenced', 'adjudicated')),
    PRIMARY KEY (entity_type, fact_type)
);

COMMENT ON TABLE public.entity_fact_policy IS
    'The Investigator''s write permissions (mig 236): which metadata the model may revise, at which tier. evidenced = provenance-contained + superseding; adjudicated = additionally requires the stronger discriminator to agree. A (entity_type, fact_type) ABSENT from this table is FROZEN to model paths — that is the guard: policy, not regex.';

INSERT INTO public.entity_fact_policy (entity_type, fact_type, tier) VALUES
    -- players — the enrichment set that already existed, now permission-checked
    ('player', 'date_of_birth',    'evidenced'),
    ('player', 'weight_kg',        'evidenced'),
    ('player', 'height_cm',        'evidenced'),
    ('player', 'photo_url',        'evidenced'),
    -- persons — the dossier that used to freeze at one role fact
    ('person', 'role',             'evidenced'),
    ('person', 'date_of_birth',    'evidenced'),
    ('person', 'photo_url',        'evidenced'),
    ('person', 'team_affiliation', 'adjudicated'),
    -- teams — venue and logo move; city/founded/sport/name are absent, hence frozen
    ('team',   'venue_name',       'evidenced'),
    ('team',   'logo_url',         'evidenced');

CREATE FUNCTION public.refresh_dynamic_entities(p_sport text, p_limit integer DEFAULT 25)
RETURNS TABLE(persons_reopened integer, players_enqueued integer, teams_enqueued integer)
LANGUAGE plpgsql
AS $$
DECLARE
    v_persons integer := 0;
    v_players integer := 0;
    v_teams   integer := 0;
BEGIN
    -- Persons: reopen the accepted candidate; the standing 5.2 enqueue conditions are
    -- met by construction (accepted candidates all carried mentions), so the reopened
    -- row re-enters the queue through the same INSERT the other classes use below.
    WITH due AS (
        SELECT c.id
        FROM public.entity_candidates c
        WHERE c.sport = p_sport
          AND c.state = 'accepted'
          AND c.resolved_entity_type = 'person'
          AND c.decided_at < NOW() - interval '30 days'
          AND EXISTS (
              SELECT 1 FROM public.news_article_entities ne
              WHERE ne.entity_type = 'person' AND ne.entity_id = c.resolved_entity_id
                AND ne.sport = p_sport AND ne.created_at > NOW() - interval '7 days')
        ORDER BY c.decided_at
        LIMIT p_limit
    ),
    reopened AS (
        UPDATE public.entity_candidates c
           SET state = 'pending', last_seen_at = NOW()
          FROM due WHERE c.id = due.id
        RETURNING c.id
    ),
    enq AS (
        INSERT INTO public.pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        SELECT 'investigate_entity', 'candidate', r.id, p_sport, 'pending', NULL, NOW(), NOW()
          FROM reopened r
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status = 'pending', attempts = 0,
            available_at = CASE WHEN public.pipeline_work.status = 'pending'
                                THEN public.pipeline_work.available_at ELSE NOW() END,
            updated_at = NOW(), last_error = NULL
        WHERE public.pipeline_work.status = 'failed'
        RETURNING entity_id
    )
    SELECT COUNT(*)::integer INTO v_persons FROM enq;

    -- Players: the existing enrichment grain, clocked.
    WITH due AS (
        SELECT p.id
        FROM public.players p
        WHERE p.sport = p_sport
          AND COALESCE((p.meta->>'investigated_at')::timestamptz, 'epoch'::timestamptz)
              < NOW() - interval '30 days'
          AND EXISTS (
              SELECT 1 FROM public.news_article_entities ne
              WHERE ne.entity_type = 'player' AND ne.entity_id = p.id
                AND ne.sport = p_sport AND ne.created_at > NOW() - interval '7 days')
        ORDER BY COALESCE((p.meta->>'investigated_at')::timestamptz, 'epoch'::timestamptz)
        LIMIT p_limit
    ),
    enq AS (
        INSERT INTO public.pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        SELECT 'investigate_entity', 'player', d.id, p_sport, 'pending', NULL, NOW(), NOW()
          FROM due d
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status = 'pending', attempts = 0,
            available_at = CASE WHEN public.pipeline_work.status = 'pending'
                                THEN public.pipeline_work.available_at ELSE NOW() END,
            updated_at = NOW(), last_error = NULL
        WHERE public.pipeline_work.status = 'failed'
        RETURNING entity_id
    )
    SELECT COUNT(*)::integer INTO v_players FROM enq;

    -- Teams: only those with a known wikidata handle — enrich_team fetches the KNOWN
    -- item; a team without one has no team-shaped source and stays as it is.
    WITH due AS (
        SELECT t.id
        FROM public.teams t
        WHERE t.sport = p_sport
          AND COALESCE((t.meta->>'investigated_at')::timestamptz, 'epoch'::timestamptz)
              < NOW() - interval '30 days'
          AND EXISTS (
              SELECT 1 FROM public.entity_external_ids x
              WHERE x.entity_type = 'team' AND x.entity_id = t.id
                AND x.namespace = 'wikidata')
          AND EXISTS (
              SELECT 1 FROM public.news_article_entities ne
              WHERE ne.entity_type = 'team' AND ne.entity_id = t.id
                AND ne.sport = p_sport AND ne.created_at > NOW() - interval '7 days')
        ORDER BY COALESCE((t.meta->>'investigated_at')::timestamptz, 'epoch'::timestamptz)
        LIMIT p_limit
    ),
    enq AS (
        INSERT INTO public.pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        SELECT 'investigate_entity', 'team', d.id, p_sport, 'pending', NULL, NOW(), NOW()
          FROM due d
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status = 'pending', attempts = 0,
            available_at = CASE WHEN public.pipeline_work.status = 'pending'
                                THEN public.pipeline_work.available_at ELSE NOW() END,
            updated_at = NOW(), last_error = NULL
        WHERE public.pipeline_work.status = 'failed'
        RETURNING entity_id
    )
    SELECT COUNT(*)::integer INTO v_teams FROM enq;

    IF v_persons + v_players + v_teams > 0 THEN
        PERFORM pg_notify('pipeline_work_ready', '');
    END IF;

    RETURN QUERY SELECT v_persons, v_players, v_teams;
END;
$$;

COMMENT ON FUNCTION public.refresh_dynamic_entities(text, integer) IS
    'The mig 236 clock: nightly, evidence-driven re-investigation of news-active entities whose last look is >30 days old — persons via candidate reopen, players/teams via their investigate_entity grains. Leisurely by construction: per-class LIMIT, FIFO queue, the drain sets the pace.';

COMMIT;

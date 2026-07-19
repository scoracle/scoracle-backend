-- 155_narrative_memory.sql
--
-- The narrative graph's long-memory layer (Plan - Narrative Graph, gaps 1-3): mig 154
-- built observation (events + current-state links); this makes it REMEMBER. Three parts:
--
--   1. Event durability — narrative_events loses its ON DELETE CASCADE to news_articles.
--      The raw rail is feedstock that may be pruned; the extracted events are the distilled
--      memory and must survive their sources (precedent: transfer_rumors.input_news_ids
--      is a soft reference already). article_id stays as a soft ref; a denormalized
--      `source` column keeps attribution alive after any pruning. The person-MENTIONS rail
--      keeps its cascade deliberately: mentions are rail-level evidence whose durable tally
--      lives on narrative_persons.evidence, same contract as news_article_entities.
--
--   2. narrative_episodes — the "team A flirted with him back in 2025" layer. An episode
--      is a bounded span of a (pair, link_type) association: opened when link strength
--      crosses a threshold, peak tracked, sealed with an outcome when the link goes quiet.
--      Links stay a rolling now-view (strength decays to 0 and the story would evaporate);
--      episodes are what remain — compact, dated, outcome-tagged memories, which is also
--      exactly the LLM-context enrichment unit for narratives/transfers/vibe/sigil.
--      roll_narrative_episodes() is link_type-generic: future typed links (trade_rumor,
--      praise, …) get episode memory for free.
--
--   3. narrative_person_affiliations — interval-valued affiliation history ("he coached
--      team B in 2019"), mirroring player_team_history's shape (valid_from/valid_until/
--      is_current/season). A trigger on narrative_persons.team_id turns every affiliation
--      change into history instead of a clobber. Entity-generic endpoints so an agent's
--      tie to a PLAYER is representable, not just person-to-team. valid_from is a plain
--      column: backdated intervals from extraction evidence ("in 2019") are insertable
--      directly; the trigger is only the automatic now-path.
--
-- Also: refresh_co_mention_links v2 adds `window_oldest` to components so episode opening
-- can date a REOPENED flirtation from current-window coverage, not the link's all-time
-- first_seen_at (episode #2 in 2026 must not claim 2025's start date).
--
-- Deploy order: ADDITIVE (narrative_events is empty; nothing reads the new tables yet) —
-- apply any time before the next cron tick.

BEGIN;

-- ── 1. Events must survive the rail ──────────────────────────────────────────────────

ALTER TABLE public.narrative_events
    DROP CONSTRAINT IF EXISTS narrative_events_article_id_fkey;

ALTER TABLE public.narrative_events
    ADD COLUMN IF NOT EXISTS source TEXT;

COMMENT ON COLUMN public.narrative_events.article_id IS
    'Soft reference to news_articles (no FK since mig 155): the rail is prunable '
    'feedstock, the event is durable memory. Join tolerantly.';
COMMENT ON COLUMN public.narrative_events.source IS
    'Denormalized news_articles.source at extraction time, so attribution survives '
    'rail pruning.';

-- ── 2. Episodes: sealed memories over the rolling link state ─────────────────────────

CREATE TABLE IF NOT EXISTS public.narrative_episodes (
    id              BIGSERIAL PRIMARY KEY,
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    link_type       TEXT NOT NULL,
    subject_type    TEXT NOT NULL,
    subject_id      INTEGER NOT NULL,
    object_type     TEXT NOT NULL,
    object_id       INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'open',
    outcome         TEXT,
    season          INTEGER,
    started_at      TIMESTAMPTZ NOT NULL,
    peaked_at       TIMESTAMPTZ NOT NULL,
    ended_at        TIMESTAMPTZ,
    peak_strength   SMALLINT NOT NULL,
    last_strength   SMALLINT,
    article_count   INTEGER NOT NULL DEFAULT 0,
    distinct_sources INTEGER NOT NULL DEFAULT 0,
    event_count     INTEGER NOT NULL DEFAULT 0,
    peak_components JSONB NOT NULL DEFAULT '{}'::jsonb,
    evidence        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT narrative_episodes_link_type_check CHECK (
        link_type IN ('co_mention', 'trade_rumor', 'trade_confirmed', 'injury',
                      'contract_dispute', 'praise', 'criticism')
    ),
    CONSTRAINT narrative_episodes_subject_type_check CHECK (
        subject_type IN ('player', 'team', 'person')
    ),
    CONSTRAINT narrative_episodes_object_type_check CHECK (
        object_type IN ('player', 'team', 'person')
    ),
    CONSTRAINT narrative_episodes_no_self_loop_check CHECK (
        NOT (subject_type = object_type AND subject_id = object_id)
    ),
    CONSTRAINT narrative_episodes_status_check CHECK (
        status IN ('open', 'sealed')
    ),
    CONSTRAINT narrative_episodes_outcome_check CHECK (
        outcome IS NULL OR outcome IN ('confirmed', 'fizzled')
    ),
    -- Open episodes have no outcome and no end; sealed episodes have both.
    CONSTRAINT narrative_episodes_open_outcome_check CHECK (
        (status = 'open') = (outcome IS NULL)
    ),
    CONSTRAINT narrative_episodes_open_ended_check CHECK (
        (status = 'open') = (ended_at IS NULL)
    ),
    CONSTRAINT narrative_episodes_peak_strength_check CHECK (
        peak_strength >= 0 AND peak_strength <= 100
    ),
    CONSTRAINT narrative_episodes_last_strength_check CHECK (
        last_strength IS NULL OR (last_strength >= 0 AND last_strength <= 100)
    )
);

-- The lifecycle invariant: at most ONE open episode per edge — the roll function's
-- ON CONFLICT target. Sealed history for the same edge accumulates freely (that IS
-- the memory: flirtation 2025, flirtation again 2026 = two rows).
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_episodes_one_open
    ON public.narrative_episodes (sport, link_type, subject_type, subject_id,
                                  object_type, object_id)
    WHERE status = 'open';
CREATE INDEX IF NOT EXISTS idx_narrative_episodes_subject
    ON public.narrative_episodes (sport, subject_type, subject_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_narrative_episodes_object
    ON public.narrative_episodes (sport, object_type, object_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_narrative_episodes_type_status
    ON public.narrative_episodes (sport, link_type, status);
CREATE INDEX IF NOT EXISTS idx_narrative_episodes_season
    ON public.narrative_episodes (sport, season);

COMMENT ON TABLE public.narrative_episodes IS
    'Bounded, outcome-tagged spans of narrative association — the graph''s long-term '
    'memory ("this is a place team A flirted with back in 2025"). Opened/peaked/sealed '
    'by roll_narrative_episodes() over narrative_links state; links forget by design, '
    'episodes are what remain. One open episode per edge (partial unique index); sealed '
    'rows accumulate per edge across seasons. outcome=confirmed is reserved for the '
    'event-driven path (e.g. a trade_confirmed narrative_event sealing a trade_rumor '
    'episode) — the mechanical quiet-seal only ever writes fizzled.';
COMMENT ON COLUMN public.narrative_episodes.season IS
    'sports.current_season at open time — the era anchor for "back in 2025" retrieval. '
    'A span crossing seasons keeps its opening season.';

-- ── 3. Person affiliation history ────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS public.narrative_person_affiliations (
    id              SERIAL PRIMARY KEY,
    person_id       INTEGER NOT NULL
                    REFERENCES public.narrative_persons(id) ON DELETE CASCADE,
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    entity_type     TEXT NOT NULL,
    entity_id       INTEGER NOT NULL,
    role            TEXT NOT NULL,
    season          INTEGER,
    valid_from      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_until     TIMESTAMPTZ,
    is_current      BOOLEAN NOT NULL DEFAULT TRUE,
    evidence        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT narrative_person_affiliations_entity_type_check CHECK (
        entity_type IN ('player', 'team')
    ),
    CONSTRAINT narrative_person_affiliations_role_check CHECK (
        role IN ('coach', 'agent', 'executive', 'family', 'other')
    ),
    CONSTRAINT narrative_person_affiliations_current_check CHECK (
        is_current = (valid_until IS NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_person_affiliations_one_current
    ON public.narrative_person_affiliations (person_id, entity_type, entity_id, role)
    WHERE is_current;
CREATE INDEX IF NOT EXISTS idx_narrative_person_affiliations_entity
    ON public.narrative_person_affiliations (sport, entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_narrative_person_affiliations_person
    ON public.narrative_person_affiliations (person_id, valid_from DESC);

COMMENT ON TABLE public.narrative_person_affiliations IS
    'Interval-valued affiliation history for news-derived people, mirroring '
    'player_team_history ("he coached team B in 2019"). Entity-generic: a coach ties to '
    'a team, an agent ties to a player. The narrative_persons.team_id trigger maintains '
    'the automatic now-path; backdated intervals from extraction evidence insert directly '
    'with explicit valid_from/valid_until.';

-- Every change to a person's current team becomes history instead of a clobber.
CREATE OR REPLACE FUNCTION public.narrative_persons_track_affiliation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.team_id IS NOT DISTINCT FROM OLD.team_id THEN
        RETURN NEW;
    END IF;
    UPDATE narrative_person_affiliations
       SET is_current = FALSE, valid_until = now()
     WHERE person_id = NEW.id
       AND entity_type = 'team'
       AND is_current
       AND (NEW.team_id IS NULL OR entity_id <> NEW.team_id);
    IF NEW.team_id IS NOT NULL THEN
        INSERT INTO narrative_person_affiliations
            (person_id, sport, entity_type, entity_id, role, season, evidence)
        SELECT NEW.id, NEW.sport, 'team', NEW.team_id, NEW.kind, s.current_season,
               jsonb_build_object('source', 'narrative_persons.team_id')
        FROM sports s WHERE s.id = NEW.sport
        ON CONFLICT (person_id, entity_type, entity_id, role) WHERE is_current
        DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_narrative_persons_affiliation ON public.narrative_persons;
CREATE TRIGGER trg_narrative_persons_affiliation
    AFTER INSERT OR UPDATE OF team_id ON public.narrative_persons
    FOR EACH ROW EXECUTE FUNCTION public.narrative_persons_track_affiliation();

-- ── refresh v2: expose the current window's oldest article for episode dating ────────
-- Identical to mig 154 except components gains 'window_oldest' (ISO timestamp of the
-- oldest in-window article for the pair). Episode opening prefers it over the link's
-- all-time first_seen_at so a reopened association is dated from its NEW coverage.

CREATE OR REPLACE FUNCTION public.refresh_co_mention_links(
    p_sport text,
    p_window_days integer DEFAULT 90,
    OUT pairs_upserted integer,
    OUT pairs_zeroed integer
) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run_started timestamptz := clock_timestamp();
BEGIN
    -- Pair corpus mirrors compute_transfer_heat's house rules: vetted both sides, title
    -- PROXIMITY within 50 when both positions are known, source-tier weighting with the
    -- 0.3 unknown-source default. Differences, deliberate: no bucket filter (all news
    -- counts toward association, not just transfer talk) and a 21-DAY recency half-life
    -- instead of transfer heat's 72-hour decay — a standing association fades in weeks,
    -- a trade window in days.
    WITH corpus AS (
        SELECT e1.entity_type AS subject_type, e1.entity_id AS subject_id,
               e2.entity_type AS object_type,  e2.entity_id AS object_id,
               a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities e1 ON e1.article_id = a.id
             AND e1.sport = p_sport AND e1.vetted IS TRUE
        JOIN news_article_entities e2 ON e2.article_id = a.id
             AND e2.sport = p_sport AND e2.vetted IS TRUE
        WHERE a.published_at > now() - make_interval(days => p_window_days)
          AND (e1.entity_type, e1.entity_id) < (e2.entity_type, e2.entity_id)
          AND (e1.title_pos IS NULL OR e2.title_pos IS NULL
               OR abs(e1.title_pos - e2.title_pos) <= 50)
    ),
    agg AS (
        SELECT c.subject_type, c.subject_id, c.object_type, c.object_id,
               count(*) AS total,
               count(DISTINCT c.src) AS distinct_sources,
               count(*) FILTER (WHERE c.ts > now() - INTERVAL '14 days') AS recent14,
               max(c.ts) AS newest,
               min(c.ts) AS oldest,
               COALESCE(MAX(st.weight), 0.3) AS tier_weight
        FROM corpus c
        LEFT JOIN source_tiers st ON lower(st.source) = lower(c.src) AND st.kind = 'news'
        GROUP BY 1, 2, 3, 4
    ),
    calc AS (
        SELECT *,
               EXTRACT(EPOCH FROM (now() - newest)) / 86400.0 AS age_days,
               LEAST(1.0, distinct_sources::numeric / 5.0) AS volume,
               recent14::numeric / GREATEST(total, 1) AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *,
               exp(-age_days / 21.0) AS recency,
               GREATEST(0, LEAST(100, round(
                   100 * tier_weight * exp(-age_days / 21.0)
                       * (0.6 * volume + 0.4 * recent_frac))))::smallint AS strength
        FROM calc
    )
    INSERT INTO narrative_links AS l
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         strength, components, article_count, distinct_sources,
         first_seen_at, last_event_at, trajectory, trajectory_components, updated_at)
    SELECT p_sport, 'co_mention', f.subject_type, f.subject_id, f.object_type, f.object_id,
           f.strength,
           jsonb_build_object(
               'article_count', f.total,
               'distinct_sources', f.distinct_sources,
               'recent_14d', f.recent14,
               'newest_age_days', round(f.age_days::numeric, 1),
               'tier_weight', f.tier_weight,
               'volume', round(f.volume::numeric, 3),
               'recency', round(f.recency::numeric, 3),
               'recent_frac', round(f.recent_frac::numeric, 3),
               'window_days', p_window_days,
               'window_oldest', f.oldest),
           f.total, f.distinct_sources,
           f.oldest, f.newest,
           'developing_story',
           jsonb_build_object('previous', NULL, 'current', f.strength,
                              'delta', NULL, 'direction', 'new_or_unmatched'),
           v_run_started
    FROM fin f
    ON CONFLICT (sport, link_type, subject_type, subject_id, object_type, object_id)
    DO UPDATE SET
        strength = EXCLUDED.strength,
        components = EXCLUDED.components,
        article_count = EXCLUDED.article_count,
        distinct_sources = EXCLUDED.distinct_sources,
        first_seen_at = LEAST(l.first_seen_at, EXCLUDED.first_seen_at),
        last_event_at = EXCLUDED.last_event_at,
        -- The shared ±10-bucket trajectory vocabulary (rust/src/trajectory.rs
        -- classify_delta / go/internal/db/db.go): keep all three homes in step.
        trajectory = CASE
            WHEN l.strength IS NULL THEN 'developing_story'
            WHEN EXCLUDED.strength - l.strength >= 10 THEN 'heating_up'
            WHEN EXCLUDED.strength - l.strength <= -10 THEN 'cooling_off'
            ELSE 'developing_story' END,
        trajectory_components = jsonb_build_object(
            'previous', l.strength,
            'current', EXCLUDED.strength,
            'delta', CASE WHEN l.strength IS NULL THEN NULL
                          ELSE EXCLUDED.strength - l.strength END,
            'direction', CASE
                WHEN l.strength IS NULL THEN 'new_or_unmatched'
                WHEN EXCLUDED.strength - l.strength >= 10 THEN 'up'
                WHEN EXCLUDED.strength - l.strength <= -10 THEN 'down'
                ELSE 'stable' END),
        updated_at = v_run_started;

    GET DIAGNOSTICS pairs_upserted = ROW_COUNT;

    -- Pairs that fell out of the window entirely: decay to 0 rather than delete, so
    -- first_seen/trajectory history survives a quiet spell. (A periodic prune of
    -- long-dead zero rows can come later if volume ever warrants it.)
    UPDATE narrative_links l
    SET trajectory = CASE WHEN l.strength >= 10 THEN 'cooling_off'
                          ELSE 'developing_story' END,
        trajectory_components = jsonb_build_object(
            'previous', l.strength, 'current', 0,
            'delta', -l.strength,
            'direction', CASE WHEN l.strength >= 10 THEN 'down' ELSE 'stable' END),
        strength = 0,
        components = l.components || '{"decayed_out_of_window": true}'::jsonb,
        updated_at = v_run_started
    WHERE l.sport = p_sport
      AND l.link_type = 'co_mention'
      AND l.updated_at < v_run_started
      AND l.strength IS DISTINCT FROM 0;

    GET DIAGNOSTICS pairs_zeroed = ROW_COUNT;
END;
$$;

-- ── The episode lifecycle: open → peak → seal ────────────────────────────────────────

CREATE OR REPLACE FUNCTION public.roll_narrative_episodes(
    p_sport text,
    p_open_threshold integer DEFAULT 40,
    p_close_threshold integer DEFAULT 15,
    p_quiet_days integer DEFAULT 14,
    OUT episodes_opened integer,
    OUT episodes_updated integer,
    OUT episodes_sealed integer
) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    -- Open: a link crossing the notability threshold with no open episode starts one.
    -- started_at prefers the current window's oldest article (components.window_oldest,
    -- refresh v2) over the link's all-time first_seen_at, so a REOPENED association is
    -- dated from its new coverage. Slight overlap with a just-sealed episode's tail is
    -- accepted fuzz.
    INSERT INTO narrative_episodes
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         status, season, started_at, peaked_at, peak_strength, last_strength,
         article_count, distinct_sources, event_count, peak_components,
         created_at, updated_at)
    SELECT l.sport, l.link_type, l.subject_type, l.subject_id,
           l.object_type, l.object_id,
           'open', s.current_season,
           COALESCE((l.components->>'window_oldest')::timestamptz,
                    l.first_seen_at, v_run),
           v_run, l.strength, l.strength,
           l.article_count, l.distinct_sources, l.event_count, l.components,
           v_run, v_run
    FROM narrative_links l
    JOIN sports s ON s.id = l.sport
    WHERE l.sport = p_sport
      AND l.strength >= p_open_threshold
    ON CONFLICT (sport, link_type, subject_type, subject_id, object_type, object_id)
        WHERE status = 'open'
    DO NOTHING;

    GET DIAGNOSTICS episodes_opened = ROW_COUNT;

    -- Freshen every open episode from its link, whatever the current strength: track
    -- the peak, keep counters at their episode-high (the link's window slides, so
    -- GREATEST approximates "over the episode"), remember the latest strength.
    UPDATE narrative_episodes e
    SET last_strength = l.strength,
        article_count = GREATEST(e.article_count, l.article_count),
        distinct_sources = GREATEST(e.distinct_sources, l.distinct_sources),
        event_count = GREATEST(e.event_count, l.event_count),
        peak_strength = GREATEST(e.peak_strength, l.strength),
        peaked_at = CASE WHEN l.strength > e.peak_strength THEN v_run
                         ELSE e.peaked_at END,
        peak_components = CASE WHEN l.strength > e.peak_strength THEN l.components
                               ELSE e.peak_components END,
        updated_at = v_run
    FROM narrative_links l
    WHERE e.sport = p_sport AND e.status = 'open'
      AND l.sport = e.sport AND l.link_type = e.link_type
      AND l.subject_type = e.subject_type AND l.subject_id = e.subject_id
      AND l.object_type = e.object_type AND l.object_id = e.object_id;

    GET DIAGNOSTICS episodes_updated = ROW_COUNT;

    -- Seal: weak AND quiet → the story is over; write the memory. The mechanical path
    -- only ever concludes 'fizzled' — 'confirmed' belongs to the event-driven path
    -- (a trade_confirmed narrative_event sealing its trade_rumor episode).
    UPDATE narrative_episodes e
    SET status = 'sealed',
        outcome = 'fizzled',
        ended_at = COALESCE(l.last_event_at, e.updated_at),
        last_strength = l.strength,
        evidence = e.evidence || jsonb_build_object('sealed', jsonb_build_object(
            'at', v_run,
            'reason', 'quiet',
            'final_strength', l.strength,
            'close_threshold', p_close_threshold,
            'quiet_days', p_quiet_days)),
        updated_at = v_run
    FROM narrative_links l
    WHERE e.sport = p_sport AND e.status = 'open'
      AND l.sport = e.sport AND l.link_type = e.link_type
      AND l.subject_type = e.subject_type AND l.subject_id = e.subject_id
      AND l.object_type = e.object_type AND l.object_id = e.object_id
      AND l.strength < p_close_threshold
      AND (l.last_event_at IS NULL
           OR l.last_event_at < now() - make_interval(days => p_quiet_days));

    GET DIAGNOSTICS episodes_sealed = ROW_COUNT;
END;
$$;

COMMENT ON FUNCTION public.roll_narrative_episodes(text, integer, integer, integer) IS
    'Episode lifecycle over narrative_links state: open at strength >= open_threshold, '
    'track peak while open, seal as fizzled when strength < close_threshold AND quiet '
    'for quiet_days. Link_type-generic. Run after refresh_co_mention_links each night '
    '(cron-narrative-links.sh); the thresholds are the notability dial.';

INSERT INTO public.schema_migrations(version) VALUES ('155_narrative_memory')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

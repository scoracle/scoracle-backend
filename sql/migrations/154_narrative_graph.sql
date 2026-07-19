-- 154_narrative_graph.sql
--
-- The Narrative Graph layer (Plan - Narrative Graph, 2026-07-19): entity-to-entity
-- relationship modeling over the existing news rail. Postgres, not Neo4j — every planned
-- query is 1-2 hops over indexed edges at ~100k-article scale, the canonical entities
-- (players/teams, composite (id, sport) keys) already live here, and a second store would
-- need continuous entity sync plus its own backup/ops surface. The schema is a property
-- graph in relational clothes: nodes = players/teams/narrative_persons, edges =
-- narrative_links (aggregated state) + narrative_events (append-only atomic history).
-- If deep variable-length traversal ever becomes real, this projects mechanically into
-- Apache AGE or Neo4j; the edge tables are the source of truth either way.
--
-- Four tables + one function:
--   narrative_persons         — news-DERIVED people (head coaches, agents, execs) the API
--                               provider never seeds; candidate → active lifecycle so a
--                               recurring name earns entityhood on evidence, never on one
--                               extraction. A bad merge is worse than a missed mention.
--   narrative_person_mentions — article ↔ person rail, parallel to news_article_entities
--                               (which keeps its player/team CHECK; the existing scrub/derive
--                               trigger machinery is not disturbed).
--   narrative_events          — atomic extracted relations (predicate, sentiment, confidence)
--                               with article provenance + model/prompt versions. Append-only
--                               like every cognition product; idempotent per article+relation.
--   narrative_links           — the served graph: one row per (subject, object, link_type)
--                               with strength 0-100, components breakdown, and the house
--                               trajectory vocabulary (developing_story/heating_up/cooling_off,
--                               ±10 buckets — vocabulary homes: rust/src/trajectory.rs,
--                               go/internal/db/db.go, and now refresh_co_mention_links here).
--
-- refresh_co_mention_links() populates co_mention edges from ALREADY-VETTED
-- news_article_entities — no model calls, pure derivation from the existing rail. The LLM
-- extraction stage (typed predicates, person discovery) lands separately and writes
-- narrative_events / narrative_persons; this migration is schema + the mechanical edge.
--
-- Deploy order: ADDITIVE — nothing reads or writes these tables yet; apply any time before
-- the first binary that does.

BEGIN;

-- ── Node registry for news-derived people ────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS public.narrative_persons (
    id              SERIAL PRIMARY KEY,
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    kind            TEXT NOT NULL,
    name            TEXT NOT NULL,
    aliases         TEXT[] NOT NULL DEFAULT '{}',
    team_id         INTEGER,
    status          TEXT NOT NULL DEFAULT 'candidate',
    merged_into     INTEGER REFERENCES public.narrative_persons(id),
    evidence        JSONB NOT NULL DEFAULT '{}'::jsonb,
    mention_count   INTEGER NOT NULL DEFAULT 0,
    distinct_sources INTEGER NOT NULL DEFAULT 0,
    first_seen_at   TIMESTAMPTZ,
    last_seen_at    TIMESTAMPTZ,
    model_version   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT narrative_persons_kind_check CHECK (
        kind IN ('coach', 'agent', 'executive', 'family', 'other')
    ),
    CONSTRAINT narrative_persons_status_check CHECK (
        status IN ('candidate', 'active', 'merged', 'retired')
    ),
    CONSTRAINT narrative_persons_merged_check CHECK (
        (status = 'merged') = (merged_into IS NOT NULL)
    ),
    CONSTRAINT narrative_persons_team_fkey
        FOREIGN KEY (team_id, sport) REFERENCES public.teams(id, sport)
);

CREATE INDEX IF NOT EXISTS idx_narrative_persons_sport_kind_status
    ON public.narrative_persons (sport, kind, status);
CREATE INDEX IF NOT EXISTS idx_narrative_persons_aliases
    ON public.narrative_persons USING gin (aliases);
CREATE INDEX IF NOT EXISTS idx_narrative_persons_lower_name
    ON public.narrative_persons (sport, lower(name));
CREATE INDEX IF NOT EXISTS idx_narrative_persons_team
    ON public.narrative_persons (team_id, sport) WHERE team_id IS NOT NULL;

COMMENT ON TABLE public.narrative_persons IS
    'News-derived people (coaches, agents, executives) absent from provider seeding. '
    'Extraction creates candidates; promotion to active requires recurring multi-source '
    'evidence (thresholds live in the promotion step, not here). Same-name collisions are '
    'legal (two Mike Browns) — disambiguation is the resolve gate''s job, merges via '
    'status=merged + merged_into.';
COMMENT ON COLUMN public.narrative_persons.evidence IS
    'Accumulated discovery evidence: mention/source tallies, team-affiliation votes, sample '
    'article ids — the promotion decision''s input, in one inspectable place.';
COMMENT ON COLUMN public.narrative_persons.team_id IS
    'Current primary affiliation derived from news evidence (nullable — agents float free).';

-- ── Article ↔ person rail ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS public.narrative_person_mentions (
    article_id      BIGINT NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    person_id       INTEGER NOT NULL REFERENCES public.narrative_persons(id) ON DELETE CASCADE,
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    match_confidence NUMERIC(4,3),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (article_id, person_id)
);

CREATE INDEX IF NOT EXISTS idx_narrative_person_mentions_person
    ON public.narrative_person_mentions (person_id, created_at DESC);

COMMENT ON TABLE public.narrative_person_mentions IS
    'Article-to-person links, the person analogue of news_article_entities. Kept separate so '
    'the existing player/team scrub + derive-trigger machinery is untouched; a later migration '
    'may unify the rails once persons need those stages.';

-- ── Atomic relation events (append-only) ─────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS public.narrative_events (
    id              BIGSERIAL PRIMARY KEY,
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    subject_type    TEXT NOT NULL,
    subject_id      INTEGER NOT NULL,
    predicate       TEXT NOT NULL,
    object_type     TEXT,
    object_id       INTEGER,
    sentiment       NUMERIC(3,2),
    confidence      TEXT NOT NULL,
    article_id      BIGINT NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    event_date      TIMESTAMPTZ NOT NULL,
    details         JSONB NOT NULL DEFAULT '{}'::jsonb,
    model_version   TEXT NOT NULL,
    prompt_version  TEXT NOT NULL,
    extracted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT narrative_events_subject_type_check CHECK (
        subject_type IN ('player', 'team', 'person')
    ),
    CONSTRAINT narrative_events_object_type_check CHECK (
        object_type IS NULL OR object_type IN ('player', 'team', 'person')
    ),
    CONSTRAINT narrative_events_object_pair_check CHECK (
        (object_type IS NULL) = (object_id IS NULL)
    ),
    CONSTRAINT narrative_events_predicate_check CHECK (
        predicate IN ('trade_rumor', 'trade_confirmed', 'injury',
                      'contract_dispute', 'praise', 'criticism')
    ),
    CONSTRAINT narrative_events_sentiment_check CHECK (
        sentiment IS NULL OR (sentiment >= -1.0 AND sentiment <= 1.0)
    ),
    CONSTRAINT narrative_events_confidence_check CHECK (
        confidence IN ('speculative', 'reported', 'confirmed')
    )
);

-- Re-extraction with a newer model/prompt upserts on this key rather than duplicating.
CREATE UNIQUE INDEX IF NOT EXISTS idx_narrative_events_dedupe
    ON public.narrative_events (article_id, sport, subject_type, subject_id, predicate,
                                COALESCE(object_type, ''), COALESCE(object_id, 0));
CREATE INDEX IF NOT EXISTS idx_narrative_events_subject_date
    ON public.narrative_events (sport, subject_type, subject_id, event_date DESC);
CREATE INDEX IF NOT EXISTS idx_narrative_events_object_date
    ON public.narrative_events (sport, object_type, object_id, event_date DESC)
    WHERE object_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_narrative_events_article
    ON public.narrative_events (article_id);

COMMENT ON TABLE public.narrative_events IS
    'Append-only atomic narrative relations extracted from articles: the trajectory series '
    'is a time-ordered scan of this table per endpoint (or endpoint pair) — the graph''s '
    'event history. Deliberately small predicate vocabulary: precision over coverage; grow '
    'it by migration, with eval evidence, never by loosening the prompt alone.';
COMMENT ON COLUMN public.narrative_events.event_date IS
    'The article''s published_at, denormalized so trajectory scans never join the rail.';

-- ── Aggregated edges (the served graph) ──────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS public.narrative_links (
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    link_type       TEXT NOT NULL,
    subject_type    TEXT NOT NULL,
    subject_id      INTEGER NOT NULL,
    object_type     TEXT NOT NULL,
    object_id       INTEGER NOT NULL,
    strength        SMALLINT,
    components      JSONB NOT NULL DEFAULT '{}'::jsonb,
    weighted_sentiment NUMERIC(4,3),
    confidence      TEXT,
    event_count     INTEGER NOT NULL DEFAULT 0,
    article_count   INTEGER NOT NULL DEFAULT 0,
    distinct_sources INTEGER NOT NULL DEFAULT 0,
    first_seen_at   TIMESTAMPTZ,
    last_event_at   TIMESTAMPTZ,
    trajectory      TEXT NOT NULL DEFAULT 'developing_story',
    trajectory_components JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (sport, link_type, subject_type, subject_id, object_type, object_id),
    CONSTRAINT narrative_links_link_type_check CHECK (
        link_type IN ('co_mention', 'trade_rumor', 'trade_confirmed', 'injury',
                      'contract_dispute', 'praise', 'criticism')
    ),
    CONSTRAINT narrative_links_subject_type_check CHECK (
        subject_type IN ('player', 'team', 'person')
    ),
    CONSTRAINT narrative_links_object_type_check CHECK (
        object_type IN ('player', 'team', 'person')
    ),
    CONSTRAINT narrative_links_no_self_loop_check CHECK (
        NOT (subject_type = object_type AND subject_id = object_id)
    ),
    -- Symmetric edges store one canonical row: endpoints ordered as a (type, id) tuple.
    CONSTRAINT narrative_links_co_mention_canonical_check CHECK (
        link_type <> 'co_mention'
        OR subject_type < object_type
        OR (subject_type = object_type AND subject_id < object_id)
    ),
    CONSTRAINT narrative_links_strength_check CHECK (
        strength IS NULL OR (strength >= 0 AND strength <= 100)
    ),
    CONSTRAINT narrative_links_sentiment_check CHECK (
        weighted_sentiment IS NULL OR (weighted_sentiment >= -1.0 AND weighted_sentiment <= 1.0)
    ),
    CONSTRAINT narrative_links_confidence_check CHECK (
        confidence IS NULL OR confidence IN ('speculative', 'reported', 'confirmed')
    ),
    CONSTRAINT narrative_links_trajectory_check CHECK (
        trajectory IN ('developing_story', 'heating_up', 'cooling_off')
    )
);

CREATE INDEX IF NOT EXISTS idx_narrative_links_object
    ON public.narrative_links (sport, object_type, object_id);
CREATE INDEX IF NOT EXISTS idx_narrative_links_strength
    ON public.narrative_links (sport, strength DESC)
    WHERE strength IS NOT NULL AND strength > 0;
CREATE INDEX IF NOT EXISTS idx_narrative_links_last_event
    ON public.narrative_links (sport, last_event_at DESC)
    WHERE last_event_at IS NOT NULL;

COMMENT ON TABLE public.narrative_links IS
    'The narrative graph''s edge state: one row per (subject, object, link_type) with '
    'strength 0-100 + components breakdown (compute_transfer_heat house pattern) and the '
    'shared ±10-bucket trajectory vocabulary. co_mention rows are mechanical derivations '
    'from the vetted news rail (refresh_co_mention_links); typed rows aggregate '
    'narrative_events. Directed as stored; co_mention is symmetric and canonically ordered. '
    'A player''s own-team edge is legitimately strong — display layers filter current-roster '
    'pairs via player_current_identity, the graph does not.';

-- ── Mechanical population from the existing news rail ────────────────────────────────

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
               'window_days', p_window_days),
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

COMMENT ON FUNCTION public.refresh_co_mention_links(text, integer) IS
    'Recomputes all co_mention narrative_links for a sport from the vetted news rail — '
    'idempotent, set-based, no model calls. v1 reads news_article_entities only; extend '
    'with a narrative_person_mentions union once the extraction stage populates persons.';

INSERT INTO public.schema_migrations(version) VALUES ('154_narrative_graph')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

-- 235_coach_transfer_eligibility.sql
--
-- Scott, 2026-09-04: "Coaches should be treated like transfer eligible entities
-- too." A managerial appointment is a transfer story with a different noun — the
-- wire treats it that way, the Insider should too.
--
-- The pair machinery was one column away. transfer_rumors.player_id never had an
-- FK (the pair is loose triples, mig-205 style) — but the persons and players id
-- sequences OVERLAP, so a bare id cannot say which table it names. Everything
-- pair-keyed (debounce, trajectory baseline, the "pair anchor stable across daily
-- re-verdicts") would silently cross-wire a coach onto a same-id player. Hence:
--
--   1. `subject_type` ('player' | 'person'), default 'player' — every existing row
--      is correct unchanged, and every pair-keyed read adds it to the key.
--   2. `compute_transfer_heat` gains a subject-type arg so the pair corpus joins
--      news_article_entities on the right entity_type. The 3-arg form remains as
--      a delegating wrapper — nothing that calls it today changes behavior.
--
-- The Rust side (same release) admits persons of kind 'coach' into the Insider's
-- candidate sweep — coaches only, deliberately: executives and agents MOVE the
-- market, they are not moved BY it; their place is the cast line and the person
-- index, not the rumor mill. Widening later is one WHERE clause.

BEGIN;

ALTER TABLE public.transfer_rumors
    ADD COLUMN IF NOT EXISTS subject_type text NOT NULL DEFAULT 'player';

ALTER TABLE public.transfer_rumors
    ADD CONSTRAINT transfer_rumors_subject_type_check
    CHECK (subject_type IN ('player', 'person'));

COMMENT ON COLUMN public.transfer_rumors.subject_type IS
    'Which table player_id names (mig 235): ''player'' → public.players, ''person'' → public.persons (kind coach). Part of the pair key everywhere — person and player id sequences overlap, so (team_id, player_id, sport) alone is ambiguous across the two.';

CREATE FUNCTION public.compute_transfer_heat(
    p_team_id integer, p_subject_id integer, p_sport text, p_subject_type text,
    OUT heat smallint, OUT components jsonb, OUT news_ids bigint[]
) RETURNS record
    LANGUAGE sql STABLE
    AS $$
    WITH corpus AS (
        SELECT 'news'::text AS kind, a.id::text AS item_id, a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type = p_subject_type
             AND pe.entity_id = p_subject_id AND pe.sport = p_sport
        WHERE a.bucket IS DISTINCT FROM 'non_transfer'
          AND a.published_at > NOW() - INTERVAL '14 days'
    ),
    agg AS (
        SELECT
            count(DISTINCT c.src) AS distinct_sources,
            count(*) FILTER (WHERE c.ts > NOW() - INTERVAL '3 days') AS recent3,
            count(*) AS total,
            max(c.ts) AS newest
        FROM corpus c
    ),
    calc AS (
        SELECT *,
            EXTRACT(EPOCH FROM (NOW() - newest)) / 3600.0 AS age_hours,
            LEAST(1.0, distinct_sources::numeric / 5.0)   AS volume,
            recent3::numeric / GREATEST(total, 1)         AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *, exp(-age_hours / 72.0) AS recency FROM calc
    )
    SELECT
        CASE WHEN total = 0 THEN NULL
             ELSE GREATEST(0, LEAST(100,
                    round(100 * recency * (0.6 * volume + 0.4 * recent_frac))))::smallint
        END,
        CASE WHEN total = 0 THEN '{}'::jsonb
             ELSE jsonb_build_object(
                'distinct_sources', distinct_sources,
                'recent_3d', recent3,
                'total_14d', total,
                'newest_age_hours', round(age_hours::numeric, 1),
                'volume', round(volume::numeric, 3),
                'recency', round(recency::numeric, 3),
                'recent_frac', round(recent_frac::numeric, 3))
        END,
        COALESCE((SELECT array_agg(item_id::bigint) FROM corpus WHERE kind='news'), '{}')
    FROM fin;
$$;

-- The 3-arg form becomes a delegating wrapper: identical signature, identical
-- behavior, zero churn for existing callers.
CREATE OR REPLACE FUNCTION public.compute_transfer_heat(
    p_team_id integer, p_player_id integer, p_sport text,
    OUT heat smallint, OUT components jsonb, OUT news_ids bigint[]
) RETURNS record
    LANGUAGE sql STABLE
    AS $$
    SELECT * FROM public.compute_transfer_heat(p_team_id, p_player_id, p_sport, 'player');
$$;

COMMIT;

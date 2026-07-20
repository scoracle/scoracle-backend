-- 162_narrative_context_for_pair.sql
--
-- The junction feed (Scotty, 2026-07-19): "the relational database is not exposed to
-- the user, it's used to give richer context to the model which generates the
-- user-facing result. That result is also banked, so the corpus continues to self
-- enrich." narrative_context_for_pair() renders the graph's memory for one
-- (player, team) pair as a compact plain-text card — prior sealed stories with
-- outcomes, the current open story with its likelihood and trajectory, and any recent
-- confirmed move — for injection into cognition-stage prompts. The transfer stage
-- consumes it first (TRANSFER_PROMPT_VERSION t8); narratives/vibe/sigil follow the
-- same pattern with their own context shapes.
--
-- Design notes:
--   * TEXT, not JSON: the card is prompt material for a 7-8B model — prose lines beat
--     nested JSON for adherence, and the house style (evidence card, t7) is already
--     rendered lines of computed fact.
--   * Returns NULL when the graph holds no memory for the pair — the prompt builder
--     skips the section entirely (no empty scaffolding for the model to hallucinate
--     against).
--   * Deliberately NOT part of the transfer input_hash: memory enrichment rides along
--     when the corpus changes rather than triggering regenerations of its own. If a
--     sealed outcome should someday force a re-read, add it to the hash then — as an
--     explicit, costed decision.
--
-- Deploy order: ADDITIVE — apply BEFORE deploying the t8 cognition binary (its
-- build_pair_request calls this function; the running binary never does).

BEGIN;

CREATE OR REPLACE FUNCTION public.narrative_context_for_pair(
    p_sport text,
    p_player_id integer,
    p_team_id integer
) RETURNS text
    LANGUAGE sql STABLE
    AS $$
WITH sealed AS (
    SELECT format(
               'Prior %s: %s, peak coverage %s/100.',
               CASE WHEN e.outcome = 'confirmed' THEN 'story ended in a CONFIRMED move'
                    ELSE 'flirtation fizzled' END,
               CASE WHEN to_char(e.started_at, 'Mon YYYY') = to_char(e.ended_at, 'Mon YYYY')
                    THEN to_char(e.started_at, 'Mon YYYY')
                    ELSE to_char(e.started_at, 'Mon YYYY') || ' - ' || to_char(e.ended_at, 'Mon YYYY')
               END,
               e.peak_strength) AS line,
           e.ended_at
    FROM narrative_episodes e
    WHERE e.sport = p_sport AND e.link_type = 'co_mention' AND e.status = 'sealed'
      AND e.subject_type = 'player' AND e.subject_id = p_player_id
      AND e.object_type = 'team' AND e.object_id = p_team_id
    ORDER BY e.ended_at DESC
    LIMIT 3
),
open_ep AS (
    SELECT format(
               'Current story: tracked since %s, peak coverage %s/100%s%s.',
               to_char(e.started_at, 'Mon DD'),
               e.peak_strength,
               COALESCE(', computed likelihood ' || e.likelihood || '/100', ''),
               COALESCE(' (' || replace(l.trajectory, '_', ' ') || ')', '')) AS line
    FROM narrative_episodes e
    LEFT JOIN narrative_links l
      ON l.sport = e.sport AND l.link_type = 'co_mention'
     AND l.subject_type = e.subject_type AND l.subject_id = e.subject_id
     AND l.object_type = e.object_type AND l.object_id = e.object_id
    WHERE e.sport = p_sport AND e.link_type = 'co_mention' AND e.status = 'open'
      AND e.subject_type = 'player' AND e.subject_id = p_player_id
      AND e.object_type = 'team' AND e.object_id = p_team_id
    LIMIT 1
),
recent_move AS (
    SELECT format(
               'Ground truth: the player completed a confirmed move to %s on %s.',
               t.name, to_char(g.applied_at, 'Mon DD YYYY')) AS line
    FROM transfer_ground_truth g
    JOIN teams t ON t.id = g.team_id AND t.sport = g.sport
    WHERE g.sport = p_sport AND g.player_id = p_player_id
      AND g.applied_at > now() - interval '120 days'
    ORDER BY g.applied_at DESC
    LIMIT 1
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM sealed),
    (SELECT line FROM open_ep),
    (SELECT line FROM recent_move)), '');
$$;

COMMENT ON FUNCTION public.narrative_context_for_pair(text, integer, integer) IS
    'The graph''s memory for one (player, team) pair as compact prompt lines: prior '
    'sealed stories with outcomes, the current open story + likelihood/trajectory, '
    'recent confirmed moves. NULL = no memory. Consumed by cognition-stage prompt '
    'builders (transfer t8 first) — the relational layer is model-facing, never '
    'user-facing.';

INSERT INTO public.schema_migrations(version) VALUES ('162_narrative_context_for_pair')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

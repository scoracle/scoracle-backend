-- 168_outputs_as_memories.sql
--
-- Junction memory rollout step 9: outputs-as-memories banking, card-level. The
-- junctions' own banked verdicts re-enter their prompts as provenance-labeled
-- "Our prior read:" lines — the third provenance class goes live everywhere
-- (mig 164's stats card used it first). THE ECHO-CHAMBER RULE rides in the label:
-- a junction reading its own past conclusion must weigh it as continuity, never as
-- corroborating evidence — the labeling IS the defense.
--
--   * narrative_context_for_pair v2 (transfer t8 card): the pair's FIRST staged call
--     (the early-warning receipt — Sky's Jun-11 advanced_talks on Rogers is the
--     canonical example) and the LATEST read when distinct. The transfer junction now
--     sees its own paper trail instead of re-deriving from the current corpus alone —
--     the 265-generation amnesia, closed at the source.
--   * narrative_context_for_entity v3 (n8/v12/s15/momentum-s5): players gain the
--     latest transfer-lens staged read (30d) naming the destination.
--
-- DELIBERATELY NOT DONE HERE: banking junction verdicts as narrative_events rows.
-- That changes corpus semantics (junction-authored events would flow into typed links
-- and the likelihood language input — a self-reinforcement loop that needs an explicit
-- provenance-partition design). Operator decision pending; the card-level banking
-- above closes the serving half of the loop without touching corpus ground truth.
--
-- ADDITIVE — cards serve on the next junction render; no binary change.

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
),
first_read AS (
    -- Our own banked verdicts (outputs-as-memories, mig 168). Continuity, NEVER
    -- corroboration — the label is the echo-chamber defense.
    SELECT r.id,
           format('Our prior read: first staged %s on %s (confidence %s).',
                  r.stage, to_char(r.generated_at, 'Mon DD'), r.confidence) AS line
    FROM transfer_rumors r
    WHERE r.sport = p_sport AND r.player_id = p_player_id AND r.team_id = p_team_id
      AND r.stage IS NOT NULL
    ORDER BY r.generated_at ASC
    LIMIT 1
),
last_read AS (
    SELECT r.id,
           format('Our prior read: latest read %s on %s (confidence %s).',
                  r.stage, to_char(r.generated_at, 'Mon DD'), r.confidence) AS line
    FROM transfer_rumors r
    WHERE r.sport = p_sport AND r.player_id = p_player_id AND r.team_id = p_team_id
      AND r.stage IS NOT NULL
    ORDER BY r.generated_at DESC
    LIMIT 1
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM sealed),
    (SELECT line FROM open_ep),
    (SELECT line FROM recent_move),
    (SELECT line FROM first_read),
    (SELECT l.line FROM last_read l
      WHERE l.id <> (SELECT id FROM first_read))), '');
$$;

COMMENT ON FUNCTION public.narrative_context_for_pair(text, integer, integer) IS
    'The graph''s memory for one (player, team) pair as compact prompt lines: prior '
    'sealed stories with outcomes, the current open story + likelihood/trajectory, '
    'recent confirmed moves, and (mig 168) the junction''s own first + latest staged '
    'reads as "Our prior read:" continuity lines. NULL = no memory. Consumed by '
    'cognition-stage prompt builders (transfer t8) — model-facing, never user-facing.';

CREATE OR REPLACE FUNCTION public.narrative_context_for_entity(
    p_sport text,
    p_entity_type text,
    p_entity_id integer
) RETURNS text
    LANGUAGE sql STABLE
    AS $$
WITH eps AS (
    SELECT e.*,
           CASE WHEN e.subject_type = p_entity_type AND e.subject_id = p_entity_id
                THEN e.object_type ELSE e.subject_type END AS other_type,
           CASE WHEN e.subject_type = p_entity_type AND e.subject_id = p_entity_id
                THEN e.object_id ELSE e.subject_id END AS other_id
    FROM narrative_episodes e
    WHERE e.sport = p_sport AND e.link_type = 'co_mention'
      AND ((e.subject_type = p_entity_type AND e.subject_id = p_entity_id)
        OR (e.object_type = p_entity_type AND e.object_id = p_entity_id))
),
named AS (
    SELECT eps.*, COALESCE(pl.name, tm.name, 'unknown') AS other_name
    FROM eps
    LEFT JOIN players pl ON eps.other_type = 'player' AND pl.id = eps.other_id
         AND pl.sport = p_sport
    LEFT JOIN teams tm ON eps.other_type = 'team' AND tm.id = eps.other_id
         AND tm.sport = p_sport
),
sealed AS (
    SELECT format('Prior story: %s — %s (%s, peak coverage %s/100).',
               other_name,
               CASE WHEN outcome = 'confirmed' THEN 'ended in a CONFIRMED move'
                    ELSE 'fizzled' END,
               CASE WHEN to_char(started_at, 'Mon YYYY') = to_char(ended_at, 'Mon YYYY')
                    THEN to_char(started_at, 'Mon YYYY')
                    ELSE to_char(started_at, 'Mon YYYY') || ' - ' || to_char(ended_at, 'Mon YYYY')
               END,
               peak_strength) AS line,
           ended_at
    FROM named
    WHERE status = 'sealed'
    ORDER BY ended_at DESC
    LIMIT 4
),
open_eps AS (
    SELECT format('Current story: %s — tracked since %s, peak coverage %s/100%s.',
               n.other_name, to_char(n.started_at, 'Mon DD'), n.peak_strength,
               COALESCE(', computed likelihood ' || n.likelihood || '/100', '')) AS line,
           COALESCE(n.likelihood, n.peak_strength) AS rank
    FROM named n
    WHERE n.status = 'open'
      AND NOT (p_entity_type = 'player' AND n.other_type = 'team' AND EXISTS (
          SELECT 1 FROM player_current_identity pci
          WHERE pci.sport = p_sport AND pci.player_id = p_entity_id
            AND pci.team_id = n.other_id))
    ORDER BY rank DESC
    LIMIT 3
),
moves AS (
    SELECT format('Ground truth: %s completed a confirmed move to %s on %s.',
               pl.name, tm.name, to_char(g.applied_at, 'Mon DD YYYY')) AS line,
           g.applied_at
    FROM transfer_ground_truth g
    JOIN players pl ON pl.id = g.player_id AND pl.sport = g.sport
    JOIN teams tm ON tm.id = g.team_id AND tm.sport = g.sport
    WHERE g.sport = p_sport
      AND g.applied_at > now() - interval '120 days'
      AND ((p_entity_type = 'player' AND g.player_id = p_entity_id)
        OR (p_entity_type = 'team' AND g.team_id = p_entity_id))
    ORDER BY g.applied_at DESC
    LIMIT 2
),
figures AS (
    -- Promoted (ACTIVE) news-derived people tied to this team — coaches, agents,
    -- executives the provider never seeds (mig 166). News-derived accumulation:
    -- graph-derived context, never ground truth.
    SELECT format('Team figure: %s (%s, news-derived, %s sources).',
               p.name, p.kind, p.distinct_sources) AS line,
           p.mention_count
    FROM narrative_persons p
    WHERE p.sport = p_sport AND p_entity_type = 'team' AND p.team_id = p_entity_id
      AND p.status = 'active' AND p.merged_into IS NULL
    ORDER BY p.mention_count DESC
    LIMIT 3
),
own_reads AS (
    -- The junction family's own STRONGEST recent banked transfer verdict for this
    -- player (outputs-as-memories, mig 168): highest stage first, then freshest —
    -- a post-move own-club speculation row must not outrank a live here_we_go.
    -- Continuity, NEVER corroboration.
    SELECT format('Our prior read: our transfer lens staged %s as %s on %s (confidence %s).',
               t.name, r.stage, to_char(r.generated_at, 'Mon DD'), r.confidence) AS line
    FROM transfer_rumors r
    JOIN teams t ON t.id = r.team_id AND t.sport = r.sport
    WHERE r.sport = p_sport AND p_entity_type = 'player' AND r.player_id = p_entity_id
      AND r.stage IS NOT NULL AND r.generated_at > now() - interval '30 days'
    ORDER BY CASE r.stage WHEN 'here_we_go' THEN 4 WHEN 'advanced_talks' THEN 3
                          WHEN 'concrete_interest' THEN 2 ELSE 1 END DESC,
             r.generated_at DESC
    LIMIT 1
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM sealed),
    (SELECT string_agg(line, E'\n' ORDER BY rank DESC) FROM open_eps),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves),
    (SELECT string_agg(line, E'\n' ORDER BY mention_count DESC) FROM figures),
    (SELECT line FROM own_reads)), '');
$$;

COMMENT ON FUNCTION public.narrative_context_for_entity(text, text, integer) IS
    'Per-entity memory card for junction prompts: sealed stories (both edge slots, '
    'outcome-labeled), open stories with likelihood (own-club employment excluded for '
    'players), recent ground-truth moves, active news-derived team figures (mig 166), '
    'and the transfer lens''s own latest staged read as an "Our prior read:" line '
    '(mig 168). Provenance-labeled — continuity, not corroboration. NULL = no memory. '
    'Consumers: narratives n8, vibe v12, sigil s15, momentum s5. Model-facing only.';

INSERT INTO public.schema_migrations(version) VALUES ('168_outputs_as_memories')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

-- 163_narrative_context_for_entity.sql
--
-- Junction memory rollout step 1 (Progressive Refinement Dataflow, folded plan):
-- narrative_context_for_entity() — the per-ENTITY memory card, the narratives/vibe/
-- sigil counterpart of mig 162's per-pair card. Renders, as provenance-labeled prompt
-- lines (the echo-chamber rule: continuity, not corroboration):
--
--   Prior story:    sealed episodes the entity appears in (EITHER edge slot),
--                   outcome-labeled, newest first (up to 4)
--   Current story:  open episodes ranked by likelihood/strength (up to 3; a player's
--                   own-club employment story is excluded — noise, not narrative)
--   Ground truth:   applied moves touching the entity in the last 120 days (players:
--                   their move; teams: arrivals), from transfer_ground_truth (both
--                   ledgers — anchors, never model output)
--
-- NULL when the graph holds no memory — the prompt renders no section. Consumed first
-- by the narratives junction (n7 → n8); vibe and sigil reuse it next with their own
-- prompt framings. Same deliberate decision as mig 162: NOT part of any input_hash.
--
-- Deploy order: ADDITIVE — apply BEFORE deploying the n8 cognition binary.

BEGIN;

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
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM sealed),
    (SELECT string_agg(line, E'\n' ORDER BY rank DESC) FROM open_eps),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves)), '');
$$;

COMMENT ON FUNCTION public.narrative_context_for_entity(text, text, integer) IS
    'Per-entity memory card for junction prompts: sealed stories (both edge slots, '
    'outcome-labeled), open stories with likelihood (own-club employment excluded for '
    'players), recent ground-truth moves. Provenance-labeled lines — continuity, not '
    'corroboration. NULL = no memory. Consumers: narratives n8 first; vibe/sigil next. '
    'Model-facing only.';

INSERT INTO public.schema_migrations(version) VALUES ('163_narrative_context_for_entity')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

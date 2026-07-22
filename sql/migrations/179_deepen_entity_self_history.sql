-- 179_deepen_entity_self_history.sql
--
-- Cognition refactor, Phase 6 (deepen memory under the continuity/measurement partition —
-- the LAST plan phase). CONTINUITY-ONLY + ADDITIVE. Widens the shared per-entity memory card
-- narrative_context_for_entity so the junctions build on a real self-history instead of a
-- single transfer read.
--
-- THE GAP THIS CLOSES: today the card's "Our prior read:" line is JUST the transfer lens's one
-- strongest recent staged read (30d, players only). A player (or team) with a rich banked
-- history of our OWN reads across the other lenses — narrative, vibe, momentum, PEAK — but no
-- live graph episode gets a BLANK card. This migration surfaces FIVE source-tagged self-history
-- lenses so every junction (and the folded Oracle crown) reads what we last concluded, tagged to
-- when and how many sources backed it.
--
-- THE FIVE LENSES (all "Our prior read (<lens>, <date>[, N source(s)]): ...", newest-first, capped):
--   * narrative  news_summaries   — trajectory + coverage/100 + the story title, source-tagged
--                                    (source_count) — the richest lens.                LIMIT 3 / 45d
--   * transfer   transfer_rumors  — staged <team> as <stage> (confidence), source-tagged.
--                                    Players only; freshest-2 (was the single strongest). LIMIT 2 / 30d
--   * vibe       vibe_scores      — sentiment/100 (+ article count; no source names banked). LIMIT 2 / 45d
--   * momentum   momentum_summaries — direction (+ signed score).                       LIMIT 2 / 45d
--   * PEAK       stat_summaries   — season divined_peak + notability/100 (+ trajectory label).
--                                    Least-weighted (stats-heavy) — the tail line.       LIMIT 1
--
-- SOURCE-TAGGING is the echo-chamber defense made auditable: each line carries the DATE and, where
-- the lens banks it, HOW MANY outlets backed the read — a 1-source read reads differently than a
-- 5-source one. The render header ("use for arc and continuity ... do NOT treat a prior story as
-- evidence for a new one") already frames these as continuity for every consumer, so NO prompt-
-- version bump and NO binary change: the loaders SELECT the fn and render whatever lines it returns.
--
-- MEASUREMENT UNTOUCHED (Locked decision 1, the provenance partition): heat / likelihood / confirm /
-- fizzle still come only from narrative_episodes (graph) + transfer_ground_truth. This migration adds
-- NO measurement write and writes NOTHING to narrative_events — the Phase 0 firewall
-- (assert_provenance_firewall, mig 172) stays green trivially; reading our output tables into a
-- display card is not banking a junction verdict as a measurement event. The self-history lenses are
-- pure continuity; the AUTHORED-MEMORY MOAT ARC (a separate, later phase) is what would flip their
-- authority — NOT this.
--
-- Also RAISED the measurement-section caps deliberately, now that dedup/novelty keep the corpus clean
-- (the point of Phase 6): sealed 4->6, open_eps 3->5, moves 2->3, figures 3->4. Still raw/graph-anchored.
--
-- ADDITIVE + binary-compatible — the deeper card serves on the next junction render; reaches each
-- entity on its natural regen (memory is deliberately NOT part of input_hash), no forced backfill.

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
    LIMIT 6
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
    LIMIT 5
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
    LIMIT 3
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
    LIMIT 4
),
-- ------------------------------------------------------------------------------
-- OUR OWN SELF-HISTORY (outputs-as-memories, mig 168 + Phase 6): five lenses, all
-- provenance-labeled continuity, NEVER corroboration. Source-tagged where the lens banks it.
-- ------------------------------------------------------------------------------
own_narrative AS (
    -- The Journalist's own recent reads (news_summaries). Richest lens: trajectory +
    -- coverage + the story title, tagged with how many outlets backed it.
    SELECT format('Our prior read (narrative, %s%s): %s%s%s.',
               to_char(ns.generated_at, 'Mon DD'),
               CASE WHEN ns.source_count > 0
                    THEN format(', %s source%s', ns.source_count,
                                CASE WHEN ns.source_count = 1 THEN '' ELSE 's' END)
                    ELSE '' END,
               replace(ns.trajectory, '_', ' '),
               COALESCE(', coverage ' || ns.impact || '/100', ''),
               COALESCE(' — "' || ns.narrative_title || '"', '')) AS line,
           ns.generated_at AS ord
    FROM news_summaries ns
    WHERE ns.sport = p_sport AND ns.entity_type = p_entity_type AND ns.entity_id = p_entity_id
      AND ns.body IS NOT NULL AND ns.generated_at > now() - interval '45 days'
    ORDER BY ns.generated_at DESC
    LIMIT 3
),
own_transfer AS (
    -- The transfer lens's own recent staged reads (transfer_rumors). Players only.
    -- Freshest two = the recent read trajectory, source-tagged.
    SELECT format('Our prior read (transfer, %s%s): staged %s as %s%s.',
               to_char(r.generated_at, 'Mon DD'),
               CASE WHEN r.source_count > 0
                    THEN format(', %s source%s', r.source_count,
                                CASE WHEN r.source_count = 1 THEN '' ELSE 's' END)
                    ELSE '' END,
               t.name, r.stage,
               COALESCE(' (confidence ' || r.confidence || ')', '')) AS line,
           r.generated_at AS ord
    FROM transfer_rumors r
    JOIN teams t ON t.id = r.team_id AND t.sport = r.sport
    WHERE r.sport = p_sport AND p_entity_type = 'player' AND r.player_id = p_entity_id
      AND r.stage IS NOT NULL AND r.generated_at > now() - interval '30 days'
    ORDER BY r.generated_at DESC
    LIMIT 2
),
own_vibe AS (
    -- The vibe lens's own recent sentiment reads (vibe_scores). No source names banked;
    -- tag with the article count instead.
    SELECT format('Our prior read (vibe, %s): sentiment %s/100%s.',
               to_char(v.generated_at, 'Mon DD'),
               v.sentiment,
               CASE WHEN array_length(v.input_news_ids, 1) > 0
                    THEN format(' (%s article%s)', array_length(v.input_news_ids, 1),
                                CASE WHEN array_length(v.input_news_ids, 1) = 1 THEN '' ELSE 's' END)
                    ELSE '' END) AS line,
           v.generated_at AS ord
    FROM vibe_scores v
    WHERE v.sport = p_sport AND v.entity_type = p_entity_type AND v.entity_id = p_entity_id
      AND v.sentiment IS NOT NULL AND v.generated_at > now() - interval '45 days'
    ORDER BY v.generated_at DESC
    LIMIT 2
),
own_momentum AS (
    -- The momentum lens's own recent reads (momentum_summaries).
    SELECT format('Our prior read (momentum, %s): %s%s.',
               to_char(m.generated_at, 'Mon DD'),
               m.direction,
               COALESCE(' (score ' || m.score || ')', '')) AS line,
           m.generated_at AS ord
    FROM momentum_summaries m
    WHERE m.sport = p_sport AND m.entity_type = p_entity_type AND m.entity_id = p_entity_id
      AND m.direction IS NOT NULL AND m.generated_at > now() - interval '45 days'
    ORDER BY m.generated_at DESC
    LIMIT 2
),
own_peak AS (
    -- The PEAK lens's latest banked read (stat_summaries). Least-weighted (stats-heavy) —
    -- the tail line. Season-keyed, so just the latest.
    SELECT format('Our prior read (PEAK, season %s): "%s" (notability %s/100)%s.',
               s.season, s.divined_peak, s.notability,
               CASE WHEN COALESCE(s.peak_trajectory_label, '') <> ''
                    THEN '; ' || s.peak_trajectory_label ELSE '' END) AS line
    FROM stat_summaries s
    WHERE s.sport = p_sport AND s.entity_type = p_entity_type AND s.entity_id = p_entity_id
      AND s.body IS NOT NULL AND COALESCE(s.divined_peak, '') <> ''
    ORDER BY s.season DESC, s.generated_at DESC
    LIMIT 1
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM sealed),
    (SELECT string_agg(line, E'\n' ORDER BY rank DESC) FROM open_eps),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves),
    (SELECT string_agg(line, E'\n' ORDER BY mention_count DESC) FROM figures),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_narrative),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_transfer),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_vibe),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_momentum),
    (SELECT line FROM own_peak)), '');
$$;

COMMENT ON FUNCTION public.narrative_context_for_entity(text, text, integer) IS
    'Per-entity memory card for junction prompts: sealed stories (both edge slots, '
    'outcome-labeled), open stories with likelihood (own-club employment excluded for '
    'players), recent ground-truth moves, active news-derived team figures (mig 166), and '
    '(Phase 6, mig 179) our own FIVE-lens source-tagged self-history as "Our prior read '
    '(<lens>, <date>[, N sources]): ..." continuity lines — narrative (news_summaries), '
    'transfer (transfer_rumors, players), vibe (vibe_scores), momentum (momentum_summaries), '
    'PEAK (stat_summaries). Provenance-labeled — continuity, NOT corroboration; measurement '
    '(heat/likelihood/confirm/fizzle) stays raw/graph-anchored. NULL = no memory. Consumers: '
    'narratives n9, vibe v12, momentum s5, sigil s15. Model-facing only.';

INSERT INTO public.schema_migrations(version) VALUES ('179_deepen_entity_self_history')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

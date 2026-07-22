-- 182_thread_progression_card.sql
--
-- Characters Phase C, read surfaces (rides mig 181's narrative_threads):
--
--   1. narrative_context_for_entity: the own_narrative self-history lens upgrades from 3 FLAT
--      "Our prior read (narrative, ...)" lines to THREAD PROGRESSION — per open thread a header
--      (current canonical title, opened date, entry + source totals) plus its last 3 chapters,
--      each with its per-step cited source count. The memory card now shows HOW a story moved
--      (developing -> heating -> cooling with dates and corroboration), not 3 disconnected
--      snapshots. Budget: <=2 threads x (header + 3 steps) ≈ 8 lean lines vs the old 3 wide
--      ones — num_ctx budgets hold (mig 179's render-header framing is unchanged, so still no
--      prompt-version bump; consumers pick the deeper card up on natural regen).
--      Derived from the CURRENT prod definition (verified byte-identical to mig 179 via \sf
--      before writing this). Only the own_narrative CTE and the COMMENT change.
--
--   2. v_narrative_threads (F6: citations made visible): one row per (thread, chapter) with the
--      chapter's cited article ids + source names — "the story so far" for the Go serving layer.
--      Feeds The Journalist's, The Insider's, and The Influencer's card surfaces; Go groups by
--      thread_id, ordered by entry_recency_rank.
--
-- APPLY ORDER: after mig 181 AND after the thread backfill (examples/thread_backfill.rs) has
-- stamped history — the lens reads threads, so an un-backfilled entity would render an empty
-- narrative section until its stories re-thread organically.

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
    -- (mig 182, Phase C) The Journalist's storylines as PROGRESSING THREADS (mig 181): per
    -- open thread a header — current canonical title, opened date, totals — plus the last
    -- 3 chapters, newest-first, each tagged with its OWN cited source count. One multi-line
    -- block per thread; ord = recency so the outer aggregate keeps the freshest story first.
    SELECT format('Our story so far ("%s", opened %s, %s entr%s, %s source%s):%s',
               t.canonical_title,
               to_char(t.opened_at, 'Mon DD'),
               t.entry_count, CASE WHEN t.entry_count = 1 THEN 'y' ELSE 'ies' END,
               t.distinct_sources, CASE WHEN t.distinct_sources = 1 THEN '' ELSE 's' END,
               steps.txt) AS line,
           t.last_progressed_at AS ord
    FROM narrative_threads t
    CROSS JOIN LATERAL (
        SELECT E'\n' || string_agg(
                   format('  %s (%s source%s): %s, coverage %s/100',
                       to_char(c.generated_at, 'Mon DD'),
                       c.source_count, CASE WHEN c.source_count = 1 THEN '' ELSE 's' END,
                       replace(c.trajectory, '_', ' '),
                       c.impact),
                   E'\n' ORDER BY c.generated_at DESC) AS txt
        FROM (
            SELECT s.generated_at, s.source_count, s.trajectory, s.impact
            FROM news_summaries s
            WHERE s.thread_id = t.id AND s.body IS NOT NULL AND s.impact IS NOT NULL
            ORDER BY s.generated_at DESC
            LIMIT 3
        ) c
    ) steps
    WHERE t.sport = p_sport AND t.entity_type = p_entity_type AND t.entity_id = p_entity_id
      AND t.status = 'open' AND steps.txt IS NOT NULL
    ORDER BY t.last_progressed_at DESC
    LIMIT 2
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
    'our own five-lens source-tagged self-history (mig 179). The narrative lens (mig 182, '
    'Phase C) renders THREAD PROGRESSION: per open narrative_threads row (mig 181) a header '
    'plus its last 3 chapters as "Our story so far (...)" with per-step cited source counts. '
    'The other lenses stay flat "Our prior read (<lens>, <date>[, N sources]): ..." lines — '
    'transfer (transfer_rumors, players), vibe (vibe_scores), momentum (momentum_summaries), '
    'PEAK (stat_summaries). Provenance-labeled — continuity, NOT corroboration; measurement '
    '(heat/likelihood/confirm/fizzle) stays raw/graph-anchored. NULL = no memory. Consumers: '
    'narratives n10, vibe v13, momentum s6, sigil or4. Model-facing only.';

-- ---------------------------------------------------------------------------
-- F6: the citations, made visible. One row per (thread, chapter); Go groups by
-- thread_id and walks entry_recency_rank to serve "the story so far".
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW public.v_narrative_threads AS
SELECT t.id AS thread_id,
       t.sport,
       t.entity_type,
       t.entity_id,
       t.canonical_title,
       t.status,
       t.outcome,
       t.opened_at,
       t.last_progressed_at,
       t.sealed_at,
       t.entry_count,
       t.peak_impact,
       t.last_impact,
       t.last_trajectory,
       t.distinct_sources,
       t.source_names,
       s.id AS entry_id,
       s.generated_at AS entry_at,
       s.narrative_title AS entry_title,
       s.body AS entry_body,
       s.impact AS entry_impact,
       s.trajectory AS entry_trajectory,
       s.source_count AS entry_source_count,
       s.source_names AS entry_source_names,
       s.input_news_ids AS entry_news_ids,
       row_number() OVER (PARTITION BY t.id ORDER BY s.generated_at DESC, s.id DESC)
           AS entry_recency_rank
FROM public.narrative_threads t
JOIN public.news_summaries s ON s.thread_id = t.id AND s.body IS NOT NULL;

COMMENT ON VIEW public.v_narrative_threads IS
    '"The story so far" with citations (F6, mig 182): each progressing-narrative thread '
    '(mig 181) joined to its attached news_summaries chapters — per-chapter title, body, '
    'impact, trajectory, cited article ids (entry_news_ids) and source names. Serving '
    'surface for The Journalist''s, The Insider''s, and The Influencer''s cards; group by '
    'thread_id, order by entry_recency_rank (1 = newest chapter).';

INSERT INTO public.schema_migrations(version) VALUES ('182_thread_progression_card')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

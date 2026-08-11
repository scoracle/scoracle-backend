-- 218: descrub the memory cards' PROMPT VOCABULARY (D-T57's schema follow-through).
--
-- Scott, 2026-08-10: served prose must never name the internal machinery ("PEAK", "Vibe") —
-- and the s13-analyst postmortem says a prompt-side ban cannot beat a word the INPUT keeps
-- shouting. The junctions stopped shouting (s18/s15/or9 renamed every model-facing label in
-- Rust), but two SQL functions still rendered the desk's vocabulary straight into the
-- per-entity memory cards every voice reads:
--
--   narrative_context_for_entity (mig 211):  "Our prior read (vibe, ...): sentiment N/100"
--                                            "Our prior read (PEAK, season ...): ... (notability N/100)"
--   stat_context_for_entity      (mig 164):  "Our prior read: season N PEAK was ... (notability N/100)"
--
-- This migration re-states both functions with the sport's words: vibe->mood,
-- sentiment->mood, PEAK->top skill, notability->profile distinctiveness. The
-- stat_context_for_entity strings match rust `scout::descrub_memory_card`'s output EXACTLY,
-- so that Rust shim becomes a no-op (it stays, as the belt-and-braces contract note).
-- Memory cards are prompt-only and outside every input_hash: NOTHING regenerates from this
-- change by itself; new wording rides along on the next natural build of each prompt.
--
-- Deploy order: ADDITIVE (CREATE OR REPLACE over the current prod definitions, mig 211 and
-- mig 164 respectively, with ONLY the format-string edits above).

CREATE OR REPLACE FUNCTION public.narrative_context_for_entity(p_sport text, p_entity_type text, p_entity_id integer)
 RETURNS text
 LANGUAGE sql
 STABLE
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
established AS (
    -- (mig 183, Phase D) ESTABLISHED stories: threads whose source growth crossed the
    -- authority gate. They graduate OUT of the "Our story so far" progression block and
    -- render here as one-line BACKGROUND FACTS — settled context the model may speak
    -- from, deliberately carrying NO impact/likelihood figures (source count + opening
    -- date are breadth and tenure, not measurement). Open threads only: a resolved
    -- established thread's confirmation already renders as Ground truth above.
    SELECT format('Established story (our archive, %s sources, since %s): "%s".',
               t.distinct_sources,
               to_char(t.opened_at, 'Mon DD'),
               t.canonical_title) AS line,
           t.last_progressed_at AS ord
    FROM narrative_threads t
    WHERE t.sport = p_sport AND t.entity_type = p_entity_type AND t.entity_id = p_entity_id
      AND t.status = 'open' AND t.authority = 'established'
    ORDER BY t.last_progressed_at DESC
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
    LIMIT 4
),
-- ------------------------------------------------------------------------------
-- OUR OWN SELF-HISTORY (outputs-as-memories, mig 168 + Phase 6): five lenses, all
-- provenance-labeled continuity, NEVER corroboration. Source-tagged where the lens banks it.
-- ------------------------------------------------------------------------------
own_storyline AS (
    -- (mig 211, PLAN-one-rail 7.10) THE STORYLINE LENS — the successor to the thread block
    -- above it. Phase 9 retires thread clustering; the Desk's storylines (§1b, assembled in
    -- code, never matched by a model) are what a character's "life of stories" becomes, and
    -- this line is how that memory survives the retirement. One line per OPEN storyline this
    -- entity is an ACTIVE participant in (left_at IS NULL — D5: a part has its own lifespan,
    -- and an entity written out of a story stops remembering it).
    --
    -- The headline is the LATEST packet's, falling back to the storyline's display title:
    -- packets are append-only snapshots, so the newest is the current state of the story and
    -- the older ones are archive. Report count is membership, not measurement — the same
    -- discipline as the ESTABLISHED line (breadth and tenure, never impact or likelihood).
    -- Provenance-labeled continuity, NOT corroboration: it tells a voice which stories it is
    -- already inside, never that a claim is true.
    SELECT format('Our storyline so far ("%s", opened %s, %s report%s%s).',
               COALESCE(NULLIF(p.headline, ''), NULLIF(s.title, ''), 'untitled'),
               to_char(se.joined_at, 'Mon DD'),
               m.n, CASE WHEN m.n = 1 THEN '' ELSE 's' END,
               CASE WHEN COALESCE(se.role, '') <> ''
                    THEN format(', this entity''s part: %s', se.role) ELSE '' END) AS line,
           s.last_seen_at AS ord
    FROM storyline_entities se
    JOIN storylines s ON s.id = se.storyline_id
    LEFT JOIN LATERAL (
        SELECT pk.headline
        FROM packets pk
        WHERE pk.storyline_id = s.id
        ORDER BY pk.compiled_at DESC, pk.id DESC
        LIMIT 1
    ) p ON true
    CROSS JOIN LATERAL (
        SELECT count(*) AS n FROM storyline_articles sa WHERE sa.storyline_id = s.id
    ) m
    WHERE se.sport = p_sport AND se.entity_type = p_entity_type AND se.entity_id = p_entity_id
      AND se.left_at IS NULL
      AND s.status = 'open'
    ORDER BY s.last_seen_at DESC
    LIMIT 3
),
own_narrative AS (
    -- (mig 182, Phase C) The Journalist's storylines as PROGRESSING THREADS (mig 181): per
    -- open thread a header — current canonical title, opened date, totals — plus the last
    -- 3 chapters, newest-first, each tagged with its OWN cited source count. One multi-line
    -- block per thread; ord = recency so the outer aggregate keeps the freshest story first.
    -- Continuity threads only (mig 183): established threads graduate to the background-
    -- fact line above.
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
      AND t.status = 'open' AND t.authority = 'continuity' AND steps.txt IS NOT NULL
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
    SELECT format('Our prior read (mood, %s): mood %s/100%s.',
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
    SELECT format('Our prior read (top skill, season %s): "%s" (profile distinctiveness %s/100)%s.',
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
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM established),
    (SELECT string_agg(line, E'\n' ORDER BY mention_count DESC) FROM figures),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_storyline),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_narrative),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_transfer),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_vibe),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_momentum),
    (SELECT line FROM own_peak)), '');
$$;


CREATE OR REPLACE FUNCTION public.stat_context_for_entity(
    p_sport text,
    p_entity_type text,
    p_entity_id integer,
    p_season integer
) RETURNS text
    LANGUAGE sql STABLE
    AS $$
WITH prior_read AS (
    SELECT format('Our prior read: season %s the top skill read was "%s" (profile distinctiveness %s/100)%s.',
               s.season, s.divined_peak, s.notability,
               CASE WHEN COALESCE(s.peak_trajectory_label, '') <> ''
                    THEN '; ' || s.peak_trajectory_label ELSE '' END) AS line
    FROM stat_summaries s
    WHERE s.entity_type = p_entity_type AND s.entity_id = p_entity_id
      AND s.sport = p_sport AND s.season < p_season
      AND s.body IS NOT NULL AND COALESCE(s.divined_peak, '') <> ''
    ORDER BY s.season DESC, s.generated_at DESC
    LIMIT 1
),
moves AS (
    SELECT format('Ground truth: %s on %s.',
               CASE WHEN p_entity_type = 'player'
                    THEN 'joined ' || tm.name
                    ELSE 'signed ' || pl.name END,
               to_char(g.applied_at, 'Mon DD YYYY')) AS line,
           g.applied_at
    FROM transfer_ground_truth g
    JOIN players pl ON pl.id = g.player_id AND pl.sport = g.sport
    JOIN teams tm ON tm.id = g.team_id AND tm.sport = g.sport
    WHERE g.sport = p_sport
      AND g.applied_at > now() - interval '180 days'
      AND ((p_entity_type = 'player' AND g.player_id = p_entity_id)
        OR (p_entity_type = 'team' AND g.team_id = p_entity_id))
    ORDER BY g.applied_at DESC
    LIMIT 3
),
matchups AS (
    SELECT format('Matchup memory: %s vs %s — %s/game vs a %s baseline (adjusted %s%s), n=%s games, reliability %s/100.',
               m.stat_key, tm.name,
               round(m.matchup_avg, 1), round(m.baseline_avg, 1),
               CASE WHEN m.shrunk_delta >= 0 THEN '+' ELSE '' END,
               round(m.shrunk_delta, 1),
               m.n_games, m.reliability) AS line,
           (m.reliability / 100.0) * abs(m.shrunk_delta) AS rank
    FROM stat_matchups m
    JOIN teams tm ON tm.id = m.object_id AND tm.sport = m.sport
    WHERE m.sport = p_sport AND m.scope = 'career'
      AND m.subject_type = p_entity_type AND m.subject_id = p_entity_id
      AND m.object_type = 'team'
      AND p_entity_type = 'player'
    ORDER BY rank DESC
    LIMIT 3
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT line FROM prior_read),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves),
    (SELECT string_agg(line, E'\n' ORDER BY rank DESC) FROM matchups)), '');
$$;


COMMENT ON FUNCTION public.stat_context_for_entity(text, text, integer, integer) IS
  'Stats-side memory card for the peak/statcommentary junction (rating s12; vocabulary descrubbed mig 218): prior-season top-skill read (banked output, echo-chamber rule), confirmed moves, reliability-framed matchup edges. Model-facing only; never user-exposed; outside input_hash.';

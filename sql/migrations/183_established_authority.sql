-- 183_established_authority.sql
--
-- Characters Phase D (authored-memory moat arc, opening move): narrative_threads gain an
-- AUTHORITY tier. Every thread starts as 'continuity' (self-history: "our story so far",
-- provenance-labeled, weighed lightly). A thread whose coverage GROWS PAST the gate —
-- corroboration breadth + persistence, or a ground-truth confirmation — flips to
-- 'established': our archive now holds a settled account of this storyline, and the memory
-- card presents it as BACKGROUND FACT rather than as one of our prior reads.
--
--   Gate (ONE auditable home, narrative_thread_established_gate):
--     >= 5 distinct sources AND >= 3 entries AND opened >= 14 days ago,
--     OR sealed as 'resolved' (ground truth confirmed the story — the seal sweep's
--     transfer-flavored + applied_at >= opened_at logic already vetted that, mig 181).
--   No demotion in v1: the flip is one-way (distinct_sources/entry_count never shrink;
--   revisit only if the thresholds themselves change).
--
--   Flip mechanics: promote_established_threads(sport) — called nightly from
--   cron-narrative-links.sh (to_regprocedure-guarded, after the seal sweep so a fresh
--   ground-truth resolve promotes the same night) and ONCE at apply time below (888 open
--   threads already met the gate at authoring; the same organic per-thread flip, just not
--   deferred to the 00:45 tick). No re-generation wave: authority is presentation-tier,
--   not part of any input_hash — consumers pick the new render up on natural regen.
--
--   Card render (narrative_context_for_entity): established OPEN threads graduate OUT of
--   the "Our story so far" progression block and render as one-line background facts —
--   'Established story (our archive, N sources, since <date>): "<title>".' — placed
--   between Ground truth and the self-history reads. NEVER numeric evidence: no impact/
--   likelihood figures in the line (source count + date are breadth/tenure, not
--   measurement). Budget: an established thread SHRINKS from a 4-line progression block
--   to 1 line; worst case (2 established + 2 continuity threads) adds ~2 lean lines vs
--   mig 182 — num_ctx 4096 holds. Derived from the CURRENT prod definition (verified
--   byte-identical to mig 182 via pg_get_functiondef before writing this).
--
--   Serving contract (client credibility surface, rides the NavWell build-out):
--   v_narrative_threads gains the authority column — Go serves it; no Go change required
--   to apply this (view recreated in-transaction, columns only appended semantically).
--
-- Measurement firewall UNTOUCHED: nothing here reads narrative_events or feeds the
-- numeric loop; assert_provenance_firewall() re-run at the end regardless (mig 172 house
-- rule for anything near the moat).
--
-- ADDITIVE and binary-compatible: the running binary calls narrative_context_for_entity
-- opaquely and never reads authority. Pure SQL — no deploy required, no restart.

BEGIN;

ALTER TABLE public.narrative_threads
    ADD COLUMN IF NOT EXISTS authority text NOT NULL DEFAULT 'continuity'
        CHECK (authority IN ('continuity', 'established'));

COMMENT ON COLUMN public.narrative_threads.authority IS
    'Authored-memory tier (Phase D, mig 183): ''continuity'' = self-history, weighed '
    'lightly; ''established'' = source growth crossed the gate '
    '(narrative_thread_established_gate) — the card renders it as background fact. '
    'One-way flip (no demotion v1), promoted nightly by promote_established_threads(). '
    'Presentation-tier only: never numeric evidence, never in an input_hash.';

-- ---------------------------------------------------------------------------
-- The gate — the ONE auditable home for the establishment thresholds. Row-typed so
-- every caller (promotion, ad-hoc audit) judges a thread by exactly the same rule:
--   SELECT id, canonical_title FROM narrative_threads t
--    WHERE narrative_thread_established_gate(t) AND authority = 'continuity';
-- is the audit query for "what would flip tonight".
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.narrative_thread_established_gate(t public.narrative_threads)
RETURNS boolean
LANGUAGE sql STABLE
AS $$
    SELECT (t.status = 'sealed' AND t.outcome = 'resolved')
        OR (    t.distinct_sources >= 5
            AND t.entry_count      >= 3
            AND t.opened_at        <= now() - interval '14 days');
$$;

COMMENT ON FUNCTION public.narrative_thread_established_gate(public.narrative_threads) IS
    'THE establishment gate (Phase D, mig 183) — the single auditable home for the '
    'authority thresholds: >= 5 distinct sources AND >= 3 entries AND >= 14 days old, OR '
    'ground-truth confirmed (sealed resolved, which mig 181''s seal sweep already vetted '
    'as transfer-flavored + applied since opening). Change thresholds HERE only.';

-- ---------------------------------------------------------------------------
-- The organic flip — nightly from cron-narrative-links.sh (after the thread seal
-- sweep, so a same-night ground-truth resolve promotes immediately). One-way.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.promote_established_threads(p_sport text)
RETURNS integer
LANGUAGE sql
AS $$
WITH flipped AS (
    UPDATE narrative_threads t
       SET authority = 'established'
     WHERE t.sport = p_sport
       AND t.authority = 'continuity'
       AND public.narrative_thread_established_gate(t)
    RETURNING t.id
)
SELECT count(*)::integer FROM flipped;
$$;

COMMENT ON FUNCTION public.promote_established_threads(text) IS
    'Nightly authority promotion (Phase D, mig 183, cron-narrative-links.sh): flips '
    'continuity threads that pass narrative_thread_established_gate() to established. '
    'One-way (no demotion v1); returns the number of threads flipped.';

-- ---------------------------------------------------------------------------
-- Memory card: established open threads render as background facts between Ground
-- truth and the self-history reads; continuity threads keep the progression block.
-- Only the established CTE (new), the own_narrative CTE (authority filter), the
-- concat_ws slot, and the COMMENT change vs mig 182.
-- ---------------------------------------------------------------------------
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
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM established),
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
    'players), recent ground-truth moves, ESTABLISHED stories (mig 183, Phase D: threads '
    'past the authority gate render as one-line background facts — "Established story '
    '(our archive, N sources, since <date>)" — between Ground truth and the self-history '
    'reads; never numeric evidence), active news-derived team figures (mig 166), and our '
    'own five-lens source-tagged self-history (mig 179). The narrative lens (mig 182, '
    'Phase C) renders THREAD PROGRESSION for CONTINUITY threads: per open '
    'narrative_threads row (mig 181) a header plus its last 3 chapters as "Our story so '
    'far (...)" with per-step cited source counts. The other lenses stay flat "Our prior '
    'read (<lens>, <date>[, N sources]): ..." lines — transfer (transfer_rumors, '
    'players), vibe (vibe_scores), momentum (momentum_summaries), PEAK (stat_summaries). '
    'Provenance-labeled — continuity, NOT corroboration; measurement '
    '(heat/likelihood/confirm/fizzle) stays raw/graph-anchored. NULL = no memory. '
    'Consumers: narratives n10, vibe v13, momentum s6, sigil or4. Model-facing only.';

-- ---------------------------------------------------------------------------
-- Serving contract: v_narrative_threads carries authority for the client credibility
-- surface (NavWell build-out reads it; recreated because CREATE OR REPLACE VIEW cannot
-- add a column mid-list — in-transaction, so serving never sees the gap).
-- ---------------------------------------------------------------------------
DROP VIEW IF EXISTS public.v_narrative_threads;

CREATE VIEW public.v_narrative_threads AS
SELECT t.id AS thread_id,
       t.sport,
       t.entity_type,
       t.entity_id,
       t.canonical_title,
       t.status,
       t.outcome,
       t.authority,
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
    '"The story so far" with citations (F6, mig 182; authority added mig 183): each '
    'progressing-narrative thread (mig 181) joined to its attached news_summaries '
    'chapters — per-chapter title, body, impact, trajectory, cited article ids '
    '(entry_news_ids) and source names, plus the thread''s authority tier '
    '(continuity|established) for the client credibility surface. Serving surface for '
    'The Journalist''s, The Insider''s, and The Influencer''s cards; group by thread_id, '
    'order by entry_recency_rank (1 = newest chapter).';

-- ---------------------------------------------------------------------------
-- Initial promotion pass — the same organic per-thread flip the cron will run
-- nightly, executed once at apply time so already-qualified threads (888 open at
-- authoring) render as established tonight instead of after the 00:45 tick.
-- ---------------------------------------------------------------------------
DO $$
DECLARE r record;
BEGIN
    FOR r IN SELECT s.sport, public.promote_established_threads(s.sport) AS promoted
             FROM (VALUES ('FOOTBALL'), ('NBA'), ('NFL')) s(sport) LOOP
        RAISE NOTICE 'promote_established_threads % promoted=%', r.sport, r.promoted;
    END LOOP;
END $$;

-- House rule for anything near the authored-memory moat (mig 172): prove the numeric
-- loop still only reads origin='extraction'.
SELECT public.assert_provenance_firewall();

INSERT INTO public.schema_migrations(version) VALUES ('183_established_authority')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

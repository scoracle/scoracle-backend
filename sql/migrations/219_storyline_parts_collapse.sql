-- 219_storyline_parts_collapse.sql
--
-- WHAT: collapse narrative_threads into the Desk's storyline structure — STEP A of two
-- (additive; the demolition of narrative_threads is step B, after the Rust cutover deploys
-- and verifies). The entity's PART in a storyline (storyline_entities) becomes the unit of
-- narrative progression: it gains the thread's progression columns (entry_count, impacts,
-- trajectory, sources, authority), news_summaries gains storyline_id, and the memory card's
-- three storyline CTEs (established / own_storyline / own_narrative) collapse into one
-- storyline-driven block. New nightly functions seal_storylines() and
-- promote_established_parts() replace seal_narrative_threads() / promote_established_threads().
--
-- WHY: on the packet rail a telling's storyline is a FACT, not a match. Every corpus article
-- reaches the Journalist through a packet (storyline_id NOT NULL), every article belongs to
-- exactly one storyline (the Editor attaches once), and every persisted narrative is grounded
-- on cited article ids — so a chapter's storyline is the mode of its citations. The thread's
-- embedding machinery (BGE-small centroids, cosine >= 0.80, EWMA) exists to give legacy-rail
-- stories a stable identity; the Desk's storylines ARE that identity, assembled in code and
-- never matched by a model (PLAN-one-rail §1b, D3). What the thread carried that is worth
-- keeping is the progression state — and that belongs on the entity's part in the story
-- ("downstream voices tell each entity's part in the story, updating it as it evolves").
--
-- Continuity discipline: provenance-labeled memory, never corroboration. Rendered vocabulary
-- stays as close to mig 218's as possible ("Our story so far (...)", "Established story
-- (our archive, ...)"). Memory cards are prompt-only and outside every input_hash, so nothing
-- regenerates from this change by itself.
--
-- Deploy order: ADDITIVE. Safe to apply before any Rust deploy: a backfill derives
-- news_summaries.storyline_id from citations, and fill_news_summaries_storylines() — also
-- called from cron-narrative-links.sh — keeps filling chapters the current binary writes
-- (thread-only) during the dual period. narrative_threads keeps its writers until the Rust
-- cutover; nothing reads it for memory after this migration.

BEGIN;

-- ---------------------------------------------------------------------------
-- 1. storyline_entities becomes the progression unit.
-- ---------------------------------------------------------------------------

ALTER TABLE public.storyline_entities
    ADD COLUMN IF NOT EXISTS entry_count integer DEFAULT 0 NOT NULL,
    ADD COLUMN IF NOT EXISTS peak_impact smallint,
    ADD COLUMN IF NOT EXISTS last_impact smallint,
    ADD COLUMN IF NOT EXISTS last_trajectory text DEFAULT 'developing_story' NOT NULL,
    ADD COLUMN IF NOT EXISTS distinct_sources integer DEFAULT 0 NOT NULL,
    ADD COLUMN IF NOT EXISTS source_names text[] DEFAULT '{}' NOT NULL,
    ADD COLUMN IF NOT EXISTS authority text DEFAULT 'continuity' NOT NULL,
    ADD COLUMN IF NOT EXISTS last_progressed_at timestamp with time zone;

ALTER TABLE public.storyline_entities
    DROP CONSTRAINT IF EXISTS storyline_entities_last_trajectory_check;
ALTER TABLE public.storyline_entities
    ADD CONSTRAINT storyline_entities_last_trajectory_check
    CHECK ((last_trajectory = ANY (ARRAY['developing_story'::text, 'heating_up'::text, 'cooling_off'::text])));

ALTER TABLE public.storyline_entities
    DROP CONSTRAINT IF EXISTS storyline_entities_authority_check;
ALTER TABLE public.storyline_entities
    ADD CONSTRAINT storyline_entities_authority_check
    CHECK ((authority = ANY (ARRAY['continuity'::text, 'established'::text])));

COMMENT ON COLUMN public.storyline_entities.entry_count IS 'Journalist tellings attached to this part (successor to narrative_threads.entry_count, mig 219). Incremented by the persist path; backfilled from news_summaries chapters.';
COMMENT ON COLUMN public.storyline_entities.peak_impact IS 'Highest telling impact this part has carried (0-100).';
COMMENT ON COLUMN public.storyline_entities.last_impact IS 'Impact of the freshest telling — the classify_delta anchor for the NEXT generation (siblings in one generation all compare against the prior state, never each other).';
COMMENT ON COLUMN public.storyline_entities.last_trajectory IS 'Trajectory marker of the freshest telling: developing_story, heating_up, or cooling_off — the shared vocabulary.';
COMMENT ON COLUMN public.storyline_entities.distinct_sources IS 'Distinct source names accumulated across this part''s tellings (authority gate input).';
COMMENT ON COLUMN public.storyline_entities.source_names IS 'Accumulated distinct source names across this part''s tellings.';
COMMENT ON COLUMN public.storyline_entities.authority IS 'Authored-memory tier (successor to narrative_threads.authority, mig 183): ''continuity'' = self-history, weighed lightly; ''established'' = source growth crossed storyline_part_established_gate() — the card renders it as a background fact. One-way flip (no demotion), promoted nightly by promote_established_parts(). Presentation-tier only: never numeric evidence, never in an input_hash.';
COMMENT ON COLUMN public.storyline_entities.last_progressed_at IS 'When the Journalist last attached a telling to this part. Distinct from last_seen_at (the Editor''s coverage touch): dormancy keys off last_seen_at, memory ordering off last_progressed_at.';

COMMENT ON TABLE public.storyline_entities IS 'D5: an entity''s part in a storyline has its own lifespan. left_at IS NULL = active participant (the packet fan-out grain). On resolution, code names who the story resolved for and closes every other edge with exit_reason ''not_the_outcome'' in one stroke. Mig 219: the part is also the unit of NARRATIVE PROGRESSION — it carries the telling count, impacts, trajectory, sources and authority that narrative_threads carried before the collapse; the Journalist updates parts, never creates story identity.';

-- ---------------------------------------------------------------------------
-- 2. news_summaries gains storyline_id (the chapter → story pointer).
-- ---------------------------------------------------------------------------

ALTER TABLE public.news_summaries
    ADD COLUMN IF NOT EXISTS storyline_id bigint;

ALTER TABLE public.news_summaries
    DROP CONSTRAINT IF EXISTS news_summaries_storyline_id_fkey;
ALTER TABLE public.news_summaries
    ADD CONSTRAINT news_summaries_storyline_id_fkey
    FOREIGN KEY (storyline_id) REFERENCES public.storylines(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_news_summaries_storyline
    ON public.news_summaries (storyline_id, generated_at DESC)
    WHERE storyline_id IS NOT NULL;

COMMENT ON COLUMN public.news_summaries.storyline_id IS 'The storyline this telling is a chapter of (mig 219, successor to thread_id). Derived deterministically: on the packet rail a chapter''s storyline is the mode of its cited articles'' storylines (fill_news_summaries_storylines() / the Rust persist). NULL for marker rows and legacy-rail chapters whose articles predate the Desk.';

-- ---------------------------------------------------------------------------
-- 3. The derivation, as one idempotent function: backfill AND the dual-period
--    nightly fill (chapters the pre-cutover binary writes, thread-only).
--    Scoped to storylines the entity participates in — a chapter belongs to a
--    PART. Deterministic tie-break: most cites, then lowest storyline_id.
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.fill_news_summaries_storylines() RETURNS integer
    LANGUAGE sql
    AS $$
WITH cited AS (
    SELECT ns.id, sa.storyline_id, count(*) AS cites
    FROM public.news_summaries ns
    CROSS JOIN LATERAL unnest(ns.input_news_ids) AS aid(article_id)
    JOIN public.storyline_articles sa ON sa.article_id = aid.article_id
    WHERE ns.storyline_id IS NULL
      AND ns.narrative_title IS NOT NULL
      AND EXISTS (SELECT 1 FROM public.storyline_entities se
                   WHERE se.storyline_id = sa.storyline_id
                     AND se.entity_type = ns.entity_type
                     AND se.entity_id = ns.entity_id
                     AND se.sport = ns.sport)
    GROUP BY ns.id, sa.storyline_id
),
best AS (
    SELECT DISTINCT ON (id) id, storyline_id
    FROM cited
    ORDER BY id, cites DESC, storyline_id
),
upd AS (
    UPDATE public.news_summaries ns
       SET storyline_id = b.storyline_id
      FROM best b
     WHERE ns.id = b.id
       AND ns.storyline_id IS NULL
    RETURNING 1
)
SELECT count(*)::integer FROM upd;
$$;

COMMENT ON FUNCTION public.fill_news_summaries_storylines() IS 'Mig 219 (threads → storyline_parts collapse, step A): fills news_summaries.storyline_id for chapters that lack it, by citation mode (most-cited storyline among the chapter''s articles, scoped to storylines the entity participates in; tie → lowest storyline_id). Ran once as the backfill; cron-narrative-links.sh calls it nightly during the dual period so chapters the pre-cutover binary writes converge without waiting for the Rust deploy. Dropped with narrative_threads in step B.';

-- Backfill, inside this transaction.
SELECT public.fill_news_summaries_storylines() AS chapters_filled;

-- ---------------------------------------------------------------------------
-- 4. Roll each part's progression state up from its (now-filled) chapters,
--    then inherit the authority of established threads whose chapters map.
-- ---------------------------------------------------------------------------

WITH agg AS (
    SELECT ns.storyline_id, ns.entity_type, ns.entity_id,
           count(*) AS entries,
           max(ns.impact) AS peak_impact,
           max(ns.generated_at) AS progressed_at
    FROM public.news_summaries ns
    WHERE ns.storyline_id IS NOT NULL AND ns.narrative_title IS NOT NULL
    GROUP BY 1, 2, 3
),
lastc AS (
    SELECT DISTINCT ON (storyline_id, entity_type, entity_id)
           storyline_id, entity_type, entity_id, impact, trajectory
    FROM public.news_summaries
    WHERE storyline_id IS NOT NULL AND narrative_title IS NOT NULL
    ORDER BY storyline_id, entity_type, entity_id, generated_at DESC, id DESC
),
src AS (
    SELECT ns.storyline_id, ns.entity_type, ns.entity_id,
           array_agg(DISTINCT s ORDER BY s) AS names
    FROM public.news_summaries ns
    CROSS JOIN LATERAL unnest(ns.source_names) AS s
    WHERE ns.storyline_id IS NOT NULL AND ns.narrative_title IS NOT NULL
    GROUP BY 1, 2, 3
)
UPDATE public.storyline_entities se
   SET entry_count = a.entries,
       peak_impact = a.peak_impact::smallint,
       last_impact = l.impact,
       last_trajectory = l.trajectory,
       distinct_sources = COALESCE(array_length(sr.names, 1), 0),
       source_names = COALESCE(sr.names, '{}'::text[]),
       last_progressed_at = a.progressed_at
  FROM agg a
  JOIN lastc l USING (storyline_id, entity_type, entity_id)
  LEFT JOIN src sr USING (storyline_id, entity_type, entity_id)
 WHERE se.storyline_id = a.storyline_id
   AND se.entity_type = a.entity_type
   AND se.entity_id = a.entity_id;

WITH thread_cites AS (
    SELECT t.id AS thread_id, t.entity_type, t.entity_id, t.sport,
           sa.storyline_id, count(*) AS cites
    FROM public.narrative_threads t
    JOIN public.news_summaries ns ON ns.thread_id = t.id
    CROSS JOIN LATERAL unnest(ns.input_news_ids) AS aid(article_id)
    JOIN public.storyline_articles sa ON sa.article_id = aid.article_id
    WHERE t.authority = 'established' AND t.status = 'open'
    GROUP BY t.id, t.entity_type, t.entity_id, t.sport, sa.storyline_id
),
best AS (
    SELECT DISTINCT ON (thread_id) thread_id, entity_type, entity_id, sport, storyline_id
    FROM thread_cites
    ORDER BY thread_id, cites DESC, storyline_id
),
upd AS (
    UPDATE public.storyline_entities se
       SET authority = 'established'
      FROM best b
     WHERE se.storyline_id = b.storyline_id
       AND se.entity_type = b.entity_type
       AND se.entity_id = b.entity_id
       AND se.sport = b.sport
       AND se.authority = 'continuity'
    RETURNING 1
)
SELECT count(*) AS parts_inherited_established FROM upd;

-- ---------------------------------------------------------------------------
-- 5. The memory card, rebuilt on storylines. The three storyline CTEs of mig
--    218 (established / own_storyline / own_narrative) become ONE scan
--    (story_parts) with two renderings: established parts stay one-line
--    background facts; continuity parts render the progression block — or the
--    flat membership line when the Journalist has not told the story yet.
--    Every other CTE is byte-identical to mig 218.
-- ---------------------------------------------------------------------------

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
story_parts AS (
    -- (mig 219) The collapse of the thread lenses: one row per OPEN storyline
    -- this entity is an ACTIVE participant in (left_at IS NULL — D5: a part has
    -- its own lifespan, and an entity written out of a story stops remembering
    -- it), carrying the headline (latest packet's, falling back to the
    -- storyline's display title — packets are append-only snapshots, so the
    -- newest is the current state of the story), the membership report count,
    -- and the part's progression state (entries/sources/authority). One scan,
    -- two renderings below. Provenance-labeled continuity, NOT corroboration:
    -- it tells a voice which stories it is already inside, never that a claim
    -- is true. Membership counts are breadth, not measurement.
    SELECT se.storyline_id, se.role, se.joined_at, se.entry_count,
           se.distinct_sources, se.authority,
           COALESCE(se.last_progressed_at, s.last_seen_at) AS ord,
           COALESCE(NULLIF(p.headline, ''), NULLIF(s.title, ''), 'untitled') AS headline,
           m.n AS reports
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
),
established AS (
    -- (mig 183 lineage, rebuilt mig 219) ESTABLISHED parts: source growth past
    -- the authority gate. They graduate OUT of the "Our story so far" block and
    -- render as one-line BACKGROUND FACTS — settled context the model may speak
    -- from, deliberately carrying NO impact/likelihood figures (source count +
    -- opening date are breadth and tenure, not measurement). Open storylines
    -- only: a resolved story's confirmation already renders as Ground truth.
    SELECT format('Established story (our archive, %s sources, since %s): "%s".',
               sp.distinct_sources,
               to_char(sp.joined_at, 'Mon DD'),
               sp.headline) AS line,
            sp.ord
    FROM story_parts sp
    WHERE sp.authority = 'established'
    ORDER BY sp.ord DESC
    LIMIT 2
),
own_story AS (
    -- (mig 182/211 lineage, rebuilt mig 219) CONTINUITY parts as progression:
    -- a header — headline, joined date, totals — plus the last 3 chapters,
    -- newest-first, each tagged with its OWN cited source count. A part the
    -- Journalist has not told yet renders the flat membership line (the mig 211
    -- shape) so a freshly-opened story is still remembered. Chapters join on
    -- (storyline_id, entity) — one entity's part in one story.
    SELECT CASE WHEN steps.txt IS NULL THEN
               format('Our story so far ("%s", opened %s, %s report%s%s).',
                   sp.headline,
                   to_char(sp.joined_at, 'Mon DD'),
                   sp.reports, CASE WHEN sp.reports = 1 THEN '' ELSE 's' END,
                   CASE WHEN COALESCE(sp.role, '') <> ''
                        THEN format(', this entity''s part: %s', sp.role) ELSE '' END)
           ELSE
               format('Our story so far ("%s", opened %s, %s entr%s, %s source%s%s):%s',
                   sp.headline,
                   to_char(sp.joined_at, 'Mon DD'),
                   sp.entry_count, CASE WHEN sp.entry_count = 1 THEN 'y' ELSE 'ies' END,
                   sp.distinct_sources, CASE WHEN sp.distinct_sources = 1 THEN '' ELSE 's' END,
                   CASE WHEN COALESCE(sp.role, '') <> ''
                        THEN format(', this entity''s part: %s', sp.role) ELSE '' END,
                   steps.txt)
           END AS line,
           sp.ord
    FROM story_parts sp
    LEFT JOIN LATERAL (
        SELECT E'\n' || string_agg(
                   format('  %s (%s source%s): %s, coverage %s/100',
                       to_char(c.generated_at, 'Mon DD'),
                       c.source_count, CASE WHEN c.source_count = 1 THEN '' ELSE 's' END,
                       replace(c.trajectory, '_', ' '),
                       c.impact),
                   E'\n' ORDER BY c.generated_at DESC, c.id DESC) AS txt
        FROM (
            SELECT s.id, s.generated_at, s.source_count, s.trajectory, s.impact
            FROM news_summaries s
            WHERE s.storyline_id = sp.storyline_id
              AND s.entity_type = p_entity_type AND s.entity_id = p_entity_id
              AND s.body IS NOT NULL AND s.impact IS NOT NULL
            ORDER BY s.generated_at DESC, s.id DESC
            LIMIT 3
        ) c
    ) steps ON true
    WHERE sp.authority = 'continuity'
    ORDER BY sp.ord DESC
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
-- OUR OWN SELF-HISTORY (outputs-as-memories, mig 168 + Phase 6): four lenses, all
-- provenance-labeled continuity, NEVER corroboration. Source-tagged where the lens banks it.
-- ------------------------------------------------------------------------------
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
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_story),
    (SELECT string_agg(line, E'\n' ORDER BY mention_count DESC) FROM figures),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_transfer),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_vibe),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_momentum),
    (SELECT line FROM own_peak)), '');
$$;

COMMENT ON FUNCTION public.narrative_context_for_entity(p_sport text, p_entity_type text, p_entity_id integer) IS 'Per-entity memory card for junction prompts: sealed stories (both edge slots, outcome-labeled), open stories with likelihood (own-club employment excluded for players), recent ground-truth moves, the STORYLINE-PART block (mig 219: the narrative_threads collapse — per open storyline the entity actively participates in, an established part renders as a one-line background fact and a continuity part renders "Our story so far (...)" with its last 3 chapters, or the flat membership line when untold; headlines from the latest packet, membership counts as breadth, never measurement), active news-derived team figures (mig 166), and our own four-lens source-tagged self-history (mig 179): transfer (transfer_rumors, players), mood (vibe_scores), momentum (momentum_summaries), top skill (stat_summaries). Provenance-labeled — continuity, NOT corroboration; measurement (heat/likelihood/confirm/fizzle) stays raw/graph-anchored. NULL = no memory. Consumers: every voice, on both rails — memory is rail-independent. Model-facing only.';

-- ---------------------------------------------------------------------------
-- 6. Nightly lifecycle, rebuilt on storylines (cron-narrative-links.sh
--    switches to these; the thread functions go inert and die in step B).
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.storyline_part_established_gate(se public.storyline_entities, s public.storylines) RETURNS boolean
    LANGUAGE sql STABLE
    AS $$
    SELECT s.status = 'resolved'
        OR (    se.distinct_sources >= 5
            AND se.entry_count      >= 3
            AND se.joined_at        <= now() - interval '14 days');
$$;

COMMENT ON FUNCTION public.storyline_part_established_gate(public.storyline_entities, public.storylines) IS 'THE establishment gate, rebuilt on storyline parts (mig 219; successor to narrative_thread_established_gate, mig 183): >= 5 distinct sources AND >= 3 tellings AND >= 14 days since the part joined, OR the story resolved on ground truth. Change thresholds HERE only.';

CREATE OR REPLACE FUNCTION public.promote_established_parts(p_sport text) RETURNS integer
    LANGUAGE sql
    AS $$
WITH flipped AS (
    UPDATE public.storyline_entities se
       SET authority = 'established'
      FROM public.storylines s
     WHERE s.id = se.storyline_id
       AND se.sport = p_sport
       AND se.authority = 'continuity'
       AND public.storyline_part_established_gate(se, s)
     RETURNING se.storyline_id
)
SELECT count(*)::integer FROM flipped;
$$;

COMMENT ON FUNCTION public.promote_established_parts(p_sport text) IS 'Nightly authority promotion (mig 219; successor to promote_established_threads, mig 183): flips continuity parts that pass storyline_part_established_gate() to established. One-way (no demotion); returns the number of parts flipped. Cron runs it AFTER seal_storylines so a same-night ground-truth resolve promotes immediately.';

CREATE OR REPLACE FUNCTION public.seal_storylines(p_sport text) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_resolved integer := 0;
BEGIN
    -- Ground truth -> resolved (the thread seal's resolved arm, rebuilt on
    -- storylines). An OPEN storyline with a transfer-flavored member and an
    -- applied ground-truth move since it opened resolves: status flips, and D5
    -- happens in the same stroke — the move's player keeps the part, every
    -- other active edge closes as not_the_outcome. Transfer flavor reads
    -- routing_tags (the Editor-derived fact) with the legacy bucket as
    -- fallback for pre-flip articles. Dormancy (the thread seal's faded arm)
    -- is already covered by mark_dormant() in the worker: a 14d-quiet
    -- storyline leaves the candidate set AND the memory card (open-only).
    WITH hits AS (
        SELECT DISTINCT ON (s.id)
               s.id AS storyline_id, g.player_id, g.team_id, g.applied_at
        FROM public.storylines s
        JOIN public.storyline_articles sa ON sa.storyline_id = s.id
        JOIN public.news_articles a ON a.id = sa.article_id
        JOIN public.storyline_entities se
          ON se.storyline_id = s.id AND se.left_at IS NULL
        JOIN public.transfer_ground_truth g
          ON g.sport = s.sport
         AND g.applied_at > s.first_seen_at
         AND ((se.entity_type = 'player' AND g.player_id = se.entity_id)
           OR (se.entity_type = 'team' AND g.team_id = se.entity_id))
        WHERE s.sport = p_sport
          AND s.status = 'open'
          AND (a.bucket = 'transfer' OR a.routing_tags @> ARRAY['transfer'])
        ORDER BY s.id, g.applied_at DESC
    ),
    resolved AS (
        UPDATE public.storylines s
           SET status = 'resolved',
               resolved_at = h.applied_at,
               resolution = jsonb_build_object(
                   'outcome', 'transfer_confirmed',
                   'player_id', h.player_id,
                   'team_id', h.team_id,
                   'sealed_by', 'seal_storylines')
          FROM hits h
         WHERE s.id = h.storyline_id
         RETURNING s.id, h.player_id
    ),
    -- Data-modifying CTEs run exactly once and to completion, so the edge
    -- close lands in the same statement (and the same snapshot) as the
    -- resolve — one stroke, as D5 requires.
    closed AS (
        UPDATE public.storyline_entities se
           SET left_at = now(), exit_reason = 'not_the_outcome'
          FROM resolved r
         WHERE se.storyline_id = r.storyline_id
           AND se.left_at IS NULL
           AND NOT (se.entity_type = 'player' AND se.entity_id = r.player_id)
         RETURNING 1
    )
    SELECT count(*) INTO v_resolved FROM resolved;

    RETURN v_resolved;
END;
$$;

COMMENT ON FUNCTION public.seal_storylines(p_sport text) IS 'Nightly ground-truth resolve for storylines (mig 219; successor to seal_narrative_threads, mig 181): an open storyline with a transfer-flavored member and an applied ground-truth move since it opened resolves, and D5 closes every other active part in the same sweep (winner = the move''s player). Returns the number of storylines resolved. Fading needs no SQL: mark_dormant() (14d, worker) takes a quiet storyline out of the candidate set and the open-only memory card. Cron order: seal_storylines -> promote_established_parts.';

-- Smoke gate: every filled chapter points at a live storyline (the FK says so
-- too, but the backfill ran INSIDE this transaction — assert the derivation
-- produced sane rows rather than trusting the FK alone).
DO $$
DECLARE
    n_orphan int;
BEGIN
    SELECT count(*) INTO n_orphan
    FROM public.news_summaries ns
    WHERE ns.storyline_id IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM public.storylines s WHERE s.id = ns.storyline_id);
    IF n_orphan <> 0 THEN
        RAISE EXCEPTION '219 backfill produced % orphaned chapters', n_orphan;
    END IF;
END $$;

-- Self-record INSIDE the transaction so apply + record are atomic.
INSERT INTO public.schema_migrations(version) VALUES ('219_storyline_parts_collapse')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: update scripts/hosting/cron-narrative-links.sh (same commit),
-- deploy the Rust persist cutover, then step B drops narrative_threads.

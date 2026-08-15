-- 222 — the old-rail demolition: episodes leave the schema, memory reads the storylines
--
-- (Scott, 2026-08-15: "today we prune the old rail completely.")
--
-- The one-rail cutover (Phase 9, 2026-08-09) deleted the legacy loaders; migs 219/220
-- collapsed threads into storyline parts; 221 walked PEAK out of storage. What remained
-- was the co-mention EPISODE layer — the old rail's memory: narrative_episodes, its
-- nightly lifecycle (seal/roll/score in cron-narrative-links.sh), the likelihood scorer
-- and its view, a person-affiliation shadow table fed by trigger, and four dead support
-- tables. The voices' memory cards still read episodes for their history sections, which
-- is why this is a REPOINT-then-DROP, not a drop:
--
--   1. narrative_context_for_entity — the sealed/open episode sections become
--      prior_parts: resolved and dormant STORYLINES this entity had a part in
--      (resolution is the confirmed outcome; dormancy is the fizzle). Current-story
--      continuity already moved in mig 219 (own_story/established); ground truth,
--      figures, and the outputs-as-memories lenses are untouched.
--   2. narrative_context_for_pair — pair history becomes shared storyline membership
--      (player part + team part in one story); the live narrative_links trajectory
--      keeps coloring the current story. Banked transfer reads untouched.
--   3. assert_provenance_firewall — score_transfer_likelihood leaves its consumer
--      roster (retired here, not missing).
--   4. The drops: trigger, functions, view, tables. Function SIGNATURES are unchanged,
--      so the Rust callers (journalist/insider) do not move.
--
-- Dead-table verdicts (repo grep + pg_proc/pg_trigger/pg_views sweep, 2026-08-15):
--   narrative_episodes            — writer: cron lifecycle (dropped here); reader: the
--                                   two memory functions (repointed here)
--   narrative_person_affiliations — writer: trg_narrative_persons_affiliation only
--   source_tiers                  — reader: backfill_narrative_episodes only
--   provider_entity_map, season_recompute_needed, topic_heat_embeddings — zero
--                                   references in code, functions, triggers, or views

-- ---------------------------------------------------------------------------
-- 1. The entity memory card, re-sourced.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.narrative_context_for_entity(p_sport text, p_entity_type text, p_entity_id integer)
 RETURNS text
 LANGUAGE sql
 STABLE
AS $fn$
WITH prior_parts AS (
    -- (222) The old sealed/open episode sections, re-sourced from the rail that runs:
    -- resolved and dormant storylines this entity had a part in. Resolution carries the
    -- confirmed outcome; dormancy (14 quiet days) is the fizzle. Departed parts still
    -- remember — a story you were written out of is still a story you were in.
    SELECT format('Prior story: "%s" — %s (%s, %s report%s).',
               COALESCE(NULLIF(p.headline, ''), NULLIF(s.title, ''), 'untitled'),
               CASE WHEN s.status = 'resolved'
                    THEN 'RESOLVED' || COALESCE(': ' || replace(s.resolution->>'outcome', '_', ' '), '')
                    ELSE 'went quiet' END,
               CASE WHEN to_char(s.first_seen_at, 'Mon YYYY') = to_char(COALESCE(s.resolved_at, s.last_seen_at), 'Mon YYYY')
                    THEN to_char(s.first_seen_at, 'Mon YYYY')
                    ELSE to_char(s.first_seen_at, 'Mon YYYY') || ' - ' || to_char(COALESCE(s.resolved_at, s.last_seen_at), 'Mon YYYY')
               END,
               m.n, CASE WHEN m.n = 1 THEN '' ELSE 's' END) AS line,
           COALESCE(s.resolved_at, s.last_seen_at) AS ended_at
    FROM storyline_entities se
    JOIN storylines s ON s.id = se.storyline_id
    LEFT JOIN LATERAL (
        SELECT pk.headline FROM packets pk
        WHERE pk.storyline_id = s.id
        ORDER BY pk.compiled_at DESC, pk.id DESC LIMIT 1
    ) p ON true
    CROSS JOIN LATERAL (
        SELECT count(*) AS n FROM storyline_articles sa WHERE sa.storyline_id = s.id
    ) m
    WHERE se.sport = p_sport AND se.entity_type = p_entity_type AND se.entity_id = p_entity_id
      AND s.status IN ('resolved', 'dormant')
    ORDER BY COALESCE(s.resolved_at, s.last_seen_at) DESC
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
    -- (mig 219) One row per OPEN storyline this entity is an ACTIVE participant in
    -- (left_at IS NULL — D5: a part has its own lifespan), carrying the headline
    -- (latest packet's, falling back to the storyline's display title), the membership
    -- report count, and the part's progression state. Provenance-labeled continuity,
    -- NOT corroboration.
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
    -- (mig 183 lineage, rebuilt mig 219) ESTABLISHED parts render as one-line
    -- BACKGROUND FACTS — settled context, deliberately carrying NO impact/likelihood
    -- figures. Open storylines only: a resolved story renders under Prior story.
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
    -- (mig 182/211 lineage, rebuilt mig 219) CONTINUITY parts as progression: a header
    -- plus the last 3 chapters, newest-first, each tagged with its OWN cited source
    -- count. An untold part renders the flat membership line.
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
    -- Promoted (ACTIVE) news-derived people tied to this team (mig 166).
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
-- OUR OWN SELF-HISTORY (outputs-as-memories, mig 168 + Phase 6): provenance-labeled
-- continuity, NEVER corroboration. Source-tagged where the lens banks it.
-- ------------------------------------------------------------------------------
own_transfer AS (
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
own_rating AS (
    -- (mig 221) The rating lens's latest banked read. Least-weighted — the tail line.
    SELECT format('Our prior read (rating, season %s): profile distinctiveness %s/100%s.',
               s.season, s.notability,
               CASE WHEN COALESCE(s.rating_trajectory_label, '') <> ''
                    THEN '; ' || s.rating_trajectory_label ELSE '' END) AS line
    FROM stat_summaries s
    WHERE s.sport = p_sport AND s.entity_type = p_entity_type AND s.entity_id = p_entity_id
      AND s.body IS NOT NULL AND s.notability IS NOT NULL
    ORDER BY s.season DESC, s.generated_at DESC
    LIMIT 1
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM prior_parts),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM established),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_story),
    (SELECT string_agg(line, E'\n' ORDER BY mention_count DESC) FROM figures),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_transfer),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_vibe),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_momentum),
    (SELECT line FROM own_rating)), '');
$fn$;

-- ---------------------------------------------------------------------------
-- 2. The pair memory card, re-sourced: pair history is shared storyline membership.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.narrative_context_for_pair(p_sport text, p_player_id integer, p_team_id integer)
 RETURNS text
 LANGUAGE sql
 STABLE
AS $fn$
WITH pair_stories AS (
    -- (222) Stories BOTH parties had a part in — the storyline junction's own record of
    -- the flirtation. Departed parts count: history is history.
    SELECT s.id, s.status, s.resolution->>'outcome' AS outcome, s.first_seen_at, s.last_seen_at, s.resolved_at,
           COALESCE(NULLIF(p.headline, ''), NULLIF(s.title, ''), 'untitled') AS headline
    FROM storylines s
    JOIN storyline_entities spl ON spl.storyline_id = s.id AND spl.sport = p_sport
         AND spl.entity_type = 'player' AND spl.entity_id = p_player_id
    JOIN storyline_entities stm ON stm.storyline_id = s.id AND stm.sport = p_sport
         AND stm.entity_type = 'team' AND stm.entity_id = p_team_id
    LEFT JOIN LATERAL (
        SELECT pk.headline FROM packets pk
        WHERE pk.storyline_id = s.id
        ORDER BY pk.compiled_at DESC, pk.id DESC LIMIT 1
    ) p ON true
),
sealed AS (
    SELECT format('Prior story: "%s" — %s (%s).',
               headline,
               CASE WHEN status = 'resolved'
                    THEN 'RESOLVED' || COALESCE(': ' || replace(outcome, '_', ' '), '')
                    ELSE 'went quiet' END,
               CASE WHEN to_char(first_seen_at, 'Mon YYYY') = to_char(COALESCE(resolved_at, last_seen_at), 'Mon YYYY')
                    THEN to_char(first_seen_at, 'Mon YYYY')
                    ELSE to_char(first_seen_at, 'Mon YYYY') || ' - ' || to_char(COALESCE(resolved_at, last_seen_at), 'Mon YYYY')
               END) AS line,
           COALESCE(resolved_at, last_seen_at) AS ended_at
    FROM pair_stories
    WHERE status IN ('resolved', 'dormant')
    ORDER BY ended_at DESC
    LIMIT 3
),
open_ep AS (
    -- The live narrative_links co-mention edge still colors the current story's
    -- trajectory (heating up / cooling off) — links are current-rail, refreshed nightly.
    SELECT format('Current story: "%s" — tracked since %s%s.',
               ps.headline,
               to_char(ps.first_seen_at, 'Mon DD'),
               COALESCE(' (' || replace(l.trajectory, '_', ' ') || ')', '')) AS line
    FROM pair_stories ps
    LEFT JOIN narrative_links l
      ON l.sport = p_sport AND l.link_type = 'co_mention'
     AND l.subject_type = 'player' AND l.subject_id = p_player_id
     AND l.object_type = 'team' AND l.object_id = p_team_id
    WHERE ps.status = 'open'
    ORDER BY ps.last_seen_at DESC
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
$fn$;

-- ---------------------------------------------------------------------------
-- 3. The provenance firewall's consumer roster: the likelihood scorer is RETIRED,
--    not missing.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.assert_provenance_firewall()
 RETURNS void
 LANGUAGE plpgsql
AS $fn$
DECLARE
    -- The measurement-side readers of narrative_events. Each MUST filter origin='extraction'
    -- so junction-authored events (mig 170) stay invisible to the numeric feedback loop.
    -- (222: score_transfer_likelihood retired with the old rail's episode layer.)
    v_consumers text[] := ARRAY['refresh_typed_links'];
    v_name  text;
    v_body  text;
    v_norm  text;
    v_missing text[] := ARRAY[]::text[];
    v_absent  text[] := ARRAY[]::text[];
BEGIN
    FOREACH v_name IN ARRAY v_consumers LOOP
        SELECT string_agg(pg_get_functiondef(p.oid), E'\n')
          INTO v_body
          FROM pg_proc p
          JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'public' AND p.proname = v_name;

        IF v_body IS NULL THEN
            v_absent := array_append(v_absent, v_name);
            CONTINUE;
        END IF;

        v_norm := regexp_replace(lower(v_body), '\s+', '', 'g');
        IF position('narrative_events' IN v_norm) > 0
           AND position('origin=''extraction''' IN v_norm) = 0 THEN
            v_missing := array_append(v_missing, v_name);
        END IF;
    END LOOP;

    IF array_length(v_absent, 1) > 0 THEN
        RAISE EXCEPTION 'provenance firewall: measurement consumer(s) not found in public schema: %',
            array_to_string(v_absent, ', ')
            USING HINT = 'assert_provenance_firewall() names a function that no longer exists; update v_consumers if it was intentionally removed/renamed.';
    END IF;

    IF array_length(v_missing, 1) > 0 THEN
        RAISE EXCEPTION 'provenance firewall breached: %() read narrative_events without an origin=''extraction'' filter',
            array_to_string(v_missing, ', ')
            USING HINT = 'A junction-authored event (origin=''junction'', mig 170) could re-enter the numeric loop. Re-add the origin=''extraction'' filter to the consumer''s narrative_events scan.';
    END IF;
END;
$fn$;

-- ---------------------------------------------------------------------------
-- 4. The demolition. Order: trigger before its function, view before its table.
-- ---------------------------------------------------------------------------
DROP TRIGGER IF EXISTS trg_narrative_persons_affiliation ON public.narrative_persons;
DROP FUNCTION IF EXISTS public.narrative_persons_track_affiliation();

DROP VIEW IF EXISTS public.v_transfer_likelihood;

DROP FUNCTION IF EXISTS public.roll_narrative_episodes(text, integer, integer, integer);
DROP FUNCTION IF EXISTS public.seal_confirmed_episodes(text, integer, integer);
DROP FUNCTION IF EXISTS public.score_transfer_likelihood(text, integer, integer, integer, numeric);
DROP FUNCTION IF EXISTS public.backfill_narrative_episodes(text, date, date, integer, integer, integer, integer);

DROP TABLE IF EXISTS public.narrative_episodes;
DROP TABLE IF EXISTS public.narrative_person_affiliations;
DROP TABLE IF EXISTS public.source_tiers;
DROP TABLE IF EXISTS public.provider_entity_map;
DROP TABLE IF EXISTS public.season_recompute_needed;
DROP TABLE IF EXISTS public.topic_heat_embeddings;

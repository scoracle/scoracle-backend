-- 166_person_promotion.sql
--
-- Junction memory rollout step 7: person promotion — candidate → active on
-- accumulated multi-source, multi-day evidence (never on one extraction; the mig-154
-- lifecycle contract). Promoted persons become servable graph context: the per-entity
-- memory card (mig 163) gains "Team figure:" lines for teams, so junction prompts can
-- say "head coach: Thomas Frank" from news-derived entityhood the provider never seeds.
--
-- (1) promote_narrative_persons(sport) — the nightly promotion pass (appended to the
--     00:45 cron chain). v1 thresholds, tunable by migration with eval evidence:
--       distinct_sources >= 2  AND  mention_count >= 3
--       AND evidence spans >= 2 days (last_seen - first_seen)
--     Team-vote consistency is deferred: mentions do not yet carry per-article team
--     context (the accumulate path records the FIRST team vote on the person row);
--     revisit when the graph stage's evidence JSONB grows per-mention votes.
--
-- (2) narrative_context_for_entity() v2 — teams gain up to 3 "Team figure:" lines for
--     ACTIVE persons tied to the team. Provenance class: news-derived accumulation
--     (model-extracted, evidence-gated) — graph-derived like "Prior story:", never
--     ground truth, never corroboration.
--
-- ADDITIVE — apply any time; the card change serves on the next junction render.

BEGIN;

CREATE OR REPLACE FUNCTION public.promote_narrative_persons(p_sport text)
RETURNS integer
    LANGUAGE sql
    AS $$
WITH promoted AS (
    UPDATE narrative_persons p
       SET status = 'active', updated_at = NOW()
     WHERE p.sport = p_sport
       AND p.status = 'candidate'
       AND p.merged_into IS NULL
       AND p.distinct_sources >= 2
       AND p.mention_count >= 3
       AND p.last_seen_at - p.first_seen_at >= interval '2 days'
    RETURNING p.id
)
SELECT count(*)::integer FROM promoted;
$$;

COMMENT ON FUNCTION public.promote_narrative_persons(text) IS
    'Nightly candidate->active promotion on accumulated evidence (>=2 sources, >=3 '
    'mentions, >=2 days span). Entityhood is earned, never granted by one extraction.';

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
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM sealed),
    (SELECT string_agg(line, E'\n' ORDER BY rank DESC) FROM open_eps),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves),
    (SELECT string_agg(line, E'\n' ORDER BY mention_count DESC) FROM figures)), '');
$$;

COMMENT ON FUNCTION public.narrative_context_for_entity(text, text, integer) IS
    'Per-entity memory card for junction prompts: sealed stories (both edge slots, '
    'outcome-labeled), open stories with likelihood (own-club employment excluded for '
    'players), recent ground-truth moves, and (teams, since mig 166) active news-derived '
    'team figures. Provenance-labeled lines — continuity, not corroboration. NULL = no '
    'memory. Consumers: narratives n8, vibe v12, sigil s15, momentum s5. '
    'Model-facing only.';

INSERT INTO public.schema_migrations(version) VALUES ('166_person_promotion')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

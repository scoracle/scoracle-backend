-- 164_stat_context_for_entity.sql
--
-- Junction memory rollout step 4 (Progressive Refinement Dataflow, folded plan):
-- stat_context_for_entity() — the STATS-side memory card, the mig 163 counterpart for
-- the PEAK/statcommentary junction. Kills the season cold-start: at a season boundary
-- the fresh datapoints are thin, but the junction now remembers who the entity WAS.
-- In-season recency windows are untouched — they do a different job (the agreed
-- refinement: memory kills cold starts, not recency).
--
-- Renders, as provenance-labeled prompt lines:
--
--   Our prior read:  the most recent PRIOR-season PEAK label + notability from
--                    stat_summaries — a BANKED JUNCTION OUTPUT, so the echo-chamber
--                    rule applies (continuity, never corroboration). First live use
--                    of the "Our prior read:" provenance class.
--   Ground truth:    confirmed moves in the last 180 days (players: their arrival;
--                    teams: signings) from transfer_ground_truth — the regime-change
--                    context that explains a stat profile shifting under a new club.
--   Matchup memory:  top player-vs-team edges from stat_matchups, ranked by
--                    reliability-weighted effect, each line carrying n + reliability
--                    (presented, not gatekept: the model gets the magnitude AND the
--                    reason to hedge — the "grain of salt" contract).
--
-- NULL when the graph holds no memory — the prompt renders no section. Consumed by
-- rating s12 (build_stat_prompt). Same deliberate decision as migs 162/163: NOT part
-- of any input_hash — memory rides along on material change, it never self-triggers.
--
-- Deploy order: ADDITIVE — apply BEFORE deploying the s12 cognition/statcommentary
-- binaries.

BEGIN;

CREATE OR REPLACE FUNCTION public.stat_context_for_entity(
    p_sport text,
    p_entity_type text,
    p_entity_id integer,
    p_season integer
) RETURNS text
    LANGUAGE sql STABLE
    AS $$
WITH prior_read AS (
    SELECT format('Our prior read: season %s PEAK was "%s" (notability %s/100)%s.',
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
    'Stats-side memory card for the PEAK/statcommentary junction (rating s12): '
    'prior-season PEAK read (Our prior read: — banked output, echo-chamber rule), '
    'confirmed moves 180d (Ground truth:), and reliability-framed matchup edges '
    '(Matchup memory: — presented, not gatekept). NULL = no memory. Kills the season '
    'cold-start; in-season recency windows survive. Model-facing only.';

INSERT INTO public.schema_migrations(version) VALUES ('164_stat_context_for_entity')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

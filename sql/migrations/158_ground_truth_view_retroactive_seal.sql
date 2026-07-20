-- 158_ground_truth_view_retroactive_seal.sql
--
-- Two gaps exposed by the Cucurella + Garrett confirmations (2026-07-19):
--
--   1. Ground truth lived in ONE ledger. seal_confirmed_episodes/source_performance read
--      only transfer_identity_applications — but the identity system has a second,
--      operator-facing ledger: player_current_identity_overrides (source='manual'), the
--      designed path for "the operator knows the move happened". Cucurella is the
--      motivating case: his application (id 4, heat 85/conf 0.85, applied 07-04) was
--      BULK-REVERTED during a workflow rerun and never re-applied, so the pipeline
--      ledger says nothing while the move is real. New view `transfer_ground_truth`
--      unions both ledgers (unreverted only, earliest event per player+team) and both
--      consumers read it — one home for "what counts as a confirmed move".
--
--   2. No episode ⇒ no confirmed memory. The graph was born 2026-07-19; stories that
--      peaked before birth (Cucurella↔Real Madrid: 28 arts/22 sources, June 14-29) or
--      under the open threshold have no episode row to seal. New retroactive path: when
--      ground truth exists, the pair has real link coverage (article floor), and no
--      episode exists, mint a SEALED 'confirmed' episode from link state — started_at
--      from first coverage, peak_strength = current link strength flagged as a LOWER
--      BOUND in evidence (the true peak decayed before we looked; the as-of backfill,
--      roadmap item 5, can later correct it). Honest memory beats no memory.
--
-- seal_confirmed_episodes gains an OUT column (episodes_retroactive) → return type
-- changes → DROP + CREATE (cron reads SELECT *, unaffected).
--
-- Deploy order: ADDITIVE — apply before the next cron tick.

BEGIN;

-- ── One home for confirmed-move ground truth ─────────────────────────────────────────

CREATE OR REPLACE VIEW public.transfer_ground_truth AS
SELECT DISTINCT ON (sport, player_id, team_id)
       sport, player_id, team_id, applied_at, ledger, ref_id
FROM (
    SELECT a.sport, a.player_id, a.new_team_id AS team_id, a.applied_at,
           'application'::text AS ledger, a.id AS ref_id
    FROM public.transfer_identity_applications a
    WHERE a.applied_at IS NOT NULL AND a.reverted_at IS NULL
    UNION ALL
    SELECT o.sport, o.player_id, o.team_id, o.applied_at,
           'override'::text AS ledger, o.id AS ref_id
    FROM public.player_current_identity_overrides o
    WHERE o.reverted_at IS NULL AND o.team_id IS NOT NULL
) u
ORDER BY sport, player_id, team_id, applied_at ASC;

COMMENT ON VIEW public.transfer_ground_truth IS
    'Applied, unreverted identity moves from BOTH ledgers (pipeline applications + '
    'manual operator overrides), earliest event per (player, team). The single source '
    'consumed by seal_confirmed_episodes and refresh_source_performance: to add a move '
    'to the confirmed list, write the appropriate ledger (manual override for '
    'operator-known facts) — never episode rows directly.';

-- ── seal v2: both ledgers + retroactive episodes ─────────────────────────────────────

DROP FUNCTION IF EXISTS public.seal_confirmed_episodes(text, integer);

CREATE FUNCTION public.seal_confirmed_episodes(
    p_sport text,
    p_lag_days integer DEFAULT 60,
    p_retro_article_floor integer DEFAULT 3,
    OUT episodes_confirmed integer,
    OUT episodes_upgraded integer,
    OUT episodes_reverted integer,
    OUT episodes_retroactive integer
) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
    v_rev2 integer;
BEGIN
    -- a. Seal open episodes whose pair has an applied move.
    UPDATE narrative_episodes e
    SET status = 'sealed',
        outcome = 'confirmed',
        ended_at = g.applied_at,
        evidence = e.evidence || jsonb_build_object('confirmed', jsonb_build_object(
            'ledger', g.ledger, 'ref_id', g.ref_id, 'applied_at', g.applied_at)),
        updated_at = v_run
    FROM transfer_ground_truth g
    WHERE e.sport = p_sport AND e.status = 'open'
      AND g.sport = e.sport
      AND e.subject_type = 'player' AND e.subject_id = g.player_id
      AND e.object_type = 'team' AND e.object_id = g.team_id
      AND e.started_at <= g.applied_at;

    GET DIAGNOSTICS episodes_confirmed = ROW_COUNT;

    -- b. Announcement lag: upgrade quiet-sealed 'fizzled' when the move applied
    --    within lag_days after coverage ended.
    UPDATE narrative_episodes e
    SET outcome = 'confirmed',
        evidence = e.evidence || jsonb_build_object('confirmed', jsonb_build_object(
            'ledger', g.ledger, 'ref_id', g.ref_id, 'applied_at', g.applied_at,
            'upgraded_from', 'fizzled')),
        updated_at = v_run
    FROM transfer_ground_truth g
    WHERE e.sport = p_sport AND e.status = 'sealed' AND e.outcome = 'fizzled'
      AND e.evidence->'confirmed' IS NULL
      AND g.sport = e.sport
      AND e.subject_type = 'player' AND e.subject_id = g.player_id
      AND e.object_type = 'team' AND e.object_id = g.team_id
      AND g.applied_at >= e.ended_at
      AND g.applied_at <= e.ended_at + make_interval(days => p_lag_days);

    GET DIAGNOSTICS episodes_upgraded = ROW_COUNT;

    -- c. Revert stream, both ledgers (also matches mig-157-era 'application_id' keys).
    UPDATE narrative_episodes e
    SET outcome = 'fizzled',
        evidence = e.evidence || jsonb_build_object('confirmation_reverted', jsonb_build_object(
            'reverted_at', a.reverted_at, 'noticed_at', v_run)),
        updated_at = v_run
    FROM transfer_identity_applications a
    WHERE e.sport = p_sport AND e.outcome = 'confirmed'
      AND e.evidence->'confirmation_reverted' IS NULL
      AND COALESCE(e.evidence->'confirmed'->>'ledger', 'application') = 'application'
      AND COALESCE((e.evidence->'confirmed'->>'ref_id')::bigint,
                   (e.evidence->'confirmed'->>'application_id')::bigint) = a.id
      AND a.reverted_at IS NOT NULL;

    GET DIAGNOSTICS episodes_reverted = ROW_COUNT;

    UPDATE narrative_episodes e
    SET outcome = 'fizzled',
        evidence = e.evidence || jsonb_build_object('confirmation_reverted', jsonb_build_object(
            'reverted_at', o.reverted_at, 'noticed_at', v_run)),
        updated_at = v_run
    FROM player_current_identity_overrides o
    WHERE e.sport = p_sport AND e.outcome = 'confirmed'
      AND e.evidence->'confirmation_reverted' IS NULL
      AND e.evidence->'confirmed'->>'ledger' = 'override'
      AND (e.evidence->'confirmed'->>'ref_id')::bigint = o.id
      AND o.reverted_at IS NOT NULL;

    GET DIAGNOSTICS v_rev2 = ROW_COUNT;
    episodes_reverted := episodes_reverted + v_rev2;

    -- d. Retroactive memory: ground truth + real link coverage, but the episode never
    --    existed (graph younger than the story, or coverage under the open threshold).
    --    peak_strength is the CURRENT decayed link strength — a lower bound, flagged.
    INSERT INTO narrative_episodes
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         status, outcome, season, started_at, peaked_at, ended_at,
         peak_strength, last_strength, article_count, distinct_sources, event_count,
         peak_components, evidence, created_at, updated_at)
    SELECT g.sport, 'co_mention', 'player', g.player_id, 'team', g.team_id,
           'sealed', 'confirmed', s.current_season,
           l.first_seen_at,
           COALESCE(l.last_event_at, g.applied_at),
           GREATEST(g.applied_at, l.first_seen_at),
           COALESCE(l.strength, 0), COALESCE(l.strength, 0),
           l.article_count, l.distinct_sources, l.event_count,
           l.components,
           jsonb_build_object(
               'confirmed', jsonb_build_object(
                   'ledger', g.ledger, 'ref_id', g.ref_id, 'applied_at', g.applied_at),
               'retroactive', jsonb_build_object(
                   'minted_at', v_run,
                   'peak_is_lower_bound', true,
                   'reason', 'story predates the graph or never crossed the open threshold')),
           v_run, v_run
    FROM transfer_ground_truth g
    JOIN sports s ON s.id = g.sport
    JOIN narrative_links l
      ON l.sport = g.sport AND l.link_type = 'co_mention'
     AND l.subject_type = 'player' AND l.subject_id = g.player_id
     AND l.object_type = 'team' AND l.object_id = g.team_id
    WHERE g.sport = p_sport
      AND l.article_count >= p_retro_article_floor
      AND l.first_seen_at IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM narrative_episodes e
          WHERE e.sport = g.sport AND e.link_type = 'co_mention'
            AND e.subject_type = 'player' AND e.subject_id = g.player_id
            AND e.object_type = 'team' AND e.object_id = g.team_id);

    GET DIAGNOSTICS episodes_retroactive = ROW_COUNT;
END;
$$;

COMMENT ON FUNCTION public.seal_confirmed_episodes(text, integer, integer) IS
    'Episode outcomes from transfer_ground_truth (both ledgers): seal open episodes, '
    'upgrade announcement-lag fizzles, downgrade reverts, and retroactively mint sealed '
    'confirmed episodes for moves whose coverage predates the graph (peak = lower '
    'bound, flagged in evidence). Run BEFORE roll_narrative_episodes.';

-- ── source_performance v2: outcomes from the unified view ───────────────────────────

CREATE OR REPLACE FUNCTION public.refresh_source_performance(
    p_sport text,
    p_k numeric DEFAULT 5,
    OUT sources_written integer
) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    DELETE FROM source_performance WHERE sport = p_sport;

    WITH apps AS (
        SELECT player_id, team_id, applied_at
        FROM transfer_ground_truth WHERE sport = p_sport
    ),
    attributions AS (
        SELECT r.player_id, r.team_id, s.source, min(r.generated_at) AS first_reported
        FROM transfer_rumors r
        CROSS JOIN LATERAL unnest(r.source_names) AS s(source)
        WHERE r.sport = p_sport
        GROUP BY 1, 2, 3
    ),
    pair_advanced AS (
        SELECT player_id, team_id, min(generated_at) AS first_advanced
        FROM transfer_rumors
        WHERE sport = p_sport AND stage IN ('advanced_talks', 'here_we_go')
        GROUP BY 1, 2
    ),
    joined AS (
        SELECT at.source, at.first_reported, pa.first_advanced, ap.applied_at
        FROM attributions at
        LEFT JOIN pair_advanced pa
               ON pa.player_id = at.player_id AND pa.team_id = at.team_id
        LEFT JOIN apps ap
               ON ap.player_id = at.player_id AND ap.team_id = at.team_id
    ),
    agg AS (
        SELECT source,
               count(*) AS pairs_covered,
               count(applied_at) AS confirmed_covered,
               count(*) FILTER (
                   WHERE applied_at IS NOT NULL
                     AND (first_advanced IS NULL OR first_reported < first_advanced)
               ) AS early_confirmed,
               round(avg(EXTRACT(EPOCH FROM (applied_at - first_reported)) / 86400.0)
                     FILTER (WHERE applied_at IS NOT NULL)::numeric, 1) AS avg_lead_days,
               round(max(EXTRACT(EPOCH FROM (applied_at - first_reported)) / 86400.0)
                     FILTER (WHERE applied_at IS NOT NULL)::numeric, 1) AS best_lead_days
        FROM joined
        GROUP BY source
    )
    INSERT INTO source_performance
        (sport, source, pairs_covered, confirmed_covered, early_confirmed,
         avg_lead_days, best_lead_days, reliability, components, computed_at)
    SELECT p_sport, a.source, a.pairs_covered, a.confirmed_covered, a.early_confirmed,
           a.avg_lead_days, a.best_lead_days,
           round(100 * a.confirmed_covered / (a.confirmed_covered + p_k))::smallint,
           jsonb_build_object(
               'k', p_k,
               'confirm_rate', CASE WHEN a.pairs_covered = 0 THEN NULL
                    ELSE round(a.confirmed_covered::numeric / a.pairs_covered, 3) END,
               'note', 'outcomes from transfer_ground_truth (both ledgers); lead days '
                       'signed (negative = pile-on after the move)'),
           v_run
    FROM agg a;

    GET DIAGNOSTICS sources_written = ROW_COUNT;
END;
$$;

INSERT INTO public.schema_migrations(version) VALUES ('158_ground_truth_view_retroactive_seal')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

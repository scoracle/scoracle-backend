-- 159_roll_own_team_exclusion.sql
--
-- Fixes the post-confirmation reopen loop found live (2026-07-19, Schlager/LeVert
-- duplicated on the confirmed list): after a transfer completes the pair stays hot as
-- an OWN-TEAM edge, so roll_narrative_episodes re-opened an episode the night after
-- seal_confirmed_episodes sealed one, and the next seal confirmed the duplicate —
-- forever. Two guards:
--
--   1. roll v2: the open path skips player-team co_mention pairs where the team is the
--      player's CURRENT team (player_current_identity — which both ledgers feed). An
--      own-team association is employment coverage, not a story; this also stops
--      minting noise episodes for every star vs their own club (Rogers↔Villa etc.).
--      Typed links (trade_rumor, contract_dispute) will legitimately cover own-team
--      sagas later. Existing open own-team episodes are left to quiet-seal naturally.
--
--   2. seal path-a ref guard: never confirm a second episode for the SAME ground-truth
--      event (same ledger+ref). A genuinely new move for the same pair years later has
--      a new ref and still earns its own confirmed episode.
--
-- Return types unchanged → CREATE OR REPLACE for both.
--
-- Deploy order: apply BEFORE the affected environment's first narrative cron tick if
-- possible; locally the 2 duplicate rows minted tonight are removed operationally
-- (they exist only in the local DB — no other environment has run these crons).

BEGIN;

CREATE OR REPLACE FUNCTION public.roll_narrative_episodes(
    p_sport text,
    p_open_threshold integer DEFAULT 40,
    p_close_threshold integer DEFAULT 15,
    p_quiet_days integer DEFAULT 14,
    OUT episodes_opened integer,
    OUT episodes_updated integer,
    OUT episodes_sealed integer
) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    -- Open: a link crossing the notability threshold with no open episode starts one —
    -- EXCEPT a player's own current team (employment coverage, not a story; and the
    -- post-transfer reopen loop, mig 159). started_at prefers the current window's
    -- oldest article (components.window_oldest) over all-time first_seen_at.
    INSERT INTO narrative_episodes
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         status, season, started_at, peaked_at, peak_strength, last_strength,
         article_count, distinct_sources, event_count, peak_components,
         created_at, updated_at)
    SELECT l.sport, l.link_type, l.subject_type, l.subject_id,
           l.object_type, l.object_id,
           'open', s.current_season,
           COALESCE((l.components->>'window_oldest')::timestamptz,
                    l.first_seen_at, v_run),
           v_run, l.strength, l.strength,
           l.article_count, l.distinct_sources, l.event_count, l.components,
           v_run, v_run
    FROM narrative_links l
    JOIN sports s ON s.id = l.sport
    WHERE l.sport = p_sport
      AND l.strength >= p_open_threshold
      AND NOT (l.link_type = 'co_mention'
               AND l.subject_type = 'player' AND l.object_type = 'team'
               AND EXISTS (
                   SELECT 1 FROM player_current_identity pci
                   WHERE pci.sport = l.sport
                     AND pci.player_id = l.subject_id
                     AND pci.team_id = l.object_id))
    ON CONFLICT (sport, link_type, subject_type, subject_id, object_type, object_id)
        WHERE status = 'open'
    DO NOTHING;

    GET DIAGNOSTICS episodes_opened = ROW_COUNT;

    -- Freshen every open episode from its link (unchanged from mig 155).
    UPDATE narrative_episodes e
    SET last_strength = l.strength,
        article_count = GREATEST(e.article_count, l.article_count),
        distinct_sources = GREATEST(e.distinct_sources, l.distinct_sources),
        event_count = GREATEST(e.event_count, l.event_count),
        peak_strength = GREATEST(e.peak_strength, l.strength),
        peaked_at = CASE WHEN l.strength > e.peak_strength THEN v_run
                         ELSE e.peaked_at END,
        peak_components = CASE WHEN l.strength > e.peak_strength THEN l.components
                               ELSE e.peak_components END,
        updated_at = v_run
    FROM narrative_links l
    WHERE e.sport = p_sport AND e.status = 'open'
      AND l.sport = e.sport AND l.link_type = e.link_type
      AND l.subject_type = e.subject_type AND l.subject_id = e.subject_id
      AND l.object_type = e.object_type AND l.object_id = e.object_id;

    GET DIAGNOSTICS episodes_updated = ROW_COUNT;

    -- Seal: weak AND quiet (unchanged from mig 155).
    UPDATE narrative_episodes e
    SET status = 'sealed',
        outcome = 'fizzled',
        ended_at = COALESCE(l.last_event_at, e.updated_at),
        last_strength = l.strength,
        evidence = e.evidence || jsonb_build_object('sealed', jsonb_build_object(
            'at', v_run,
            'reason', 'quiet',
            'final_strength', l.strength,
            'close_threshold', p_close_threshold,
            'quiet_days', p_quiet_days)),
        updated_at = v_run
    FROM narrative_links l
    WHERE e.sport = p_sport AND e.status = 'open'
      AND l.sport = e.sport AND l.link_type = e.link_type
      AND l.subject_type = e.subject_type AND l.subject_id = e.subject_id
      AND l.object_type = e.object_type AND l.object_id = e.object_id
      AND l.strength < p_close_threshold
      AND (l.last_event_at IS NULL
           OR l.last_event_at < now() - make_interval(days => p_quiet_days));

    GET DIAGNOSTICS episodes_sealed = ROW_COUNT;
END;
$$;

-- Path-a ref guard: recreate seal v2 body with the same signature, adding the
-- same-ground-truth-event idempotence check.
CREATE OR REPLACE FUNCTION public.seal_confirmed_episodes(
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
      AND e.started_at <= g.applied_at
      -- Same ground-truth event never confirms twice (mig 159); a future NEW move for
      -- the pair has a new ref and earns its own episode.
      AND NOT EXISTS (
          SELECT 1 FROM narrative_episodes e2
          WHERE e2.sport = e.sport AND e2.link_type = e.link_type
            AND e2.subject_type = e.subject_type AND e2.subject_id = e.subject_id
            AND e2.object_type = e.object_type AND e2.object_id = e.object_id
            AND e2.outcome = 'confirmed'
            AND e2.evidence->'confirmed'->>'ledger' = g.ledger
            AND (e2.evidence->'confirmed'->>'ref_id')::bigint = g.ref_id);

    GET DIAGNOSTICS episodes_confirmed = ROW_COUNT;

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
      AND g.applied_at <= e.ended_at + make_interval(days => p_lag_days)
      AND NOT EXISTS (
          SELECT 1 FROM narrative_episodes e2
          WHERE e2.sport = e.sport AND e2.link_type = e.link_type
            AND e2.subject_type = e.subject_type AND e2.subject_id = e.subject_id
            AND e2.object_type = e.object_type AND e2.object_id = e.object_id
            AND e2.outcome = 'confirmed'
            AND e2.evidence->'confirmed'->>'ledger' = g.ledger
            AND (e2.evidence->'confirmed'->>'ref_id')::bigint = g.ref_id);

    GET DIAGNOSTICS episodes_upgraded = ROW_COUNT;

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

INSERT INTO public.schema_migrations(version) VALUES ('159_roll_own_team_exclusion')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
-- Local-only data fix (documented in the migration header): delete the two duplicate
-- confirmed episodes minted by the pre-fix reopen loop, keeping each pair's original.

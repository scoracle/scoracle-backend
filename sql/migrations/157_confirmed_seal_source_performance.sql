-- 157_confirmed_seal_source_performance.sql
--
-- Transfer Likelihood roadmap items 1+2 (Plan - Narrative Graph, Rogers test case
-- 2026-07-19). Two mechanical additions, no model calls:
--
--   1. seal_confirmed_episodes() — episodes gain the outcome path the quiet-seal cannot
--      provide. The Rogers test proved a completed transfer converts the pair into an
--      own-team edge that never goes quiet, so `confirmed` MUST come from ground truth:
--      transfer_identity_applications rows that were genuinely applied (applied_at set,
--      not reverted). Three sub-paths, all set-based:
--        a. seal open episodes whose (player, new_team) got an applied move;
--        b. UPGRADE recently quiet-sealed 'fizzled' episodes when the application lands
--           ≤60 days after coverage ended (announcement lag is real: agreed deals go
--           quiet before they are announced);
--        c. DOWNGRADE confirmed episodes whose application was later reverted (4 of the
--           first 6 applies were reverted — reverts are not rare). Calibration labels
--           must track the revert stream or the eval lies.
--
--   2. source_performance — the virtuous cycle's first artifact: per (sport, source),
--      the measured predictive record from transfer_rumors attribution × applied
--      outcomes. Sky Sports' 5-weeks-early Rogers call is the archetype. Cold start is
--      thin (2 applied moves today; 14 manual-review pending) — presented, not
--      gatekept applies to OUR OWN measurements: every row carries an n-based
--      reliability instead of being suppressed. NOT yet wired into scrub/heat/vibe
--      weighting — that is a later, eval-gated step (the table is measurement, the
--      formula change needs evidence).
--
-- Deploy order: ADDITIVE — apply any time; cron picks the new calls up on next tick.

BEGIN;

-- ── 1. Confirmed sealing from roster ground truth ────────────────────────────────────

CREATE OR REPLACE FUNCTION public.seal_confirmed_episodes(
    p_sport text,
    p_lag_days integer DEFAULT 60,
    OUT episodes_confirmed integer,
    OUT episodes_upgraded integer,
    OUT episodes_reverted integer
) RETURNS record
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    -- Genuinely applied, unreverted moves only: earliest application per (player, team).
    -- (failed_closed / manual_review / rejected rows are NOT ground truth.)
    WITH apps AS (
        SELECT DISTINCT ON (a.player_id, a.new_team_id)
               a.id, a.player_id, a.new_team_id, a.old_team_id, a.applied_at
        FROM transfer_identity_applications a
        WHERE a.sport = p_sport
          AND a.applied_at IS NOT NULL
          AND a.reverted_at IS NULL
        ORDER BY a.player_id, a.new_team_id, a.applied_at ASC
    )
    UPDATE narrative_episodes e
    SET status = 'sealed',
        outcome = 'confirmed',
        ended_at = ap.applied_at,
        evidence = e.evidence || jsonb_build_object('confirmed', jsonb_build_object(
            'application_id', ap.id,
            'applied_at', ap.applied_at,
            'old_team_id', ap.old_team_id,
            'new_team_id', ap.new_team_id)),
        updated_at = v_run
    FROM apps ap
    WHERE e.sport = p_sport AND e.status = 'open'
      AND e.subject_type = 'player' AND e.subject_id = ap.player_id
      AND e.object_type = 'team' AND e.object_id = ap.new_team_id
      AND e.started_at <= ap.applied_at;

    GET DIAGNOSTICS episodes_confirmed = ROW_COUNT;

    -- Announcement lag: coverage went quiet (quiet-seal fired 'fizzled'), THEN the move
    -- applied. Upgrade the label; ended_at stays at coverage end (it is true).
    WITH apps AS (
        SELECT DISTINCT ON (a.player_id, a.new_team_id)
               a.id, a.player_id, a.new_team_id, a.old_team_id, a.applied_at
        FROM transfer_identity_applications a
        WHERE a.sport = p_sport
          AND a.applied_at IS NOT NULL
          AND a.reverted_at IS NULL
        ORDER BY a.player_id, a.new_team_id, a.applied_at ASC
    )
    UPDATE narrative_episodes e
    SET outcome = 'confirmed',
        evidence = e.evidence || jsonb_build_object('confirmed', jsonb_build_object(
            'application_id', ap.id,
            'applied_at', ap.applied_at,
            'old_team_id', ap.old_team_id,
            'new_team_id', ap.new_team_id,
            'upgraded_from', 'fizzled')),
        updated_at = v_run
    FROM apps ap
    WHERE e.sport = p_sport AND e.status = 'sealed' AND e.outcome = 'fizzled'
      AND e.evidence->'confirmed' IS NULL
      AND e.subject_type = 'player' AND e.subject_id = ap.player_id
      AND e.object_type = 'team' AND e.object_id = ap.new_team_id
      AND ap.applied_at >= e.ended_at
      AND ap.applied_at <= e.ended_at + make_interval(days => p_lag_days);

    GET DIAGNOSTICS episodes_upgraded = ROW_COUNT;

    -- Revert stream: an application this function once consumed was rolled back.
    -- The episode's confirmation claim must follow, or downstream calibration lies.
    UPDATE narrative_episodes e
    SET outcome = 'fizzled',
        evidence = e.evidence || jsonb_build_object('confirmation_reverted', jsonb_build_object(
            'application_id', a.id,
            'reverted_at', a.reverted_at,
            'noticed_at', v_run)),
        updated_at = v_run
    FROM transfer_identity_applications a
    WHERE e.sport = p_sport AND e.outcome = 'confirmed'
      AND e.evidence->'confirmation_reverted' IS NULL
      AND (e.evidence->'confirmed'->>'application_id')::bigint = a.id
      AND a.reverted_at IS NOT NULL;

    GET DIAGNOSTICS episodes_reverted = ROW_COUNT;
END;
$$;

COMMENT ON FUNCTION public.seal_confirmed_episodes(text, integer) IS
    'Episode outcome ground truth from transfer_identity_applications (applied, '
    'unreverted): seal open episodes confirmed, upgrade announcement-lag fizzles '
    '(<= lag_days after coverage end), downgrade when applications are reverted. '
    'Run BEFORE roll_narrative_episodes so confirmation beats same-night quiet-seal.';

-- ── 2. Source performance: the virtuous cycle's measurement ─────────────────────────

CREATE TABLE IF NOT EXISTS public.source_performance (
    sport           TEXT NOT NULL REFERENCES public.sports(id),
    source          TEXT NOT NULL,
    pairs_covered   INTEGER NOT NULL,
    confirmed_covered INTEGER NOT NULL,
    early_confirmed INTEGER NOT NULL,
    avg_lead_days   NUMERIC(6,1),
    best_lead_days  NUMERIC(6,1),
    reliability     SMALLINT NOT NULL,
    components      JSONB NOT NULL DEFAULT '{}'::jsonb,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (sport, source),
    CONSTRAINT source_performance_reliability_check CHECK (reliability BETWEEN 0 AND 100)
);

COMMENT ON TABLE public.source_performance IS
    'Per-source measured transfer-prediction record: of the (player, team) pairs a '
    'source was attributed on (transfer_rumors.source_names), how many became applied '
    'moves, how many did the source report EARLY (before the pair ever reached '
    'advanced_talks), and with what lead time. The earned overlay to the assigned '
    'source_tiers prior. Presented, not gatekept: reliability is n-based belief in the '
    'track record itself — serve it with the score, do not filter by it. Feeding these '
    'weights back into scrub/heat/vibe is a separate, eval-gated step.';

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
        SELECT DISTINCT ON (a.player_id, a.new_team_id)
               a.player_id, a.new_team_id AS team_id, a.applied_at
        FROM transfer_identity_applications a
        WHERE a.sport = p_sport
          AND a.applied_at IS NOT NULL AND a.reverted_at IS NULL
        ORDER BY a.player_id, a.new_team_id, a.applied_at ASC
    ),
    attributions AS (
        -- A source's first attribution per rumor pair.
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
               'note', 'confirm_rate denominator includes open/unresolved pairs; '
                       'lead days are signed (negative = pile-on after the move)'),
           v_run
    FROM agg a;

    GET DIAGNOSTICS sources_written = ROW_COUNT;
END;
$$;

COMMENT ON FUNCTION public.refresh_source_performance(text, numeric) IS
    'Full recompute of the per-source predictive record from transfer_rumors '
    'attribution x applied identity moves. Idempotent, set-based, no model calls. '
    'reliability = 100 * confirmed/(confirmed + k): n-based belief in the record.';

INSERT INTO public.schema_migrations(version) VALUES ('157_confirmed_seal_source_performance')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

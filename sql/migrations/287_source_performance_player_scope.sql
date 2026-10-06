-- Coach transfer reports share the transfer_rumors.player_id column with players.
-- Keep the existing source accuracy study scoped to player outcomes.
CREATE OR REPLACE FUNCTION public.refresh_source_performance(
    p_sport text,
    p_k numeric DEFAULT 5,
    OUT sources_written integer
) RETURNS integer
LANGUAGE plpgsql AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    DELETE FROM public.source_performance WHERE sport = p_sport;

    WITH apps AS (
        SELECT player_id, team_id, applied_at
        FROM public.transfer_ground_truth WHERE sport = p_sport
    ),
    attributions AS (
        SELECT r.player_id, r.team_id, s.source, min(r.generated_at) AS first_reported
        FROM public.transfer_rumors r
        CROSS JOIN LATERAL unnest(r.source_names) AS s(source)
        WHERE r.sport = p_sport AND r.subject_type = 'player'
          AND r.is_rumor IS TRUE
        GROUP BY 1, 2, 3
    ),
    pair_advanced AS (
        SELECT player_id, team_id, min(generated_at) AS first_advanced
        FROM public.transfer_rumors
        WHERE sport = p_sport AND subject_type = 'player'
          AND is_rumor IS TRUE
          AND stage IN ('advanced_talks', 'here_we_go')
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
    INSERT INTO public.source_performance
        (sport, source, pairs_covered, confirmed_covered, early_confirmed,
         avg_lead_days, best_lead_days, reliability, components, computed_at)
    SELECT p_sport, a.source, a.pairs_covered, a.confirmed_covered, a.early_confirmed,
           a.avg_lead_days, a.best_lead_days,
           round(100 * a.confirmed_covered / (a.confirmed_covered + p_k))::smallint,
           jsonb_build_object(
               'k', p_k,
               'confirm_rate', CASE WHEN a.pairs_covered = 0 THEN NULL
                    ELSE round(a.confirmed_covered::numeric / a.pairs_covered, 3) END,
               'note', 'player outcomes from transfer_ground_truth; lead days signed'),
           v_run
    FROM agg a;

    GET DIAGNOSTICS sources_written = ROW_COUNT;
END;
$$;

COMMENT ON FUNCTION public.refresh_source_performance(text, numeric) IS
'Full recompute of player-transfer source accuracy from attributed positive reports and applied player moves. Denials and coach rumor rows are excluded.';

INSERT INTO public.schema_migrations(version)
VALUES ('287_source_performance_player_scope') ON CONFLICT DO NOTHING;

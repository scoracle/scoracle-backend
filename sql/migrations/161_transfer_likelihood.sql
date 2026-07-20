-- 161_transfer_likelihood.sql
--
-- Roadmap item 3 (Plan - Narrative Graph): the Transfer Likelihood score — the
-- user-facing number that answers Transfermarkt's human-eye rumour probability with a
-- calibrated, receipted score. Design decisions, all from the Rogers/Tottenham tests:
--
--   * STATEFUL, ON THE OPEN EPISODE. One number per (player, destination) story — not
--     per legacy rumor generation (265 whipsawing rows for Rogers). Sealed episodes
--     freeze their final likelihood + full history: that frozen series against the
--     sealed outcome is EXACTLY the dataset the calibration eval (item 6) needs.
--
--   * HYSTERESIS kills the whipsaw. The raw fused score can move freely; the SERVED
--     score rises fast only with corroboration (multi-source or tier-1 recency) and
--     falls at a bounded rate. No more here_we_go-at-heat-4 one day, speculation the
--     next.
--
--   * FUSION with receipts (presented, not gatekept — components carry every part):
--       coverage   (0.40) — link strength: the volume/diversity/recency signal
--       language   (0.35) — best recent legacy rumor stage x confidence, recency-decayed
--                           (the signal that led Rogers by 5 weeks; replaced by typed
--                           narrative_events confidence when the extraction stage lands)
--       source     (0.15) — best source tier among recent attributions
--       trajectory (0.10) — heating_up / developing / cooling_off
--       + up to 5 bonus pts from source_performance early-caller records (earned
--         overlay; thin today, compounds every window)
--
--   * Own-team pairs never score (they never open episodes since mig 159).
--
-- v_transfer_likelihood is the serving surface: open scored episodes with names,
-- ordered by likelihood — the future Go endpoint reads this view.
--
-- Deploy order: ADDITIVE — apply before next cron tick.

BEGIN;

ALTER TABLE public.narrative_episodes
    ADD COLUMN IF NOT EXISTS likelihood SMALLINT,
    ADD COLUMN IF NOT EXISTS likelihood_components JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS likelihood_history JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS likelihood_updated_at TIMESTAMPTZ;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'narrative_episodes_likelihood_check'
          AND conrelid = 'public.narrative_episodes'::regclass
    ) THEN
        ALTER TABLE public.narrative_episodes
            ADD CONSTRAINT narrative_episodes_likelihood_check
            CHECK (likelihood IS NULL OR (likelihood >= 0 AND likelihood <= 100));
    END IF;
END $$;

COMMENT ON COLUMN public.narrative_episodes.likelihood IS
    'Served transfer likelihood 0-100 for open player-team stories: hysteresis-smoothed '
    'fusion of coverage, language stage, source tier, and trajectory (breakdown in '
    'likelihood_components). Frozen at seal — final value + history vs outcome is the '
    'calibration dataset.';
COMMENT ON COLUMN public.narrative_episodes.likelihood_history IS
    'Append-only [{d, v}] daily series of the served likelihood while open. Bounded by '
    'episode lifetime (stories run weeks, not years).';

CREATE OR REPLACE FUNCTION public.score_transfer_likelihood(
    p_sport text,
    p_max_rise_uncorroborated integer DEFAULT 10,
    p_max_fall integer DEFAULT 8,
    p_corrob_sources integer DEFAULT 3,
    p_corrob_tier numeric DEFAULT 0.7,
    OUT episodes_scored integer
) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    WITH open_eps AS (
        SELECT e.id, e.sport, e.subject_id AS player_id, e.object_id AS team_id,
               e.likelihood AS prev, e.likelihood_history AS hist
        FROM narrative_episodes e
        WHERE e.sport = p_sport AND e.status = 'open'
          AND e.link_type = 'co_mention'
          AND e.subject_type = 'player' AND e.object_type = 'team'
    ),
    linked AS (
        SELECT o.*, COALESCE(l.strength, 0) AS link_strength, l.trajectory
        FROM open_eps o
        LEFT JOIN narrative_links l
          ON l.sport = o.sport AND l.link_type = 'co_mention'
         AND l.subject_type = 'player' AND l.subject_id = o.player_id
         AND l.object_type = 'team' AND l.object_id = o.team_id
    ),
    rumor AS (
        -- Best recent language signal per pair: max stage in the last 30 days with its
        -- confidence, recency-decayed from the latest generation carrying that stage.
        SELECT r.player_id, r.team_id,
               max(CASE r.stage WHEN 'here_we_go' THEN 4 WHEN 'advanced_talks' THEN 3
                                WHEN 'concrete_interest' THEN 2 WHEN 'speculation' THEN 1
                                ELSE 0 END) AS stage_rank,
               max(r.confidence) FILTER (WHERE r.stage IS NOT NULL) AS best_conf,
               max(r.generated_at) FILTER (WHERE r.stage IS NOT NULL) AS latest_staged,
               max(r.source_count) AS recent_source_count
        FROM transfer_rumors r
        WHERE r.sport = p_sport AND r.generated_at > now() - interval '30 days'
        GROUP BY 1, 2
    ),
    tiering AS (
        -- Best source tier among the pair's recent attributions.
        SELECT r.player_id, r.team_id, max(st.weight) AS best_tier
        FROM transfer_rumors r
        CROSS JOIN LATERAL unnest(r.source_names) AS s(source)
        JOIN source_tiers st ON lower(st.source) = lower(s.source) AND st.kind = 'news'
        WHERE r.sport = p_sport AND r.generated_at > now() - interval '30 days'
        GROUP BY 1, 2
    ),
    perf AS (
        -- Earned early-caller bonus: best (early_confirmed x its own reliability).
        SELECT r.player_id, r.team_id,
               max(spf.early_confirmed * spf.reliability / 100.0) AS early_cred
        FROM transfer_rumors r
        CROSS JOIN LATERAL unnest(r.source_names) AS s(source)
        JOIN source_performance spf ON spf.sport = r.sport AND spf.source = s.source
        WHERE r.sport = p_sport AND r.generated_at > now() - interval '30 days'
        GROUP BY 1, 2
    ),
    calc AS (
        SELECT li.*,
               COALESCE(ru.stage_rank, 0) AS stage_rank,
               COALESCE(ru.best_conf, 0.5) AS stage_conf,
               CASE WHEN ru.latest_staged IS NULL THEN 0
                    ELSE exp(-(EXTRACT(EPOCH FROM (now() - ru.latest_staged)) / 86400.0) / 14.0)
               END AS stage_recency,
               COALESCE(ru.recent_source_count, 0) AS recent_sources,
               COALESCE(t.best_tier, 0.3) AS best_tier,
               LEAST(5, round(COALESCE(pf.early_cred, 0)))::int AS perf_bonus,
               CASE li.trajectory WHEN 'heating_up' THEN 100
                                  WHEN 'cooling_off' THEN 0 ELSE 50 END AS traj_score
        FROM linked li
        LEFT JOIN rumor ru ON ru.player_id = li.player_id AND ru.team_id = li.team_id
        LEFT JOIN tiering t ON t.player_id = li.player_id AND t.team_id = li.team_id
        LEFT JOIN perf pf ON pf.player_id = li.player_id AND pf.team_id = li.team_id
    ),
    scored AS (
        SELECT c.*,
               LEAST(100, GREATEST(0, round(
                   0.40 * c.link_strength
                 + 0.35 * 100 * (c.stage_rank / 4.0) * c.stage_conf * c.stage_recency
                 + 0.15 * 100 * LEAST(1.0, c.best_tier)
                 + 0.10 * c.traj_score
               ) + c.perf_bonus))::smallint AS raw,
               (c.recent_sources >= p_corrob_sources OR c.best_tier >= p_corrob_tier)
                   AS corroborated
        FROM calc c
    ),
    final AS (
        SELECT s.*,
               CASE
                   WHEN s.prev IS NULL THEN s.raw
                   WHEN s.raw > s.prev AND s.corroborated THEN s.raw
                   WHEN s.raw > s.prev THEN LEAST(s.raw, s.prev + p_max_rise_uncorroborated)::smallint
                   WHEN s.raw < s.prev THEN GREATEST(s.raw, s.prev - p_max_fall)::smallint
                   ELSE s.prev
               END AS served
        FROM scored s
    )
    UPDATE narrative_episodes e
    SET likelihood = f.served,
        likelihood_components = jsonb_build_object(
            'raw', f.raw,
            'served', f.served,
            'previous', f.prev,
            'coverage', f.link_strength,
            'stage_rank', f.stage_rank,
            'stage_conf', f.stage_conf,
            'stage_recency', round(f.stage_recency::numeric, 3),
            'best_tier', f.best_tier,
            'traj', f.traj_score,
            'perf_bonus', f.perf_bonus,
            'recent_sources', f.recent_sources,
            'corroborated', f.corroborated),
        likelihood_history = CASE
            WHEN f.prev IS DISTINCT FROM f.served
                 OR e.likelihood_updated_at IS NULL
                 OR e.likelihood_updated_at < now() - interval '1 day'
            THEN e.likelihood_history
                 || jsonb_build_array(jsonb_build_object('d', to_char(v_run, 'YYYY-MM-DD'),
                                                         'v', f.served))
            ELSE e.likelihood_history END,
        likelihood_updated_at = v_run
    FROM final f
    WHERE e.id = f.id;

    GET DIAGNOSTICS episodes_scored = ROW_COUNT;
END;
$$;

COMMENT ON FUNCTION public.score_transfer_likelihood(text, integer, integer, integer, numeric) IS
    'Fuses coverage + language stage + source tier + trajectory (+ earned early-caller '
    'bonus) into the served transfer likelihood on each open player-team episode, with '
    'hysteresis: rises freely only when corroborated (multi-source or tier-1), falls at '
    'a bounded rate. Run after roll_narrative_episodes in the nightly chain.';

CREATE OR REPLACE VIEW public.v_transfer_likelihood AS
SELECT e.sport, e.id AS episode_id,
       e.subject_id AS player_id, p.name AS player_name,
       e.object_id AS team_id, t.name AS team_name,
       e.likelihood, e.likelihood_components, trajectory_hint.trajectory,
       e.started_at, e.peak_strength, e.article_count, e.distinct_sources,
       e.season, e.likelihood_updated_at
FROM public.narrative_episodes e
JOIN public.players p ON p.id = e.subject_id AND p.sport = e.sport
JOIN public.teams t ON t.id = e.object_id AND t.sport = e.sport
LEFT JOIN LATERAL (
    SELECT l.trajectory
    FROM public.narrative_links l
    WHERE l.sport = e.sport AND l.link_type = 'co_mention'
      AND l.subject_type = 'player' AND l.subject_id = e.subject_id
      AND l.object_type = 'team' AND l.object_id = e.object_id
) trajectory_hint ON TRUE
WHERE e.status = 'open' AND e.link_type = 'co_mention'
  AND e.subject_type = 'player' AND e.object_type = 'team'
  AND e.likelihood IS NOT NULL
  -- Never board a player's own current club (covers pre-mig-159 leftover open
  -- episodes; those quiet-seal naturally but must not serve meanwhile).
  AND NOT EXISTS (
      SELECT 1 FROM public.player_current_identity pci
      WHERE pci.sport = e.sport AND pci.player_id = e.subject_id
        AND pci.team_id = e.object_id);

COMMENT ON VIEW public.v_transfer_likelihood IS
    'Serving surface for the likelihood board: open player-team stories with the served '
    'score, full component receipts, and link trajectory. The Go endpoint reads this.';

INSERT INTO public.schema_migrations(version) VALUES ('161_transfer_likelihood')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

-- 170_junction_events_partition.sql
--
-- Junction memory rollout step 9, corpus half: outputs-as-EVENTS banking with a
-- PROVENANCE PARTITION (operator, 2026-07-20: "partition + exclude"). Junction verdicts
-- (starting with the transfer stage) are banked as narrative_events so the graph carries
-- one unified event log — BUT tagged origin='junction' and WALLED OUT of every numeric
-- feedback path, so the model's own conclusions can never inflate the signal it reads.
-- This is the echo-chamber rule enforced in the schema, not just the prompt label.
--
-- (1) narrative_events.origin ('extraction' | 'junction', default 'extraction'). The
--     dedupe unique index gains origin so a junction verdict and a real extraction event
--     for the same (article, pair, predicate) coexist instead of clobbering each other.
--
-- (2) refresh_typed_links + score_transfer_likelihood: both re-emitted with an
--     `origin = 'extraction'` filter on their narrative_events scan. Extraction-authored
--     events feed typed links and the likelihood language input exactly as before;
--     junction-authored events are invisible to both. (Bodies are the mig-167 functions
--     verbatim + the one filter line — dumped from the live catalog to avoid drift.)
--
-- The transfer stage becomes the first junction producer in the accompanying binary
-- (bank_transfer_junction_event): a served rumor writes a trade_rumor/trade_confirmed
-- event anchored to the pair's OLDEST article (stable across re-verdicts → upsert, not
-- accumulate). Excluded from the numeric loop; available for audit + future card lines.
--
-- Deploy order: apply BEFORE the binary that writes junction events (and that updated
-- the extraction upsert's ON CONFLICT to include origin).

BEGIN;

ALTER TABLE public.narrative_events
    ADD COLUMN IF NOT EXISTS origin text NOT NULL DEFAULT 'extraction';

ALTER TABLE public.narrative_events
    DROP CONSTRAINT IF EXISTS narrative_events_origin_check;
ALTER TABLE public.narrative_events
    ADD CONSTRAINT narrative_events_origin_check
    CHECK (origin IN ('extraction', 'junction'));

COMMENT ON COLUMN public.narrative_events.origin IS
    'Provenance partition (mig 170): extraction = model read of one article (feeds typed '
    'links + likelihood); junction = a junction stage''s own banked verdict (walled out '
    'of every numeric feedback path — continuity/audit only, never corroboration).';

-- Rebuild the dedupe unique index to include origin: junction and extraction events for
-- the same (article, subject, predicate, object) must not collide.
DROP INDEX IF EXISTS public.idx_narrative_events_dedupe;
CREATE UNIQUE INDEX idx_narrative_events_dedupe
    ON public.narrative_events (article_id, sport, subject_type, subject_id, predicate,
                                COALESCE(object_type, ''), COALESCE(object_id, 0), origin);

CREATE OR REPLACE FUNCTION public.refresh_typed_links(p_sport text, p_window_days integer DEFAULT 90, OUT links_upserted integer, OUT links_zeroed integer)
 RETURNS record
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_run timestamptz := clock_timestamp();
BEGIN
    WITH ev AS (
        SELECT e.subject_type, e.subject_id, e.predicate, e.object_type, e.object_id,
               e.article_id, e.source, e.event_date, e.sentiment,
               CASE e.confidence WHEN 'confirmed' THEN 1.0
                                 WHEN 'reported'  THEN 0.7
                                 ELSE 0.4 END AS w,
               CASE e.confidence WHEN 'confirmed' THEN 3
                                 WHEN 'reported'  THEN 2
                                 ELSE 1 END AS conf_rank
        FROM narrative_events e
        WHERE e.sport = p_sport
          AND e.origin = 'extraction'  -- mig 170: junction-authored events NEVER feed typed links
          AND e.object_id IS NOT NULL
          AND e.event_date > now() - make_interval(days => p_window_days)
    ),
    agg AS (
        SELECT subject_type, subject_id, predicate, object_type, object_id,
               count(*) AS total,
               count(DISTINCT article_id) AS articles,
               count(DISTINCT source) FILTER (WHERE source IS NOT NULL) AS distinct_sources,
               sum(w) AS wsum,
               count(*) FILTER (WHERE event_date > now() - interval '14 days') AS recent14,
               max(event_date) AS newest,
               min(event_date) AS oldest,
               max(conf_rank) AS conf_rank,
               sum(w * sentiment) FILTER (WHERE sentiment IS NOT NULL)
                   / NULLIF(sum(w) FILTER (WHERE sentiment IS NOT NULL), 0) AS wsent
        FROM ev
        GROUP BY 1, 2, 3, 4, 5
    ),
    calc AS (
        SELECT *,
               EXTRACT(EPOCH FROM (now() - newest)) / 86400.0 AS age_days,
               LEAST(1.0, wsum / 4.0) AS evidence,
               LEAST(1.0, GREATEST(distinct_sources, 1)::numeric / 3.0) AS volume,
               recent14::numeric / GREATEST(total, 1) AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *,
               exp(-age_days / 21.0) AS recency,
               GREATEST(0, LEAST(100, round(
                   100 * exp(-age_days / 21.0)
                       * (0.5 * evidence + 0.3 * volume + 0.2 * recent_frac))))::smallint
                   AS strength
        FROM calc
    )
    INSERT INTO narrative_links AS l
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         strength, components, weighted_sentiment, confidence,
         event_count, article_count, distinct_sources,
         first_seen_at, last_event_at, trajectory, trajectory_components, updated_at)
    SELECT p_sport, f.predicate, f.subject_type, f.subject_id, f.object_type, f.object_id,
           f.strength,
           jsonb_build_object(
               'event_count', f.total,
               'article_count', f.articles,
               'distinct_sources', f.distinct_sources,
               'weighted_evidence', round(f.wsum::numeric, 2),
               'recent_14d', f.recent14,
               'newest_age_days', round(f.age_days::numeric, 1),
               'evidence', round(f.evidence::numeric, 3),
               'volume', round(f.volume::numeric, 3),
               'recency', round(f.recency::numeric, 3),
               'recent_frac', round(f.recent_frac::numeric, 3),
               'window_days', p_window_days,
               'window_oldest', f.oldest),
           round(f.wsent::numeric, 3),
           CASE f.conf_rank WHEN 3 THEN 'confirmed' WHEN 2 THEN 'reported'
                            ELSE 'speculative' END,
           f.total, f.articles, f.distinct_sources,
           f.oldest, f.newest,
           'developing_story',
           jsonb_build_object('previous', NULL, 'current', f.strength,
                              'delta', NULL, 'direction', 'new_or_unmatched'),
           v_run
    FROM fin f
    ON CONFLICT (sport, link_type, subject_type, subject_id, object_type, object_id)
    DO UPDATE SET
        strength = EXCLUDED.strength,
        components = EXCLUDED.components,
        weighted_sentiment = EXCLUDED.weighted_sentiment,
        confidence = EXCLUDED.confidence,
        event_count = EXCLUDED.event_count,
        article_count = EXCLUDED.article_count,
        distinct_sources = EXCLUDED.distinct_sources,
        first_seen_at = LEAST(l.first_seen_at, EXCLUDED.first_seen_at),
        last_event_at = EXCLUDED.last_event_at,
        -- The shared ±10-bucket trajectory vocabulary (trajectory.rs / db.go / co_mention).
        trajectory = CASE
            WHEN l.strength IS NULL THEN 'developing_story'
            WHEN EXCLUDED.strength - l.strength >= 10 THEN 'heating_up'
            WHEN EXCLUDED.strength - l.strength <= -10 THEN 'cooling_off'
            ELSE 'developing_story' END,
        trajectory_components = jsonb_build_object(
            'previous', l.strength,
            'current', EXCLUDED.strength,
            'delta', CASE WHEN l.strength IS NULL THEN NULL
                          ELSE EXCLUDED.strength - l.strength END,
            'direction', CASE
                WHEN l.strength IS NULL THEN 'new_or_unmatched'
                WHEN EXCLUDED.strength - l.strength >= 10 THEN 'up'
                WHEN EXCLUDED.strength - l.strength <= -10 THEN 'down'
                ELSE 'stable' END),
        updated_at = v_run;

    GET DIAGNOSTICS links_upserted = ROW_COUNT;

    -- Typed links whose events all fell out of the window: decay to 0, keep history.
    UPDATE narrative_links l
    SET trajectory = CASE WHEN l.strength >= 10 THEN 'cooling_off'
                          ELSE 'developing_story' END,
        trajectory_components = jsonb_build_object(
            'previous', l.strength, 'current', 0,
            'delta', -l.strength,
            'direction', CASE WHEN l.strength >= 10 THEN 'down' ELSE 'stable' END),
        strength = 0,
        components = l.components || '{"decayed_out_of_window": true}'::jsonb,
        updated_at = v_run
    WHERE l.sport = p_sport
      AND l.link_type <> 'co_mention'
      AND l.strength > 0
      AND l.updated_at < v_run;

    GET DIAGNOSTICS links_zeroed = ROW_COUNT;
END;
$function$

;

CREATE OR REPLACE FUNCTION public.score_transfer_likelihood(p_sport text, p_max_rise_uncorroborated integer DEFAULT 10, p_max_fall integer DEFAULT 8, p_corrob_sources integer DEFAULT 3, p_corrob_tier numeric DEFAULT 0.7, OUT episodes_scored integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
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
        -- Best recent LEGACY language signal per pair: max stage in the last 30 days
        -- with its confidence, recency-decayed from the latest staged generation.
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
    typed AS (
        -- TYPED language signal (mig 167, step 8): trade events from the extraction
        -- stage. confirmed maps to the top of the legacy ladder; a reported rumor to
        -- concrete-interest grade; speculative to the bottom rung.
        SELECT ne.subject_id AS player_id, ne.object_id AS team_id,
               max(CASE WHEN ne.predicate = 'trade_confirmed' THEN 4
                        WHEN ne.predicate = 'trade_rumor'
                             AND ne.confidence = 'reported' THEN 2
                        WHEN ne.predicate = 'trade_rumor' THEN 1
                        ELSE 0 END) AS typed_rank,
               max(ne.event_date) AS latest_typed
        FROM narrative_events ne
        WHERE ne.sport = p_sport
          AND ne.origin = 'extraction'  -- mig 170: junction-authored events NEVER feed the likelihood language input
          AND ne.event_date > now() - interval '30 days'
          AND ne.subject_type = 'player' AND ne.object_type = 'team'
          AND ne.predicate IN ('trade_rumor', 'trade_confirmed')
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
               COALESCE(ty.typed_rank, 0) AS typed_rank,
               CASE WHEN ty.latest_typed IS NULL THEN 0
                    ELSE exp(-(EXTRACT(EPOCH FROM (now() - ty.latest_typed)) / 86400.0) / 14.0)
               END AS typed_recency,
               COALESCE(ru.recent_source_count, 0) AS recent_sources,
               COALESCE(t.best_tier, 0.3) AS best_tier,
               LEAST(5, round(COALESCE(pf.early_cred, 0)))::int AS perf_bonus,
               CASE li.trajectory WHEN 'heating_up' THEN 100
                                  WHEN 'cooling_off' THEN 0 ELSE 50 END AS traj_score
        FROM linked li
        LEFT JOIN rumor ru ON ru.player_id = li.player_id AND ru.team_id = li.team_id
        LEFT JOIN typed ty ON ty.player_id = li.player_id AND ty.team_id = li.team_id
        LEFT JOIN tiering t ON t.player_id = li.player_id AND t.team_id = li.team_id
        LEFT JOIN perf pf ON pf.player_id = li.player_id AND pf.team_id = li.team_id
    ),
    langs AS (
        -- The 0-100 language subscores: legacy rumor-stage vs typed extraction. The
        -- fusion takes the GREATEST — typed is sparse today and degrades gracefully to
        -- legacy; typed confidence is a fixed 0.85 (its confidence lives in the rank
        -- mapping, not a per-generation float).
        SELECT c.*,
               100 * (c.stage_rank / 4.0) * c.stage_conf * c.stage_recency AS legacy_lang,
               100 * (c.typed_rank / 4.0) * 0.85 * c.typed_recency AS typed_lang
        FROM calc c
    ),
    scored AS (
        SELECT l.*,
               LEAST(100, GREATEST(0, round(
                   0.40 * l.link_strength
                 + 0.35 * GREATEST(l.legacy_lang, l.typed_lang)
                 + 0.15 * 100 * LEAST(1.0, l.best_tier)
                 + 0.10 * l.traj_score
               ) + l.perf_bonus))::smallint AS raw,
               (l.recent_sources >= p_corrob_sources OR l.best_tier >= p_corrob_tier)
                   AS corroborated
        FROM langs l
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
            'typed_rank', f.typed_rank,
            'typed_recency', round(f.typed_recency::numeric, 3),
            'language_source', CASE
                WHEN f.typed_lang = 0 AND f.legacy_lang = 0 THEN 'none'
                WHEN f.typed_lang > f.legacy_lang THEN 'typed'
                ELSE 'legacy' END,
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
$function$

;

INSERT INTO public.schema_migrations(version) VALUES ('170_junction_events_partition')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

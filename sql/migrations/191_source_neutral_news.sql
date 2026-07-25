-- 191_source_neutral_news.sql
--
-- Remove static source tiering from live news/transfer scoring. Source diversity
-- and source-performance memories remain, but source_tiers no longer changes
-- fetch/read eligibility, heat, graph strength, likelihood, or transfer prompts.

BEGIN;

CREATE OR REPLACE FUNCTION public.compute_transfer_heat(
    p_team_id integer,
    p_player_id integer,
    p_sport text,
    OUT heat smallint,
    OUT components jsonb,
    OUT news_ids bigint[]
) RETURNS record
LANGUAGE sql STABLE
AS $$
    WITH corpus AS (
        SELECT 'news'::text AS kind, a.id::text AS item_id, a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type='player'
             AND pe.entity_id = p_player_id AND pe.sport = p_sport
        WHERE a.bucket IS DISTINCT FROM 'non_transfer'
          AND a.published_at > NOW() - INTERVAL '14 days'
          AND te.vetted IS TRUE
          AND pe.vetted IS TRUE
          AND (te.title_pos IS NULL OR pe.title_pos IS NULL
               OR abs(te.title_pos - pe.title_pos) <= 50)
    ),
    agg AS (
        SELECT
            count(DISTINCT c.src) AS distinct_sources,
            count(*) FILTER (WHERE c.ts > NOW() - INTERVAL '3 days') AS recent3,
            count(*) AS total,
            max(c.ts) AS newest
        FROM corpus c
    ),
    calc AS (
        SELECT *,
            EXTRACT(EPOCH FROM (NOW() - newest)) / 3600.0 AS age_hours,
            LEAST(1.0, distinct_sources::numeric / 5.0)   AS volume,
            recent3::numeric / GREATEST(total, 1)         AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *, exp(-age_hours / 72.0) AS recency FROM calc
    )
    SELECT
        CASE WHEN total = 0 THEN NULL
             ELSE GREATEST(0, LEAST(100,
                    round(100 * recency * (0.6 * volume + 0.4 * recent_frac))))::smallint
        END,
        CASE WHEN total = 0 THEN '{}'::jsonb
             ELSE jsonb_build_object(
                'distinct_sources', distinct_sources,
                'recent_3d', recent3,
                'total_14d', total,
                'newest_age_hours', round(age_hours::numeric, 1),
                'volume', round(volume::numeric, 3),
                'recency', round(recency::numeric, 3),
                'recent_frac', round(recent_frac::numeric, 3))
        END,
        COALESCE((SELECT array_agg(item_id::bigint) FROM corpus WHERE kind='news'), '{}')
    FROM fin;
$$;

CREATE OR REPLACE FUNCTION public.refresh_co_mention_links(
    p_sport text,
    p_window_days integer DEFAULT 90,
    OUT pairs_upserted integer,
    OUT pairs_zeroed integer
) RETURNS record
LANGUAGE plpgsql
AS $$
DECLARE
    v_run_started timestamptz := clock_timestamp();
BEGIN
    WITH corpus AS (
        SELECT e1.entity_type AS subject_type, e1.entity_id AS subject_id,
               e2.entity_type AS object_type,  e2.entity_id AS object_id,
               a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities e1 ON e1.article_id = a.id
             AND e1.sport = p_sport AND e1.vetted IS TRUE
        JOIN news_article_entities e2 ON e2.article_id = a.id
             AND e2.sport = p_sport AND e2.vetted IS TRUE
        WHERE a.published_at > now() - make_interval(days => p_window_days)
          AND (e1.entity_type, e1.entity_id) < (e2.entity_type, e2.entity_id)
          AND (e1.title_pos IS NULL OR e2.title_pos IS NULL
               OR abs(e1.title_pos - e2.title_pos) <= 50)
    ),
    agg AS (
        SELECT c.subject_type, c.subject_id, c.object_type, c.object_id,
               count(*) AS total,
               count(DISTINCT c.src) AS distinct_sources,
               count(*) FILTER (WHERE c.ts > now() - INTERVAL '14 days') AS recent14,
               max(c.ts) AS newest,
               min(c.ts) AS oldest
        FROM corpus c
        GROUP BY 1, 2, 3, 4
    ),
    calc AS (
        SELECT *,
               EXTRACT(EPOCH FROM (now() - newest)) / 86400.0 AS age_days,
               LEAST(1.0, distinct_sources::numeric / 5.0) AS volume,
               recent14::numeric / GREATEST(total, 1) AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *,
               exp(-age_days / 21.0) AS recency,
               GREATEST(0, LEAST(100, round(
                   100 * exp(-age_days / 21.0)
                       * (0.6 * volume + 0.4 * recent_frac))))::smallint AS strength
        FROM calc
    )
    INSERT INTO narrative_links AS l
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         strength, components, article_count, distinct_sources,
         first_seen_at, last_event_at, trajectory, trajectory_components, updated_at)
    SELECT p_sport, 'co_mention', f.subject_type, f.subject_id, f.object_type, f.object_id,
           f.strength,
           jsonb_build_object(
               'article_count', f.total,
               'distinct_sources', f.distinct_sources,
               'recent_14d', f.recent14,
               'newest_age_days', round(f.age_days::numeric, 1),
               'volume', round(f.volume::numeric, 3),
               'recency', round(f.recency::numeric, 3),
               'recent_frac', round(f.recent_frac::numeric, 3),
               'window_days', p_window_days,
               'window_oldest', f.oldest),
           f.total, f.distinct_sources,
           f.oldest, f.newest,
           'developing_story',
           jsonb_build_object('previous', NULL, 'current', f.strength,
                              'delta', NULL, 'direction', 'new_or_unmatched'),
           v_run_started
    FROM fin f
    ON CONFLICT (sport, link_type, subject_type, subject_id, object_type, object_id)
    DO UPDATE SET
        strength = EXCLUDED.strength,
        components = EXCLUDED.components,
        article_count = EXCLUDED.article_count,
        distinct_sources = EXCLUDED.distinct_sources,
        first_seen_at = LEAST(l.first_seen_at, EXCLUDED.first_seen_at),
        last_event_at = EXCLUDED.last_event_at,
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
        updated_at = v_run_started;

    GET DIAGNOSTICS pairs_upserted = ROW_COUNT;

    UPDATE narrative_links l
    SET trajectory = CASE WHEN l.strength >= 10 THEN 'cooling_off'
                          ELSE 'developing_story' END,
        trajectory_components = jsonb_build_object(
            'previous', l.strength, 'current', 0,
            'delta', -l.strength,
            'direction', CASE WHEN l.strength >= 10 THEN 'down' ELSE 'stable' END),
        strength = 0,
        components = l.components || '{"decayed_out_of_window": true}'::jsonb,
        updated_at = v_run_started
    WHERE l.sport = p_sport
      AND l.link_type = 'co_mention'
      AND l.updated_at < v_run_started
      AND l.strength IS DISTINCT FROM 0;

    GET DIAGNOSTICS pairs_zeroed = ROW_COUNT;
END;
$$;

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
        SELECT ne.subject_id AS player_id, ne.object_id AS team_id,
               max(CASE WHEN ne.predicate = 'trade_confirmed' THEN 4
                        WHEN ne.predicate = 'trade_rumor'
                             AND ne.confidence = 'reported' THEN 2
                        WHEN ne.predicate = 'trade_rumor' THEN 1
                        ELSE 0 END) AS typed_rank,
               max(ne.event_date) AS latest_typed
        FROM narrative_events ne
        WHERE ne.sport = p_sport
          AND ne.origin = 'extraction'
          AND ne.event_date > now() - interval '30 days'
          AND ne.subject_type = 'player' AND ne.object_type = 'team'
          AND ne.predicate IN ('trade_rumor', 'trade_confirmed')
        GROUP BY 1, 2
    ),
    perf AS (
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
               LEAST(5, round(COALESCE(pf.early_cred, 0)))::int AS perf_bonus,
               CASE li.trajectory WHEN 'heating_up' THEN 100
                                  WHEN 'cooling_off' THEN 0 ELSE 50 END AS traj_score
        FROM linked li
        LEFT JOIN rumor ru ON ru.player_id = li.player_id AND ru.team_id = li.team_id
        LEFT JOIN typed ty ON ty.player_id = li.player_id AND ty.team_id = li.team_id
        LEFT JOIN perf pf ON pf.player_id = li.player_id AND pf.team_id = li.team_id
    ),
    langs AS (
        SELECT c.*,
               100 * (c.stage_rank / 4.0) * c.stage_conf * c.stage_recency AS legacy_lang,
               100 * (c.typed_rank / 4.0) * 0.85 * c.typed_recency AS typed_lang
        FROM calc c
    ),
    scored AS (
        SELECT l.*,
               LEAST(100, GREATEST(0, round(
                   0.45 * l.link_strength
                 + 0.40 * GREATEST(l.legacy_lang, l.typed_lang)
                 + 0.15 * l.traj_score
               ) + l.perf_bonus))::smallint AS raw,
               (l.recent_sources >= p_corrob_sources) AS corroborated
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

COMMENT ON FUNCTION public.score_transfer_likelihood(
    p_sport text,
    p_max_rise_uncorroborated integer,
    p_max_fall integer,
    p_corrob_sources integer,
    p_corrob_tier numeric,
    OUT episodes_scored integer
) IS 'Fuses coverage + language (GREATEST of legacy rumor-stage and typed-event signal) + trajectory (+ earned early-caller bonus) into the served transfer likelihood on each open player-team episode, with hysteresis. Static source tiers are ignored; source performance can still appear as organic memory.';

COMMENT ON COLUMN public.transfer_rumors.input_hash IS
    'SHA-256 (128-bit hex prefix) of the pair''s material vetting inputs (sorted pair-corpus news ids + distinct_sources from the heat components + the deterministic team relationship); the per-pair transfer debounce key. Static source tiers are deliberately excluded.';

INSERT INTO public.schema_migrations(version) VALUES ('191_source_neutral_news')
    ON CONFLICT DO NOTHING;

COMMIT;

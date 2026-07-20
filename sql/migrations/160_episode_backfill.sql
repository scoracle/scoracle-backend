-- 160_episode_backfill.sql
--
-- Roadmap item 5 (Plan - Narrative Graph): as-of replay backfill. The graph was born
-- 2026-07-19; every story that rose AND died before birth left no episode — the summer
-- window's fizzled flirtations are invisible. backfill_narrative_episodes() replays the
-- strength formula over an as-of date grid (weekly by default) across the news rail and
-- mints SEALED 'fizzled' episodes for pre-graph stories, giving the memory layer its
-- history ("Spurs also chased X in June before signing Y").
--
-- Rules, conservative by design:
--   * Only pairs with NO existing episode (live opens already carry June started_at via
--     window_oldest; retro-confirmed pairs are owned by seal_confirmed_episodes).
--   * Own-team pairs excluded (mig 159 semantics, current identity — honest limitation:
--     historical identity is not replayed).
--   * Mint only if the as-of grid peak reached the open threshold AND the pair has the
--     article floor — same notability bar the live roll applies.
--   * All minted episodes seal 'fizzled'; ground truth upgrades them to 'confirmed' via
--     seal_confirmed_episodes path b on the next cron tick (run seal after backfilling).
--   * Peak fidelity = grid granularity (step_days); flagged in evidence.
--
-- One-time operational process per window (idempotent: re-running skips existing
-- episodes) — NOT a cron. Run after any future gap in graph uptime, or to extend
-- memory when the rail's retention grows.
--
-- Deploy order: ADDITIVE — function only; the run is operational.

BEGIN;

CREATE OR REPLACE FUNCTION public.backfill_narrative_episodes(
    p_sport text,
    p_start date,
    p_end date,
    p_step_days integer DEFAULT 7,
    p_open_threshold integer DEFAULT 40,
    p_close_threshold integer DEFAULT 15,
    p_min_articles integer DEFAULT 3,
    OUT episodes_minted integer
) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_run timestamptz := clock_timestamp();
    v_d timestamptz;
BEGIN
    CREATE TEMP TABLE _bf_grid (
        subject_type text, subject_id integer, object_type text, object_id integer,
        asof timestamptz, strength smallint
    ) ON COMMIT DROP;

    -- Replay the live formula (refresh_co_mention_links) at each grid date.
    FOR v_d IN
        SELECT generate_series(p_start::timestamptz, p_end::timestamptz,
                               make_interval(days => p_step_days))
    LOOP
        INSERT INTO _bf_grid
        WITH corpus AS (
            SELECT e1.entity_type AS subject_type, e1.entity_id AS subject_id,
                   e2.entity_type AS object_type, e2.entity_id AS object_id,
                   a.source AS src, a.published_at AS ts
            FROM news_articles a
            JOIN news_article_entities e1 ON e1.article_id = a.id
                 AND e1.sport = p_sport AND e1.vetted IS TRUE
            JOIN news_article_entities e2 ON e2.article_id = a.id
                 AND e2.sport = p_sport AND e2.vetted IS TRUE
            WHERE a.published_at > v_d - interval '90 days'
              AND a.published_at <= v_d
              AND (e1.entity_type, e1.entity_id) < (e2.entity_type, e2.entity_id)
              AND (e1.title_pos IS NULL OR e2.title_pos IS NULL
                   OR abs(e1.title_pos - e2.title_pos) <= 50)
        ),
        agg AS (
            SELECT c.subject_type, c.subject_id, c.object_type, c.object_id,
                   count(*) AS total,
                   count(DISTINCT c.src) AS distinct_sources,
                   count(*) FILTER (WHERE c.ts > v_d - interval '14 days') AS recent14,
                   max(c.ts) AS newest,
                   COALESCE(MAX(st.weight), 0.3) AS tier_weight
            FROM corpus c
            LEFT JOIN source_tiers st ON lower(st.source) = lower(c.src) AND st.kind = 'news'
            GROUP BY 1, 2, 3, 4
        )
        SELECT subject_type, subject_id, object_type, object_id, v_d,
               GREATEST(0, LEAST(100, round(
                   100 * tier_weight
                       * exp(-(EXTRACT(EPOCH FROM (v_d - newest)) / 86400.0) / 21.0)
                       * (0.6 * LEAST(1.0, distinct_sources::numeric / 5.0)
                          + 0.4 * recent14::numeric / GREATEST(total, 1)))))::smallint
        FROM agg;
    END LOOP;

    -- Mint sealed 'fizzled' episodes for notable pre-graph stories.
    WITH peaks AS (
        SELECT DISTINCT ON (subject_type, subject_id, object_type, object_id)
               subject_type, subject_id, object_type, object_id,
               strength AS peak_strength, asof AS peaked_at
        FROM _bf_grid
        ORDER BY subject_type, subject_id, object_type, object_id, strength DESC, asof ASC
    ),
    spans AS (
        -- True coverage span from the rail itself (not grid-quantized).
        SELECT pk.*, sp.first_art, sp.last_art, sp.arts, sp.srcs
        FROM peaks pk,
        LATERAL (
            SELECT min(a.published_at) AS first_art, max(a.published_at) AS last_art,
                   count(*) AS arts, count(DISTINCT a.source) AS srcs
            FROM news_articles a
            JOIN news_article_entities e1 ON e1.article_id = a.id
                 AND e1.sport = p_sport AND e1.vetted IS TRUE
                 AND e1.entity_type = pk.subject_type AND e1.entity_id = pk.subject_id
            JOIN news_article_entities e2 ON e2.article_id = a.id
                 AND e2.sport = p_sport AND e2.vetted IS TRUE
                 AND e2.entity_type = pk.object_type AND e2.entity_id = pk.object_id
            WHERE a.published_at <= p_end::timestamptz
              AND (e1.title_pos IS NULL OR e2.title_pos IS NULL
                   OR abs(e1.title_pos - e2.title_pos) <= 50)
        ) sp
        WHERE pk.peak_strength >= p_open_threshold
    )
    INSERT INTO narrative_episodes
        (sport, link_type, subject_type, subject_id, object_type, object_id,
         status, outcome, season, started_at, peaked_at, ended_at,
         peak_strength, last_strength, article_count, distinct_sources, event_count,
         peak_components, evidence, created_at, updated_at)
    SELECT p_sport, 'co_mention', s.subject_type, s.subject_id, s.object_type, s.object_id,
           'sealed', 'fizzled', sp.current_season,
           s.first_art, s.peaked_at, s.last_art,
           s.peak_strength, NULL,
           s.arts, s.srcs, 0,
           jsonb_build_object('backfill_peak_strength', s.peak_strength),
           jsonb_build_object('backfill', jsonb_build_object(
               'window_start', p_start, 'window_end', p_end,
               'step_days', p_step_days,
               'peak_granularity_days', p_step_days,
               'minted_at', v_run)),
           v_run, v_run
    FROM spans s
    JOIN sports sp ON sp.id = p_sport
    WHERE s.arts >= p_min_articles
      AND s.first_art IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM narrative_episodes e
          WHERE e.sport = p_sport AND e.link_type = 'co_mention'
            AND e.subject_type = s.subject_type AND e.subject_id = s.subject_id
            AND e.object_type = s.object_type AND e.object_id = s.object_id)
      AND NOT (s.subject_type = 'player' AND s.object_type = 'team'
               AND EXISTS (
                   SELECT 1 FROM player_current_identity pci
                   WHERE pci.sport = p_sport
                     AND pci.player_id = s.subject_id
                     AND pci.team_id = s.object_id));

    GET DIAGNOSTICS episodes_minted = ROW_COUNT;

    DROP TABLE _bf_grid;
END;
$$;

COMMENT ON FUNCTION public.backfill_narrative_episodes(text, date, date, integer, integer, integer, integer) IS
    'As-of replay over the news rail: mints sealed fizzled episodes for stories that '
    'rose and died before the graph existed (peak from a step_days grid, span from the '
    'rail). Idempotent — skips pairs with any existing episode. Run seal_confirmed_'
    'episodes afterward so ground truth upgrades backfilled fizzles to confirmed.';

INSERT INTO public.schema_migrations(version) VALUES ('160_episode_backfill')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
-- Operational run (summer window, all sports):
--   SELECT backfill_narrative_episodes('FOOTBALL','2026-04-15',CURRENT_DATE);
--   SELECT backfill_narrative_episodes('NBA','2026-04-15',CURRENT_DATE);
--   SELECT backfill_narrative_episodes('NFL','2026-04-15',CURRENT_DATE);
--   then: SELECT seal_confirmed_episodes(sport) per sport.

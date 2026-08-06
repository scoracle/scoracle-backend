-- 214_editor_owns_links.sql
--
-- The Editor becomes the SOLE author of "which entities is this article about", and
-- `news_article_entities` stops being a scrub-era workflow table.
--
-- WHY. The table carried the old rail's workflow in its columns: a tri-state `vetted`
-- (NULL = nobody looked, TRUE = the scrub gate approved, FALSE = it rejected), a
-- `match_confidence` sentinel (0.95 = Go's regex-era query hypothesis, 0.8 = the regex secondary,
-- 0.90 = the Editor), `scrubbed_at` named for a stage that no longer exists, and `title_pos`, the
-- character offset behind a headline-proximity gate. TWO processes wrote it — Go at ingest and the
-- Editor after reading — so the Editor's write needed three reconciliation arms (confirm / deny /
-- retract) to keep them coherent. That shape is what let an article silently lose its ENTIRE link
-- set when one entity resolved under two names (8.10).
--
-- Now: Go writes no links at all (the query provenance it was really recording lives on
-- `news_articles.raw`), the Editor clears and rewrites an article's links in one transaction, and a
-- row's EXISTENCE is the verdict. Nothing filters on `vetted` because there is nothing left to
-- filter: every row is a link a model established by reading the body.
--
-- THE ONE PROPERTY THAT MAKES THIS SAFE, worth checking rather than trusting: every consumer
-- already filtered `vetted IS TRUE`, and only the Editor ever set it. So deleting the rows that are
-- not TRUE deletes exactly what every consumer was already ignoring — no live query sees a
-- different row set the moment this lands.
--
-- Deploy order (F-022 — this is a column DROP): release the binaries that stop reading these
-- columns FIRST, then apply. 8.11's Go and Rust do not reference them.
--
-- Functions below are derived from the CURRENT prod definitions (pg_get_functiondef), per the
-- template, with only the `vetted` / `title_pos` predicates removed.

BEGIN;

-- 1. The rows every consumer was already filtering out: 52,963 explicit denials + 98,003 never
--    adjudicated (Go hypotheses for articles the Editor has not reached, plus the pre-flip
--    unscrubbed tail). Under "a row IS a link", neither is a link.
DELETE FROM public.news_article_entities WHERE vetted IS NOT TRUE;

-- 2. The four LIVE functions, rebuilt without the retired predicates. compute_transfer_heat
--    matters most: the Insider calls it per (team, player) pair, and it still carried the mig-033
--    title-proximity gate that 8.8 removed from the Rust side — the same gate, half-alive in SQL,
--    thinning pairs on a column nothing writes any more.
CREATE OR REPLACE FUNCTION public.compute_transfer_heat(p_team_id integer, p_player_id integer, p_sport text, OUT heat smallint, OUT components jsonb, OUT news_ids bigint[])
 RETURNS record
 LANGUAGE sql
 STABLE
AS $function$
    WITH corpus AS (
        SELECT 'news'::text AS kind, a.id::text AS item_id, a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type='player'
             AND pe.entity_id = p_player_id AND pe.sport = p_sport
        WHERE a.bucket IS DISTINCT FROM 'non_transfer'
          AND a.published_at > NOW() - INTERVAL '14 days'
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
$function$;


CREATE OR REPLACE FUNCTION public.collapse_exact_title_duplicates(p_lookback interval DEFAULT '72:00:00'::interval, p_min_title_len integer DEFAULT 30)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_marked integer;
BEGIN
    WITH cand AS (
        SELECT a.id,
               a.source,
               a.published_at,
               a.feed_rank,
               -- strip punctuation, fold accents, collapse runs of spaces
               unaccent(lower(regexp_replace(
                   regexp_replace(a.title, '[^a-zA-Z0-9 ]', '', 'g'), ' +', ' ', 'g'))) AS norm,
               EXISTS (SELECT 1 FROM public.news_article_entities e
                        WHERE e.article_id = a.id) AS corpus_visible,
               EXISTS (SELECT 1 FROM public.news_article_readings r
                        WHERE r.article_id = a.id) AS already_read
          FROM public.news_articles a
         WHERE a.published_at > now() - p_lookback
           AND a.title <> ''
           AND a.duplicate_of IS NULL
    ),
    grp AS (
        SELECT norm
          FROM cand
         WHERE length(norm) >= p_min_title_len
         GROUP BY norm
        HAVING count(*) > 1
           AND count(DISTINCT source) > 1     -- cross-source only
    ),
    ranked AS (
        SELECT c.id,
               c.source,
               first_value(c.id) OVER w     AS canonical_id,
               first_value(c.source) OVER w AS canonical_source
          FROM cand c
          JOIN grp g ON g.norm = c.norm
        WINDOW w AS (
            PARTITION BY c.norm
            ORDER BY c.corpus_visible DESC,
                     c.already_read    DESC,
                     c.published_at    ASC,
                     c.feed_rank       ASC NULLS LAST,
                     c.id              ASC
        )
    )
    UPDATE public.news_articles a
       SET duplicate_of = r.canonical_id
      FROM ranked r
     WHERE a.id = r.id
       AND r.id <> r.canonical_id
       -- Per-PAIR cross-source check, not per-group. A group of {A, A, B} passes the group-level
       -- `count(DISTINCT source) > 1` test, and without this the second A would be suppressed by
       -- its own sibling -- exactly the same-source collapse the deleted cosine branch was doing.
       -- The second A stays canonical; only B's copy is suppressed.
       AND r.source IS DISTINCT FROM r.canonical_source
       AND a.duplicate_of IS NULL;

    GET DIAGNOSTICS v_marked = ROW_COUNT;
    RETURN v_marked;
END;
$function$;


CREATE OR REPLACE FUNCTION public.refresh_co_mention_links(p_sport text, p_window_days integer DEFAULT 90, OUT pairs_upserted integer, OUT pairs_zeroed integer)
 RETURNS record
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_run_started timestamptz := clock_timestamp();
BEGIN
    WITH corpus AS (
        SELECT e1.entity_type AS subject_type, e1.entity_id AS subject_id,
               e2.entity_type AS object_type,  e2.entity_id AS object_id,
               a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities e1 ON e1.article_id = a.id
             AND e1.sport = p_sport
        JOIN news_article_entities e2 ON e2.article_id = a.id
             AND e2.sport = p_sport
        WHERE a.published_at > now() - make_interval(days => p_window_days)
          AND (e1.entity_type, e1.entity_id) < (e2.entity_type, e2.entity_id)
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
$function$;


CREATE OR REPLACE FUNCTION public.backfill_narrative_episodes(p_sport text, p_start date, p_end date, p_step_days integer DEFAULT 7, p_open_threshold integer DEFAULT 40, p_close_threshold integer DEFAULT 15, p_min_articles integer DEFAULT 3, OUT episodes_minted integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
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
                 AND e1.sport = p_sport
            JOIN news_article_entities e2 ON e2.article_id = a.id
                 AND e2.sport = p_sport
            WHERE a.published_at > v_d - interval '90 days'
              AND a.published_at <= v_d
              AND (e1.entity_type, e1.entity_id) < (e2.entity_type, e2.entity_id)
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
                 AND e1.sport = p_sport
                 AND e1.entity_type = pk.subject_type AND e1.entity_id = pk.subject_id
            JOIN news_article_entities e2 ON e2.article_id = a.id
                 AND e2.sport = p_sport
                 AND e2.entity_type = pk.object_type AND e2.entity_id = pk.object_id
            WHERE a.published_at <= p_end::timestamptz
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
$function$;



-- 3. Trigger functions whose triggers were dropped in the flip (mig 213). Zero triggers reference
--    them; they survive only as text mentioning a column that is about to disappear. Appendix A
--    scheduled these for Phase 9 — the column drop brings them forward.
DROP FUNCTION IF EXISTS public.enqueue_derive_on_vetted();
DROP FUNCTION IF EXISTS public.enqueue_transfers_if_transfer_related();
DROP FUNCTION IF EXISTS public.enqueue_voices_on_routing_tags();

-- 4. Indexes that exist only to serve the dropped columns. The partial vetted index is covered by
--    idx_news_entities_lookup_created, which already leads with the same three columns.
DROP INDEX IF EXISTS public.idx_nae_vetted_lookup;
DROP INDEX IF EXISTS public.idx_news_entities_unscrubbed;

-- 5. Assertions BEFORE the drop, while the columns still exist to be checked.
DO $mig$
DECLARE not_true int; still_referencing int;
BEGIN
    SELECT count(*) INTO not_true FROM public.news_article_entities WHERE vetted IS NOT TRUE;
    IF not_true <> 0 THEN
        RAISE EXCEPTION 'expected 0 non-TRUE rows after the delete, found %', not_true;
    END IF;

    SELECT count(*) INTO still_referencing
      FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
     WHERE n.nspname = 'public' AND p.prokind = 'f'
       AND pg_get_functiondef(p.oid) LIKE '%news_article_entities%'
       AND (pg_get_functiondef(p.oid) LIKE '%vetted%'
         OR pg_get_functiondef(p.oid) LIKE '%title_pos%'
         OR pg_get_functiondef(p.oid) LIKE '%scrubbed_at%'
         OR pg_get_functiondef(p.oid) LIKE '%match_confidence%');
    IF still_referencing <> 0 THEN
        RAISE EXCEPTION '% function(s) still reference a column this migration drops', still_referencing;
    END IF;

    RAISE NOTICE 'assertions passed: 0 non-TRUE rows, 0 functions referencing dropped columns';
END $mig$;

-- 6. The columns. Every one is old-rail workflow state; none has a reader left in Go, Rust or SQL.
ALTER TABLE public.news_article_entities
    DROP COLUMN vetted,
    DROP COLUMN match_confidence,
    DROP COLUMN scrubbed_at,
    DROP COLUMN title_pos;

COMMENT ON TABLE public.news_article_entities IS
    'Which entities an article is about. WRITTEN ONLY by the Editor (editor::write_links), which '
    'clears an article''s rows and rewrites them from what it resolved after reading the body. A '
    'row''s EXISTENCE is the verdict -- there is no vetted/confidence column, and absence is a '
    'denial. Ingest writes nothing here; which entity''s sweep surfaced an article is recorded on '
    'news_articles.raw (query_team_id). PLAN-one-rail 8.11.';

-- 7. Self-record inside the transaction.
INSERT INTO public.schema_migrations(version) VALUES ('214_editor_owns_links')
    ON CONFLICT DO NOTHING;

COMMIT;

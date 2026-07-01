-- 098_decommission_tweets.sql  (Optimization Ledger O15)
--
-- X / Twitter is permanently decommissioned (parked 2026-06-13; news + transfer
-- heat now derive entirely from the Google-RSS → local model corpus). The serving routes,
-- handler, thirdparty client, prepared statements, config, and the tweet-TTL purge
-- ticker were removed in the Go change that ships with this migration.
--
-- Both tweet tables are empty (X stopped being fetched at the 2026-06-13 park), so
-- this drops no live data. compute_transfer_heat is redefined to drop its tweets
-- UNION arm; its tweet_ids OUT param is retained (now always '{}') so the transfer
-- worker (ml/transfer.go) and seed_transfer_rumors stay source-compatible — the
-- worker's loadPairTweets already short-circuits on an empty id list, so no tweets
-- table is ever queried after this. (Trivial future cleanup: drop the vestigial
-- tweet_ids OUT param + transfer_rumors.input_tweet_ids; left for a no-X-revival pass.)
--
-- Order matters: redefine the function (drops its only references to tweets/
-- tweet_entities) BEFORE dropping the tables.

CREATE OR REPLACE FUNCTION public.compute_transfer_heat(
    p_team_id integer, p_player_id integer, p_sport text,
    OUT heat smallint, OUT components jsonb, OUT news_ids bigint[], OUT tweet_ids text[]
)
 RETURNS record
 LANGUAGE sql
 STABLE
AS $function$
    WITH corpus AS (
        -- News articles linking BOTH the team and the player, in PROXIMITY + VETTED.
        -- (The former tweets UNION arm was removed — X is decommissioned.)
        SELECT 'news'::text AS kind, a.id::text AS item_id, a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type='player'
             AND pe.entity_id = p_player_id AND pe.sport = p_sport
        WHERE a.published_at > NOW() - INTERVAL '14 days'
          AND (te.vetted IS TRUE OR te.scrubbed_at IS NULL)
          AND (pe.vetted IS TRUE OR pe.scrubbed_at IS NULL)
          AND (te.title_pos IS NULL OR pe.title_pos IS NULL
               OR abs(te.title_pos - pe.title_pos) <= 50)
    ),
    agg AS (
        SELECT
            count(DISTINCT c.src) AS distinct_sources,
            count(*) FILTER (WHERE c.ts > NOW() - INTERVAL '3 days') AS recent3,
            count(*) AS total,
            max(c.ts) AS newest,
            COALESCE(MAX(st.weight), 0.3) AS tier_weight
        FROM corpus c
        LEFT JOIN source_tiers st ON lower(st.source) = lower(c.src) AND st.kind = c.kind
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
                    round(100 * tier_weight * recency * (0.6 * volume + 0.4 * recent_frac))))::smallint
        END,
        CASE WHEN total = 0 THEN '{}'::jsonb
             ELSE jsonb_build_object(
                'distinct_sources', distinct_sources,
                'recent_3d', recent3,
                'total_14d', total,
                'newest_age_hours', round(age_hours::numeric, 1),
                'tier_weight', tier_weight,
                'volume', round(volume::numeric, 3),
                'recency', round(recency::numeric, 3),
                'recent_frac', round(recent_frac::numeric, 3))
        END,
        COALESCE((SELECT array_agg(item_id::bigint) FROM corpus WHERE kind='news'), '{}'),
        '{}'::text[]    -- tweet_ids: permanently empty (X decommissioned)
    FROM fin;
$function$;

-- Now safe to drop the empty tweet infrastructure (002_add_twitter_cache.sql).
DROP TABLE IF EXISTS public.tweet_entities;
DROP TABLE IF EXISTS public.tweets;
DROP TABLE IF EXISTS public.twitter_lists;

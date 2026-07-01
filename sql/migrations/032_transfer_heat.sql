-- 032_transfer_heat.sql
--
-- Deterministic transfer/trade heat index — pure SQL over the news+tweet
-- co-mention corpus for a (team, player) pair. The engine stores the number;
-- it is fully decomposable (heat_components), exactly like rating_breakdown.
-- local model never touches this — it only vets (migration 033 + ml/transfer.go).
--
-- Heat = 100 · tier_weight · recency · (0.6·volume + 0.4·recent_frac)
--   volume       = LEAST(1, distinct_sources / 5)      — breadth of coverage
--   recent_frac  = items(3d) / items(14d)               — heating up vs cooling
--   recency      = exp(-newest_age_hours / 72)          — fades over ~3 days
--   tier_weight  = MAX source credibility (0.3 default) — Romano > 10 aggregators
-- A "source" is a news publication OR a tweet author handle.

BEGIN;

-- ---------------------------------------------------------------------------
-- compute_transfer_heat(team, player, sport) — the pair corpus → heat + parts.
-- Returns NULL heat when the pair has no recent corpus (the persistNoCorpus
-- analog; the read path + seed skip it).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION compute_transfer_heat(
    p_team_id INTEGER, p_player_id INTEGER, p_sport TEXT,
    OUT heat SMALLINT, OUT components JSONB, OUT news_ids BIGINT[], OUT tweet_ids TEXT[]
) LANGUAGE sql STABLE AS $$
    WITH corpus AS (
        -- News articles linking BOTH the team and the player.
        SELECT 'news'::text AS kind, a.id::text AS item_id, a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type='player'
             AND pe.entity_id = p_player_id AND pe.sport = p_sport
        WHERE a.published_at > NOW() - INTERVAL '14 days'
        UNION ALL
        -- Tweets linking BOTH (author handle is the "source").
        SELECT 'twitter', t.id, t.author_username, t.posted_at
        FROM tweets t
        JOIN tweet_entities te ON te.tweet_id = t.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN tweet_entities pe ON pe.tweet_id = t.id AND pe.entity_type='player'
             AND pe.entity_id = p_player_id AND pe.sport = p_sport
        WHERE t.posted_at > NOW() - INTERVAL '14 days'
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
        COALESCE((SELECT array_agg(item_id)         FROM corpus WHERE kind='twitter'), '{}')
    FROM fin;
$$;

-- ---------------------------------------------------------------------------
-- seed_transfer_rumors(sport) — heat-only backfill (Phase 1, no local model). Walks
-- co-mention candidate pairs (team co-mentioned with a player in >= min_articles
-- distinct articles, 14d) and appends a heat row per pair with a positive heat.
-- is_rumor=TRUE / stage='speculation' are PROVISIONAL until local model vets (Phase 2).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION seed_transfer_rumors(p_sport TEXT, p_min_articles INTEGER DEFAULT 2)
RETURNS INTEGER LANGUAGE plpgsql AS $$
DECLARE
    r RECORD;
    h RECORD;
    v_count INTEGER := 0;
BEGIN
    FOR r IN
        SELECT te.entity_id AS team_id, pe.entity_id AS player_id
        FROM news_article_entities te
        JOIN news_article_entities pe
          ON pe.article_id = te.article_id AND pe.sport = te.sport AND pe.entity_type = 'player'
        WHERE te.entity_type = 'team' AND te.sport = p_sport
          AND te.created_at > NOW() - INTERVAL '14 days'
        GROUP BY te.entity_id, pe.entity_id
        HAVING count(DISTINCT te.article_id) >= p_min_articles
    LOOP
        SELECT * INTO h FROM compute_transfer_heat(r.team_id, r.player_id, p_sport);
        IF h.heat IS NOT NULL AND h.heat > 0 THEN
            INSERT INTO transfer_rumors (
                team_id, player_id, sport, trigger_type, heat, heat_components,
                is_rumor, stage, input_news_ids, input_tweet_ids, prompt_version
            ) VALUES (
                r.team_id, r.player_id, p_sport, 'periodic', h.heat, h.components,
                TRUE, 'speculation', h.news_ids, h.tweet_ids, 'heat-v1'
            );
            v_count := v_count + 1;
        END IF;
    END LOOP;
    RETURN v_count;
END;
$$;

COMMIT;

-- 141_news_article_bucket_topic_heat.sql
-- Wave 5 / F2-F3: article-level bucket classification and topic heat-rank.
-- Buckets live on news_articles because they are properties of the article, not
-- of an entity link. NULL is the transition/backfill state.

ALTER TABLE public.news_articles
    ADD COLUMN IF NOT EXISTS bucket text,
    ADD COLUMN IF NOT EXISTS topic_heat integer;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'news_articles_bucket_check'
          AND conrelid = 'public.news_articles'::regclass
    ) THEN
        ALTER TABLE public.news_articles
            ADD CONSTRAINT news_articles_bucket_check
            CHECK (bucket IS NULL OR bucket IN ('transfer', 'non_transfer'));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'news_articles_topic_heat_check'
          AND conrelid = 'public.news_articles'::regclass
    ) THEN
        ALTER TABLE public.news_articles
            ADD CONSTRAINT news_articles_topic_heat_check
            CHECK (topic_heat IS NULL OR topic_heat >= 1);
    END IF;
END $$;

COMMENT ON COLUMN public.news_articles.bucket IS
    'Wave 5 scrub bucket: transfer or non_transfer. NULL means pre-backfill/unknown; downstream reads keep NULL lenient during transition.';

COMMENT ON COLUMN public.news_articles.topic_heat IS
    'Wave 5 topic heat-rank: size of the article''s same-day embedding topic cluster, recomputed idempotently by the Rust cognition worker.';

CREATE INDEX IF NOT EXISTS idx_news_articles_bucket
    ON public.news_articles (bucket)
    WHERE bucket IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_news_articles_topic_heat
    ON public.news_articles (topic_heat DESC, published_at DESC)
    WHERE topic_heat IS NOT NULL;

CREATE OR REPLACE FUNCTION public.compute_transfer_heat(p_team_id integer, p_player_id integer, p_sport text, OUT heat smallint, OUT components jsonb, OUT news_ids bigint[]) RETURNS record
    LANGUAGE sql STABLE
    AS $$
    WITH corpus AS (
        -- News articles linking BOTH the team and the player, in PROXIMITY and
        -- scrub-vetted on both sides. Wave 5 keeps NULL bucket rows during the
        -- transition, but confirmed non-transfer articles no longer contribute
        -- to transfer heat.
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
        COALESCE((SELECT array_agg(item_id::bigint) FROM corpus WHERE kind='news'), '{}')
    FROM fin;
$$;

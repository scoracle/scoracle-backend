-- Operate only on the reports already admitted and selected by the plugin.
-- Slot zero is the whole edition; later slots retain selected report order.
WITH reports AS (
    SELECT publisher, published_at, slot
    FROM unnest($1::text[], $2::bigint[]) WITH ORDINALITY
        AS input(publisher, published_at, slot)
), selected AS (
    SELECT 0::bigint AS slot, count(*) AS article_count,
           count(DISTINCT publisher COLLATE "C") AS distinct_sources, max(published_at) AS newest
    FROM reports
    UNION ALL
    SELECT slot, 1::bigint, 1::bigint, published_at FROM reports
), components AS (
    SELECT slot, article_count, distinct_sources,
           60.0::float8 * (1.0::float8 - exp(-(article_count::float8) / 5.0::float8)) AS volume,
           least(25.0::float8, distinct_sources::float8 * 6.0::float8) AS source_breadth,
           -- Numeric subtraction avoids bigint overflow, preserving the age bands
           -- of Rust's saturating_sub for every i64 epoch (including future dates).
           CASE WHEN $3::bigint::numeric - newest::numeric BETWEEN 0 AND 43200 THEN 15.0::float8
                WHEN $3::bigint::numeric - newest::numeric BETWEEN 43201 AND 86400 THEN 10.0::float8
                WHEN $3::bigint::numeric - newest::numeric BETWEEN 86401 AND 172800 THEN 5.0::float8
                ELSE 0.0::float8 END AS recency
    FROM selected
), scored AS (
    SELECT *, least(100, greatest(0, floor(volume + source_breadth + recency + 0.5)::int)) AS score
    FROM components
)
SELECT slot, score, article_count, distinct_sources, volume, source_breadth, recency,
       least(99, greatest(1, score))::smallint AS card_score
FROM scored ORDER BY slot

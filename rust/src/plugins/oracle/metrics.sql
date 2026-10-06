-- Selected raw card values: Scout, Influencer, strongest narrative, Insider,
-- and Analyst direction. Missing values stay absent, never neutral substitutes.
WITH signs AS (
    SELECT CASE WHEN $1::int >= 70 THEN 1 WHEN $1 <= 35 THEN -1 ELSE 0 END AS strength,
           CASE WHEN $2::int >= 60 THEN 1 WHEN $2 <= 40 THEN -1 ELSE 0 END AS vibe,
           CASE $5::text WHEN 'rising' THEN 1 WHEN 'heating_up' THEN 1
                        WHEN 'falling' THEN -1 WHEN 'cooling_off' THEN -1 ELSE 0 END AS momentum
), pairs AS (
    SELECT product FROM signs,
        unnest(ARRAY[vibe * momentum, strength * momentum, strength * vibe]) AS pair(product)
    WHERE product <> 0
), convergence AS (
    SELECT CASE WHEN count(*) = 0 THEN NULL ELSE
        greatest(1, floor(count(*) FILTER (WHERE product > 0)::float8 / count(*)::float8 * 100 + 0.5)::int)
    END AS value FROM pairs
), signals AS (
    SELECT signal FROM unnest(ARRAY[
        CASE WHEN $1 IS NOT NULL THEN least(100, greatest(1, $1)) END,
        CASE WHEN $2 IS NOT NULL THEN least(100, greatest(1, $2)) END,
        -- Rust round/clamp then cast: NaN casts to zero, infinities clamp.
        CASE WHEN $3::float8 IS NULL THEN NULL WHEN $3 = 'NaN'::float8 THEN 0
             ELSE floor(least(100::float8, greatest(1::float8, $3)) + 0.5)::int END,
        CASE WHEN $4::int IS NOT NULL THEN least(100, greatest(1, $4)) END
    ]) AS input(signal) WHERE signal IS NOT NULL
), score AS (
    SELECT CASE WHEN count(*) = 0 THEN 50 ELSE floor(sum(signal)::float8 / count(*)::float8 + 0.5)::int END AS value
    FROM signals
)
SELECT score.value, convergence.value,
       CASE WHEN convergence.value <= 50 THEN 'crossroads'
            WHEN $5 = 'rising' THEN 'ascendant' WHEN $5 = 'falling' THEN 'waning'
            ELSE 'steady' END AS omen
FROM score, convergence

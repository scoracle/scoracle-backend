-- Classify the selected snapshot's raw float8 score; never round before bands.
-- PostgreSQL orders NaN above finite numbers; Rust comparisons with NaN are
-- false, so preserve its steady direction explicitly (and conviction +5).
SELECT CASE WHEN $1::float8 IS NULL OR $1 = 'NaN'::float8 THEN 'steady'
            WHEN $1 >= 10 THEN 'rising'
            WHEN $1 <= -10 THEN 'falling'
            ELSE 'steady' END AS direction,
       (CASE WHEN $1 IS NULL OR abs($1) < 5 THEN 0
             WHEN abs($1) < 20 THEN 1
             WHEN abs($1) < 35 THEN 2
             WHEN abs($1) < 55 THEN 3
             WHEN abs($1) < 80 THEN 4
             ELSE 5 END
        * CASE WHEN $1 < 0 THEN -1 ELSE 1 END)::int AS conviction

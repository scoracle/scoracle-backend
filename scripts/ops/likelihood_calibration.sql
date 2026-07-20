-- likelihood_calibration.sql — junction memory rollout step 10: the Brier/reliability
-- eval of the transfer-likelihood score against realized outcomes.
--
-- DATA-GATED: sealed episodes freeze their likelihood + daily history (mig 161); the
-- series started accruing 2026-07-19, so this reads meaningfully only after weeks of
-- seals. Run ad hoc (psql -f); when N is real, the numbers become the sentence
-- Transfermarkt cannot say: "our 80s confirm 80% of the time."
--
--   Usage: psql "$DATABASE_URL" -f scripts/ops/likelihood_calibration.sql
--
-- Two readouts:
--   1. Overall Brier score per sport (0 = clairvoyant, 0.25 = coin-flip-at-50 grade):
--      every (day, value) point in every sealed episode's frozen history, scored
--      against the sealed outcome (confirmed = 1, fizzled = 0).
--   2. Reliability diagram: predicted-decile vs observed confirm rate, with N per
--      bin — presented, not gatekept: small-N bins print with their receipts.

WITH sealed AS (
    SELECT e.sport, e.id, e.outcome,
           (e.outcome = 'confirmed')::int AS y,
           jsonb_array_elements(e.likelihood_history) AS pt
    FROM narrative_episodes e
    WHERE e.status = 'sealed'
      AND e.outcome IN ('confirmed', 'fizzled')
      AND jsonb_array_length(e.likelihood_history) > 0
),
points AS (
    SELECT sport, id, outcome, y,
           (pt->>'v')::numeric / 100.0 AS p,
           pt->>'d' AS day
    FROM sealed
)
SELECT sport,
       count(DISTINCT id) AS episodes,
       count(*) AS points,
       round(avg(power(p - y, 2))::numeric, 4) AS brier,
       round(avg(y)::numeric, 3) AS base_rate
FROM points
GROUP BY sport
ORDER BY sport;

WITH sealed AS (
    SELECT e.sport, e.id, (e.outcome = 'confirmed')::int AS y,
           jsonb_array_elements(e.likelihood_history) AS pt
    FROM narrative_episodes e
    WHERE e.status = 'sealed'
      AND e.outcome IN ('confirmed', 'fizzled')
      AND jsonb_array_length(e.likelihood_history) > 0
),
points AS (
    SELECT sport, id, y, (pt->>'v')::int AS v FROM sealed
)
SELECT sport,
       (v / 10) * 10 AS predicted_decile,
       count(*) AS n_points,
       count(DISTINCT id) AS n_episodes,
       round(avg(v)::numeric, 1) AS mean_predicted,
       round(100 * avg(y)::numeric, 1) AS observed_confirm_pct
FROM points
GROUP BY sport, (v / 10)
ORDER BY sport, predicted_decile;

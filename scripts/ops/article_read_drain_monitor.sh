#!/usr/bin/env bash
# Snapshot Article Reader drain/readability while a news backlog is clearing.
#
# Usage:
#   scripts/ops/article_read_drain_monitor.sh
#   scripts/ops/article_read_drain_monitor.sh "30 minutes"
#   scripts/ops/article_read_drain_monitor.sh "6 hours"

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

DB="${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}"
SINCE="${1:-2 hours}"

if [ -z "$DB" ]; then
  echo "DATABASE_PRIVATE_URL or DATABASE_URL is required" >&2
  exit 2
fi

echo "== Article Reader Drain Snapshot =="
date
echo "window: last $SINCE"
echo

psql "$DB" -v ON_ERROR_STOP=1 -P pager=off -v since="$SINCE" <<'SQL'
\echo '== queue =='
SELECT stage,
       status,
       sport,
       count(*) AS rows,
       min(available_at) AS oldest_available_at,
       max(available_at) AS newest_available_at
FROM public.pipeline_work
WHERE stage IN ('scrub', 'graph', 'article_read', 'transfers', 'narratives', 'vibe', 'peak', 'momentum', 'sigil')
GROUP BY stage, status, sport
ORDER BY stage, status, rows DESC;

\echo ''
\echo '== article_read drain pressure =='
WITH params AS (
    SELECT :'since'::interval AS window,
           GREATEST(EXTRACT(EPOCH FROM :'since'::interval) / 3600.0, 0.001)::numeric AS hours
),
queue AS (
    SELECT count(*)::numeric AS pending,
           min(available_at) AS oldest_pending_available_at
    FROM public.pipeline_work
    WHERE stage = 'article_read'
      AND status = 'pending'
),
recent AS (
    SELECT count(*)::numeric AS terminal_rows,
           count(*) FILTER (WHERE status IN ('success', 'irrelevant', 'parse_failed'))::numeric AS model_rows,
           count(*) FILTER (WHERE status IN ('empty_body', 'paywall', 'blocked', 'fetch_failed'))::numeric AS unreadable_rows,
           count(*) FILTER (WHERE status = 'duplicate')::numeric AS duplicate_rows
    FROM public.news_article_readings nar, params
    WHERE nar.updated_at >= NOW() - params.window
),
rates AS (
    SELECT queue.pending,
           queue.oldest_pending_available_at,
           recent.terminal_rows,
           recent.model_rows,
           recent.unreadable_rows,
           recent.duplicate_rows,
           ROUND(recent.terminal_rows / params.hours, 1) AS terminal_per_hour,
           ROUND(recent.model_rows / params.hours, 1) AS model_rows_per_hour,
           CASE
               WHEN recent.terminal_rows > 0
               THEN ROUND(queue.pending / (recent.terminal_rows / params.hours), 2)
               ELSE NULL
           END AS est_hours_to_clear_pending
    FROM queue, recent, params
)
SELECT * FROM rates;

\echo ''
\echo '== recorded model-call share =='
WITH model_calls AS (
    SELECT stage, count(*)::numeric AS rows
    FROM public.cognition_ledger
    WHERE generated_at >= NOW() - :'since'::interval
    GROUP BY stage
    UNION ALL
    SELECT 'article_read' AS stage, count(*)::numeric AS rows
    FROM public.data_fetch_ledger
    WHERE generated_at >= NOW() - :'since'::interval
      AND stage = 'article_read'
      AND parser_outcome = 'parsed'
),
rolled AS (
    SELECT stage, sum(rows) AS rows
    FROM model_calls
    GROUP BY stage
),
total AS (
    SELECT sum(rows) AS rows FROM rolled
)
SELECT rolled.stage,
       rolled.rows::bigint AS model_calls,
       ROUND(rolled.rows * 100.0 / NULLIF(total.rows, 0), 1) AS pct
FROM rolled, total
ORDER BY rolled.rows DESC, rolled.stage;

\echo ''
\echo '== article_read outcomes =='
WITH r AS (
    SELECT nar.status, nar.prompt_version, nar.updated_at
    FROM public.news_article_readings nar
    WHERE nar.updated_at >= NOW() - :'since'::interval
),
b AS (
    SELECT CASE
               WHEN status = 'success' THEN 'readable_success'
               WHEN status = 'irrelevant' THEN 'readable_irrelevant'
               WHEN status IN ('empty_body', 'paywall', 'blocked', 'fetch_failed', 'parse_failed') THEN 'attempted_unreadable'
               WHEN status = 'not_allowlisted' THEN 'legacy_not_attempted_source_policy'
               ELSE 'other_terminal'
           END AS bucket,
           status,
           count(*) AS rows,
           min(updated_at) AS oldest,
           max(updated_at) AS newest
    FROM r
    GROUP BY 1, 2
),
t AS (
    SELECT sum(rows)::numeric AS total FROM b
)
SELECT bucket,
       status,
       rows,
       ROUND(rows * 100.0 / NULLIF(t.total, 0), 1) AS pct,
       oldest,
       newest
FROM b
CROSS JOIN t
ORDER BY rows DESC, bucket, status;

\echo ''
\echo '== top pending article_read sources =='
WITH pending AS (
    SELECT COALESCE(a.source, '') AS source
    FROM public.pipeline_work pw
    JOIN public.news_articles a ON a.id = pw.entity_id
    WHERE pw.stage = 'article_read'
      AND pw.entity_type = 'article'
      AND pw.status = 'pending'
)
SELECT NULLIF(source, '') AS source,
       count(*) AS rows
FROM pending
GROUP BY source
ORDER BY rows DESC, source
LIMIT 30;

\echo ''
\echo '== attempted unreadable by source/domain =='
SELECT COALESCE(a.source, '') AS source,
       COALESCE(nar.final_domain, '(none)') AS final_domain,
       nar.status,
       count(*) AS rows
FROM public.news_article_readings nar
JOIN public.news_articles a ON a.id = nar.article_id
WHERE nar.updated_at >= NOW() - :'since'::interval
  AND nar.status IN ('empty_body', 'paywall', 'blocked', 'fetch_failed', 'parse_failed')
GROUP BY source, final_domain, nar.status
ORDER BY rows DESC, source, final_domain
LIMIT 30;
SQL

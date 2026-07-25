#!/usr/bin/env bash
# Where did the articles go? One report over both halves of the news funnel.
#
# The ingest funnel has two halves that need different instruments:
#
#   1. Pre-persist (the RSS fetch path, Go). Articles dropped here are never
#      written anywhere, so SQL cannot see them at all. The sweep logs a
#      thirdparty.Funnel per sport and per run — that is the only record.
#   2. Post-persist (news_article_entities). Everything admitted is a row with a
#      created_at, so SQL answers these directly and no instrumentation is needed.
#
# Usage:
#   scripts/ops/news_ingest_funnel.sh
#   scripts/ops/news_ingest_funnel.sh "48 hours"

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
# Default 48h, not 24h: co-mention verdicts lag ingest by the Article Reader
# queue depth, so a 24h window is mostly `pending` and the reject columns read
# artificially low.
SINCE="${1:-48 hours}"
INGEST_LOG="${INGEST_LOG:-$ROOT/logs/pipeline-ingest.log}"

if [ -z "$DB" ]; then
  echo "DATABASE_PRIVATE_URL or DATABASE_URL is required" >&2
  exit 2
fi

echo "== News Ingest Funnel =="
date
echo "window: last $SINCE"
echo

echo "== 1. pre-persist: RSS fetch funnel (from $INGEST_LOG) =="
echo "   editions_skipped > 0  => the -rss-limit cap ended the sweep before the"
echo "                            localized editions ran (only edition 0 is exempt)"
echo "   limit_truncated       => articles fetched, matched and deduped, then thrown away"
echo "   teams_zero_admitted   => RSS returned items and MatchesEntity kept none"
echo "   residual != 0         => an uncounted drop stage exists in the fetch path"
echo
if [ -r "$INGEST_LOG" ]; then
  grep -E 'rss sweep (funnel|complete)' "$INGEST_LOG" | tail -12 || echo "   (no funnel lines yet — the sweep predates A4, or has not run since deploy)"
else
  echo "   (log not readable: $INGEST_LOG)"
fi
echo

psql "$DB" -v ON_ERROR_STOP=1 -P pager=off -v since="$SINCE" <<'SQL'
\echo '== 2. post-persist: links written, by grain =='
\echo '   0.95 = broad team primary (scrub decides relevance)'
\echo '   0.8  = Go-flagged co-mention (Article Reader decides from the body)'
\echo '   1.0  = exact non-team primary, auto-vetted at ingest'
SELECT match_confidence,
       sport,
       count(*) AS links,
       count(*) FILTER (WHERE vetted IS TRUE)  AS promoted,
       count(*) FILTER (WHERE vetted IS FALSE) AS rejected,
       count(*) FILTER (WHERE vetted IS NULL)  AS pending,
       ROUND(count(*) FILTER (WHERE vetted IS FALSE) * 100.0
             / NULLIF(count(*) FILTER (WHERE vetted IS NOT NULL), 0), 0) AS reject_pct
FROM public.news_article_entities
WHERE created_at >= NOW() - :'since'::interval
GROUP BY 1, 2
ORDER BY 1 DESC, 3 DESC;

\echo ''
\echo '== 3. noise at the source: entities whose links the model keeps rejecting =='
\echo '   Every rejected link was a model call spent to say "no". A high reject_pct on'
\echo '   a short or common-word name is a missing guard in Go (match.go), not a'
\echo '   model problem: the <4-chars-needs-sport-context rule covers ALIASES only,'
\echo '   never Name, and the risky-solo-term list is query-side only.'
SELECT e.entity_type,
       e.entity_id,
       e.sport,
       COALESCE(t.name, p.name) AS name,
       count(*) AS links,
       count(*) FILTER (WHERE e.vetted IS FALSE) AS rejected,
       ROUND(count(*) FILTER (WHERE e.vetted IS FALSE) * 100.0
             / NULLIF(count(*) FILTER (WHERE e.vetted IS NOT NULL), 0), 0) AS reject_pct
FROM public.news_article_entities e
LEFT JOIN public.teams   t ON t.id = e.entity_id AND e.entity_type = 'team'   AND t.sport = e.sport
LEFT JOIN public.players p ON p.id = e.entity_id AND e.entity_type = 'player' AND p.sport = e.sport
WHERE e.created_at >= NOW() - :'since'::interval
GROUP BY 1, 2, 3, 4
HAVING count(*) >= 10
ORDER BY count(*) FILTER (WHERE e.vetted IS FALSE) DESC
LIMIT 25;

\echo ''
\echo '== 4. co-mention proximity gate coverage =='
\echo '   title_pos IS NULL means the match came from the description only, so the'
\echo '   proximity gate that was meant to constrain these links never applies.'
SELECT count(*) AS secondary_links,
       count(*) FILTER (WHERE title_pos IS NULL) AS no_title_pos,
       ROUND(count(*) FILTER (WHERE title_pos IS NULL) * 100.0 / NULLIF(count(*), 0), 0) AS pct_ungated
FROM public.news_article_entities
WHERE match_confidence = 0.8
  AND created_at >= NOW() - :'since'::interval;
SQL

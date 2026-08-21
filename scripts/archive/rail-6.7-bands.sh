#!/usr/bin/env bash
# PLAN-one-rail 6.7 — the 72h organic-storyline reading. READ-ONLY: every statement here is a
# SELECT, so it is safe to run at any time, as often as you like.
#
# THE READING IS ONLY VALID ONCE THE WINDOW HAS CLOSED (~2026-08-08 22:10 EDT, 72h after the
# Phase 6 deploy at 2026-08-05 22:08:27 EDT). Run it earlier and the script says INTERIM in its
# header — an interim run is a health check, NOT the reading, exactly as the +4-min checkpoint
# was not (Phase 6 Log). Do not close Phase 6 on an INTERIM banner.
#
# The pre-deploy backfill numbers in the Phase 6 Log are the BASELINE, not this measurement:
# every count here is restricted to attach_method='auto', the organic path, since the deploy.
#
# Usage (from archbox, where the DB lives):
#   scripts/rail-6.7-bands.sh                 # 72h from the Phase 6 deploy stamp
#   WINDOW_START='2026-08-05 22:08:27-04' scripts/rail-6.7-bands.sh
#
# What 6.7 asks for, in order:
#   1. storylines/day/sport
#   2. articles-per-storyline distribution — the top cluster should land ~15-25:1 against the
#      20:1 hand count. OUTSIDE THAT BAND: STOP and inspect attach scores (§0 rule 3).
#   3. % of reads attached vs opened-new
#   4. the 3 biggest storylines, listed for the hand-inspection (wrong merges + one preserved
#      contradiction). That last step is a HUMAN read of the titles this prints — a query cannot
#      decide whether a merge is wrong.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null && pwd)"
cd "$REPO_ROOT"

WINDOW_START="${WINDOW_START:-2026-08-05 22:08:27-04}"
WINDOW_HOURS="${WINDOW_HOURS:-72}"

set -a
# shellcheck disable=SC1091
. ./.env.local
set +a
DB="${DATABASE_PRIVATE_URL:-$DATABASE_URL}"

psql "$DB" -v ON_ERROR_STOP=1 -v ws="$WINDOW_START" -v wh="$WINDOW_HOURS" <<'SQL'
\timing off
\pset footer off

SELECT CASE WHEN now() >= (:'ws')::timestamptz + make_interval(hours => :wh)
            THEN 'READING — the 72h window has CLOSED. These numbers close Phase 6.'
            ELSE 'INTERIM — the window is STILL OPEN (' ||
                 to_char((:'ws')::timestamptz + make_interval(hours => :wh), 'Mon DD HH24:MI') ||
                 ' TZ). A health check, NOT the reading. Do not close Phase 6 on this.'
       END AS status,
       to_char((:'ws')::timestamptz, 'Mon DD HH24:MI') AS window_open,
       round(extract(epoch from (now() - (:'ws')::timestamptz)) / 3600.0, 1) AS hours_elapsed;

\echo '\n=== 1. storylines/day/sport (organic — first_seen_at inside the window) ==='
SELECT s.sport,
       date_trunc('day', s.first_seen_at)::date AS day,
       count(*) AS storylines_opened
  FROM public.storylines s
 WHERE s.first_seen_at >= (:'ws')::timestamptz
   AND s.first_seen_at <  (:'ws')::timestamptz + make_interval(hours => :wh)
 GROUP BY 1, 2
 ORDER BY 1, 2;

\echo '\n=== 2. articles-per-storyline, organic attaches only (band: top cluster 15-25:1) ==='
WITH org AS (
    SELECT sa.storyline_id, count(*) AS articles
      FROM public.storyline_articles sa
     WHERE sa.attach_method = 'auto'
       AND sa.attached_at >= (:'ws')::timestamptz
       AND sa.attached_at <  (:'ws')::timestamptz + make_interval(hours => :wh)
     GROUP BY 1
)
SELECT count(*)                                                   AS storylines_with_attaches,
       sum(articles)                                              AS attaches_total,
       round(avg(articles), 2)                                    AS mean_per_storyline,
       percentile_disc(0.5) WITHIN GROUP (ORDER BY articles)      AS p50,
       percentile_disc(0.9) WITHIN GROUP (ORDER BY articles)      AS p90,
       percentile_disc(0.99) WITHIN GROUP (ORDER BY articles)     AS p99,
       max(articles)                                              AS top_cluster,
       CASE WHEN max(articles) IS NULL THEN 'no organic attaches yet'
            WHEN max(articles) BETWEEN 15 AND 25 THEN 'IN BAND (15-25:1)'
            WHEN max(articles) < 15 THEN 'BELOW BAND — inspect attach scores before closing'
            ELSE 'ABOVE BAND — STOP and inspect attach scores for a wrong merge'
       END                                                        AS band_verdict
  FROM org;

\echo '\n=== 3. reads attached vs opened-new (organic reads inside the window) ==='
WITH reads AS (
    SELECT er.article_id, er.storyline_id
      FROM public.editor_reads er
     WHERE er.fetched_at >= (:'ws')::timestamptz
       AND er.fetched_at <  (:'ws')::timestamptz + make_interval(hours => :wh)
       AND er.status = 'success'
)
SELECT count(*)                                                        AS reads_success,
       count(*) FILTER (WHERE storyline_id IS NOT NULL)                AS attached,
       count(*) FILTER (WHERE storyline_id IS NULL)                    AS unattached,
       round(100.0 * count(*) FILTER (WHERE storyline_id IS NOT NULL)
             / NULLIF(count(*), 0), 1)                                 AS pct_attached
  FROM reads;

\echo '\n=== 3b. of the attached, joined an existing storyline vs opened a new one ==='
WITH reads AS (
    SELECT er.article_id, er.storyline_id, er.fetched_at
      FROM public.editor_reads er
     WHERE er.fetched_at >= (:'ws')::timestamptz
       AND er.fetched_at <  (:'ws')::timestamptz + make_interval(hours => :wh)
       AND er.storyline_id IS NOT NULL
)
SELECT count(*) FILTER (WHERE s.first_seen_at >= r.fetched_at - interval '2 seconds') AS opened_new,
       count(*) FILTER (WHERE s.first_seen_at <  r.fetched_at - interval '2 seconds') AS joined_existing,
       round(100.0 * count(*) FILTER (WHERE s.first_seen_at < r.fetched_at - interval '2 seconds')
             / NULLIF(count(*), 0), 1)                                                AS pct_joined_existing
  FROM reads r
  JOIN public.storylines s ON s.id = r.storyline_id;

\echo '\n=== 4. the 3 biggest organic storylines — HAND-INSPECT these for wrong merges ==='
WITH org AS (
    SELECT sa.storyline_id, count(*) AS articles
      FROM public.storyline_articles sa
     WHERE sa.attach_method = 'auto'
       AND sa.attached_at >= (:'ws')::timestamptz
       AND sa.attached_at <  (:'ws')::timestamptz + make_interval(hours => :wh)
     GROUP BY 1
     ORDER BY 2 DESC
     LIMIT 3
)
SELECT o.storyline_id, o.articles, s.sport, s.status,
       to_char(s.first_seen_at, 'Mon DD HH24:MI') AS opened,
       left(coalesce(s.title, '(untitled)'), 90)  AS title
  FROM org o JOIN public.storylines s ON s.id = o.storyline_id
 ORDER BY o.articles DESC;

\echo '\n=== 4b. their member headlines (read these: one saga, or two stories wrongly merged?) ==='
WITH org AS (
    SELECT sa.storyline_id, count(*) AS articles
      FROM public.storyline_articles sa
     WHERE sa.attach_method = 'auto'
       AND sa.attached_at >= (:'ws')::timestamptz
       AND sa.attached_at <  (:'ws')::timestamptz + make_interval(hours => :wh)
     GROUP BY 1 ORDER BY 2 DESC LIMIT 3
)
SELECT sa.storyline_id,
       to_char(sa.attached_at, 'MM-DD HH24:MI') AS attached,
       coalesce(a.source, '?')                  AS source,
       left(a.title, 96)                        AS headline
  FROM org o
  JOIN public.storyline_articles sa ON sa.storyline_id = o.storyline_id AND sa.attach_method = 'auto'
  JOIN public.news_articles a ON a.id = sa.article_id
 ORDER BY sa.storyline_id, sa.attached_at;

\echo '\n=== 4c. T3 contradiction spot-check: opposite-polarity headlines inside ONE storyline ==='
\echo '(a preserved contradiction is a PASS — both sides must still stand, not be reconciled)'
WITH org AS (
    SELECT sa.storyline_id
      FROM public.storyline_articles sa
     WHERE sa.attach_method = 'auto'
       AND sa.attached_at >= (:'ws')::timestamptz
       AND sa.attached_at <  (:'ws')::timestamptz + make_interval(hours => :wh)
     GROUP BY 1 ORDER BY count(*) DESC LIMIT 3
)
SELECT sa.storyline_id, coalesce(a.source, '?') AS source, left(a.title, 96) AS headline
  FROM org o
  JOIN public.storyline_articles sa ON sa.storyline_id = o.storyline_id
  JOIN public.news_articles a ON a.id = sa.article_id
 WHERE a.title ~* '(agreed|agreement|done deal|set to (stay|join|sign)|confirm)'
    OR a.title ~* '(not agreed|no agreement|denied|deny|reject|collapse|off|stays put|no deal)'
 ORDER BY sa.storyline_id, a.title;

\echo '\n=== 5. Verify (Phase 6): the packet trigger must still have fired 0 work rows ==='
SELECT (SELECT count(*) FROM public.packets)                                          AS packets_total,
       (SELECT count(*) FROM public.stage_routing_subscriptions)                      AS subscriptions,
       (SELECT count(*) FROM public.pipeline_work WHERE input_version LIKE 'pk:%')    AS packet_work_rows;
SQL

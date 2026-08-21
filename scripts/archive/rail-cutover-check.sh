#!/usr/bin/env bash
# PLAN-one-rail 8.1 — the five §2 cutover clauses, measured. READ-ONLY: every statement is a
# SELECT, so it is safe to run at any time, as often as you like.
#
# Prints PASS/FAIL per clause with the numbers behind it, and a final verdict line. Clause 3 is
# the exception and says so: the script EMITS the day's 50-link sample, and a human (or a model)
# scores it — a query cannot tell you whether a link is correct. Record the score in the Phase 8
# Log; the script reports clause 3 as SAMPLE, never as PASS.
#
# Usage (from archbox, where the DB lives):
#   scripts/rail-cutover-check.sh              # yesterday's full day (the honest daily reading)
#   DAY=2026-08-06 scripts/rail-cutover-check.sh
#   SAMPLE=50 scripts/rail-cutover-check.sh
#
# §2's condition is 7 CONSECUTIVE days with all five clauses green. One FAIL resets the window
# and is itself a finding: fix it, then restart the count.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null && pwd)"
cd "$REPO_ROOT"

DAY="${DAY:-$(date -d 'yesterday' +%F 2>/dev/null || date -v-1d +%F)}"
SAMPLE="${SAMPLE:-50}"

set -a
# shellcheck disable=SC1091
. ./.env.local
set +a
DB="${DATABASE_PRIVATE_URL:-$DATABASE_URL}"

echo "=============================================================="
echo " §2 CUTOVER CHECK — day $DAY  (link sample size $SAMPLE)"
echo "=============================================================="

psql "$DB" -v ON_ERROR_STOP=1 -v day="$DAY" -v sample="$SAMPLE" <<'SQL'
\timing off
\pset border 2

\echo ''
\echo '=== CLAUSE 1 — Coverage: >=95% of the day''s articles have an editor_reads row within 24h ==='
WITH arrivals AS (
    SELECT na.id, na.fetched_at
      FROM public.news_articles na
     WHERE na.fetched_at >= :'day'::date
       AND na.fetched_at <  :'day'::date + 1
)
SELECT count(*)                                                        AS articles,
       count(er.article_id)                                            AS read_within_24h,
       round(100.0 * count(er.article_id) / NULLIF(count(*), 0), 1)    AS pct,
       CASE WHEN count(*) = 0 THEN 'NO DATA'
            WHEN 100.0 * count(er.article_id) / count(*) >= 95 THEN 'PASS'
            ELSE 'FAIL' END                                            AS verdict
  FROM arrivals a
  LEFT JOIN public.editor_reads er
    ON er.article_id = a.id
   AND er.fetched_at <= a.fetched_at + interval '24 hours';

\echo ''
\echo '=== CLAUSE 2 — Packet presence: every legacy narratives corpus (entity,day) appears in a packet ==='
\echo '    (legacy corpus = an entity with >=3 linked articles that day; the bar the old rail used)'
\echo '    NOTE 2026-08-06: news_article_entities.vetted was DROPPED in the 8.10/8.11 rip. Every'
\echo '    surviving link IS Editor-vetted by construction (Phase 2 retired auto-vetting), so the'
\echo '    predicate was removed, not weakened -- the bar is unchanged.'
WITH legacy AS (
    SELECT nae.entity_type, nae.entity_id, nae.sport, count(*) AS linked_articles
      FROM public.news_article_entities nae
      JOIN public.news_articles na ON na.id = nae.article_id
     WHERE na.fetched_at >= :'day'::date
       AND na.fetched_at <  :'day'::date + 1
       AND nae.entity_type IN ('player','team')
     GROUP BY 1,2,3
    HAVING count(*) >= 3
),
packeted AS (
    SELECT DISTINCT se.entity_type, se.entity_id, se.sport
      FROM public.packets p
      JOIN public.storyline_entities se ON se.storyline_id = p.storyline_id
     WHERE se.left_at IS NULL
       AND p.compiled_at >= :'day'::date
       AND p.compiled_at <  :'day'::date + 1
)
SELECT count(*)                                                          AS legacy_entity_days,
       count(*) FILTER (WHERE pk.entity_id IS NOT NULL)                  AS covered_by_packet,
       count(*) FILTER (WHERE pk.entity_id IS NULL)                      AS missing,
       CASE WHEN count(*) = 0 THEN 'NO DATA'
            WHEN count(*) FILTER (WHERE pk.entity_id IS NULL) = 0 THEN 'PASS'
            ELSE 'FAIL' END                                              AS verdict
  FROM legacy l
  LEFT JOIN packeted pk
    ON pk.entity_type = l.entity_type AND pk.entity_id = l.entity_id AND pk.sport = l.sport;

\echo ''
\echo '--- clause 2 detail: the missing entity-days (empty = PASS) ---'
WITH legacy AS (
    SELECT nae.entity_type, nae.entity_id, nae.sport, count(*) AS linked_articles
      FROM public.news_article_entities nae
      JOIN public.news_articles na ON na.id = nae.article_id
     WHERE na.fetched_at >= :'day'::date
       AND na.fetched_at <  :'day'::date + 1
       AND nae.entity_type IN ('player','team')
     GROUP BY 1,2,3
    HAVING count(*) >= 3
),
packeted AS (
    SELECT DISTINCT se.entity_type, se.entity_id, se.sport
      FROM public.packets p
      JOIN public.storyline_entities se ON se.storyline_id = p.storyline_id
     WHERE se.left_at IS NULL
       AND p.compiled_at >= :'day'::date
       AND p.compiled_at <  :'day'::date + 1
)
SELECT l.entity_type, l.entity_id, l.sport, l.linked_articles
  FROM legacy l
  LEFT JOIN packeted pk
    ON pk.entity_type = l.entity_type AND pk.entity_id = l.entity_id AND pk.sport = l.sport
 WHERE pk.entity_id IS NULL
 ORDER BY l.linked_articles DESC
 LIMIT 20;

\echo ''
\echo '=== CLAUSE 3 — Precision: the day''s link sample. SCORE THIS BY HAND (>=95% correct). ==='
\echo '    Not a PASS/FAIL the script can render — a query cannot tell you if a link is right.'
SELECT er.article_id,
       l->>'entity_type' AS entity_type,
       (l->>'entity_id')::int AS entity_id,
       l->>'sport'       AS sport,
       l->>'via_surface' AS via_surface,
       left(na.title, 70) AS headline
  FROM public.editor_reads er
  JOIN public.news_articles na ON na.id = er.article_id
  CROSS JOIN LATERAL jsonb_array_elements(er.resolved -> 'links') AS l
 WHERE er.fetched_at >= :'day'::date
   AND er.fetched_at <  :'day'::date + 1
 ORDER BY md5(er.article_id::text || (l->>'entity_id'))
 LIMIT :sample;

\echo ''
\echo '=== CLAUSE 4a — Editor/Investigator dead-letters (attempts >= 5) must be 0 ==='
SELECT coalesce(sum(CASE WHEN stage IN ('editor','investigate_entity','fixture_boxscore') THEN 1 ELSE 0 END), 0) AS dead_letters,
       CASE WHEN coalesce(sum(CASE WHEN stage IN ('editor','investigate_entity','fixture_boxscore') THEN 1 ELSE 0 END), 0) = 0
            THEN 'PASS' ELSE 'FAIL' END AS verdict
  FROM public.pipeline_work
 WHERE attempts >= 5;

\echo ''
\echo '--- clause 4a detail: which rows (empty = PASS) ---'
SELECT stage, entity_type, entity_id, sport, attempts, left(coalesce(last_error,''), 80) AS last_error
  FROM public.pipeline_work
 WHERE attempts >= 5
   AND stage IN ('editor','investigate_entity','fixture_boxscore')
 ORDER BY updated_at DESC
 LIMIT 20;

\echo ''
\echo '=== CLAUSE 5 — Accounting: every packet claim references a MEMBER article. Must be 0. ==='
WITH cl AS (
    SELECT p.id AS packet_id, p.storyline_id, (c->>'article_id')::bigint AS article_id
      FROM public.packets p
      CROSS JOIN LATERAL jsonb_array_elements(p.claims) AS c
     WHERE p.compiled_at >= :'day'::date
       AND p.compiled_at <  :'day'::date + 1
       AND c->>'article_id' IS NOT NULL
)
SELECT count(*)                                                    AS claims_checked,
       count(*) FILTER (WHERE er.article_id IS NULL)               AS orphan_claims,
       CASE WHEN count(*) = 0 THEN 'NO DATA'
            WHEN count(*) FILTER (WHERE er.article_id IS NULL) = 0 THEN 'PASS'
            ELSE 'FAIL' END                                        AS verdict
  FROM cl
  LEFT JOIN public.editor_reads er
    ON er.article_id = cl.article_id
   AND er.storyline_id = cl.storyline_id;

\echo ''
\echo '=== context (not a clause): the day at a glance ==='
SELECT (SELECT count(*) FROM public.packets
         WHERE compiled_at >= :'day'::date AND compiled_at < :'day'::date + 1)      AS packets_compiled,
       (SELECT count(*) FROM public.editor_reads
         WHERE fetched_at >= :'day'::date AND fetched_at < :'day'::date + 1)        AS editor_reads,
       (SELECT count(*) FROM public.storylines
         WHERE first_seen_at >= :'day'::date AND first_seen_at < :'day'::date + 1)  AS storylines_opened;
SQL

echo ""
echo "=============================================================="
echo " CLAUSE 4b — the Editor fixture gate (must be 100%)"
echo "=============================================================="
if [ -x rust/bin/eval ]; then
    rust/bin/eval --task editor --fixtures 2>&1 | tail -12
else
    echo "  rust/bin/eval not present — build it, then: rust/bin/eval --task editor --fixtures"
fi

echo ""
echo "Clauses 1, 2, 4, 5 print their own PASS/FAIL above."
echo "Clause 3 is the hand-scored link sample — record the score in the Phase 8 Log."
echo "§2 needs all five green for 7 CONSECUTIVE days. One FAIL resets the window."

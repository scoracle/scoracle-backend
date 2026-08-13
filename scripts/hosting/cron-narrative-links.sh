#!/usr/bin/env bash
# Cron wrapper for the narrative-graph co-mention refresh (migration 154): recomputes
# narrative_links co_mention edges for every sport from the vetted news rail. Pure SQL,
# set-based, sub-second per sport, no model calls — safe to run any time.
#
# CADENCE IS THE TRAJECTORY BASELINE: each link's heating_up/cooling_off classification
# is its strength delta vs the PREVIOUS refresh (±10 buckets, the shared vocabulary).
# The live cron runs this 45 minutes after each six-hour RSS ingest, after the
# cognition daemon has had time to scrub the sweep.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

set -a
[[ -f .env ]] && source .env
[[ -f .env.local ]] && source .env.local
set +a

DB="${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}"
if [[ -z "$DB" ]]; then
    echo "cron-narrative-links: no DATABASE_PRIVATE_URL / DATABASE_URL" >&2
    exit 1
fi

# Order matters: refresh links (now-state), seal confirmed outcomes from roster ground
# truth (mig 157 — MUST precede the roll so confirmation beats same-night quiet-seal),
# roll episodes (open/peak/quiet-seal), fill chapter→storyline derivations (mig 219 —
# converges news_summaries written thread-only by the pre-cutover binary; precedes the
# part lifecycle so the night's seal/promote sees the freshest chapters), seal
# STORYLINES (mig 219 — ground-truth-confirmed stories resolve and D5 closes every
# other part in one stroke; dormancy is mark_dormant's job in the worker, not here),
# promote established parts (mig 219 — source growth past the authority gate flips
# continuity → established; after the seal so a fresh ground-truth resolve promotes
# same-night), score transfer likelihood on the fresh open set (mig 161 — after roll
# so new stories get scored same-night), re-measure source performance, then promote
# persons (mig 166 — evidence accumulated by the graph stage earns candidate → active;
# promoted figures serve on team memory cards).
exec psql "$DB" -v ON_ERROR_STOP=1 -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, * FROM refresh_co_mention_links('FOOTBALL')
UNION ALL
SELECT 'NBA', now(), * FROM refresh_co_mention_links('NBA')
UNION ALL
SELECT 'NFL', now(), * FROM refresh_co_mention_links('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, * FROM refresh_typed_links('FOOTBALL')
UNION ALL
SELECT 'NBA', now(), * FROM refresh_typed_links('NBA')
UNION ALL
SELECT 'NFL', now(), * FROM refresh_typed_links('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, * FROM seal_confirmed_episodes('FOOTBALL')
UNION ALL
SELECT 'NBA', now(), * FROM seal_confirmed_episodes('NBA')
UNION ALL
SELECT 'NFL', now(), * FROM seal_confirmed_episodes('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, * FROM roll_narrative_episodes('FOOTBALL')
UNION ALL
SELECT 'NBA', now(), * FROM roll_narrative_episodes('NBA')
UNION ALL
SELECT 'NFL', now(), * FROM roll_narrative_episodes('NFL')" -c "
DO \$\$
DECLARE n int;
BEGIN
    -- Guarded: fill_news_summaries_storylines arrives with mig 219 (the threads →
    -- storyline-parts collapse, step A). Fills news_summaries.storyline_id for chapters
    -- the pre-cutover binary wrote thread-only; inert once the Rust cutover writes
    -- storyline_id directly (and dropped with step B).
    IF to_regprocedure('public.fill_news_summaries_storylines()') IS NOT NULL THEN
        n := public.fill_news_summaries_storylines();
        RAISE NOTICE 'fill_news_summaries_storylines filled=%', n;
    ELSE
        RAISE NOTICE 'fill_news_summaries_storylines not installed yet (mig 219) — skipped';
    END IF;
END \$\$;" -c "
DO \$\$
DECLARE r record;
BEGIN
    -- Guarded: seal_storylines arrives with mig 219. PL/pgSQL resolves the call at
    -- first execution, so the IF lets this cron run (and be installed) before the migration.
    IF to_regprocedure('public.seal_storylines(text)') IS NOT NULL THEN
        FOR r IN SELECT s.sport, public.seal_storylines(s.sport) AS resolved
                 FROM (VALUES ('FOOTBALL'),('NBA'),('NFL')) s(sport) LOOP
            RAISE NOTICE 'seal_storylines % resolved=%', r.sport, r.resolved;
        END LOOP;
    ELSE
        RAISE NOTICE 'seal_storylines not installed yet (mig 219) — skipped';
    END IF;
END \$\$;" -c "
DO \$\$
DECLARE r record;
BEGIN
    -- Guarded: promote_established_parts arrives with mig 219 (the collapse's authority
    -- promotion). AFTER the seal sweep so a same-night ground-truth resolve promotes
    -- immediately.
    IF to_regprocedure('public.promote_established_parts(text)') IS NOT NULL THEN
        FOR r IN SELECT s.sport, public.promote_established_parts(s.sport) AS promoted
                 FROM (VALUES ('FOOTBALL'),('NBA'),('NFL')) s(sport) LOOP
            RAISE NOTICE 'promote_established_parts % promoted=%', r.sport, r.promoted;
        END LOOP;
    ELSE
        RAISE NOTICE 'promote_established_parts not installed yet (mig 219) — skipped';
    END IF;
END \$\$;" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, score_transfer_likelihood('FOOTBALL') AS scored
UNION ALL
SELECT 'NBA', now(), score_transfer_likelihood('NBA')
UNION ALL
SELECT 'NFL', now(), score_transfer_likelihood('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, refresh_source_performance('FOOTBALL') AS sources
UNION ALL
SELECT 'NBA', now(), refresh_source_performance('NBA')
UNION ALL
SELECT 'NFL', now(), refresh_source_performance('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, promote_narrative_persons('FOOTBALL') AS promoted
UNION ALL
SELECT 'NBA', now(), promote_narrative_persons('NBA')
UNION ALL
SELECT 'NFL', now(), promote_narrative_persons('NFL')"

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
[[ -f .env.local ]] && source .env.local
set +a

DB="${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}"
if [[ -z "$DB" ]]; then
    echo "cron-narrative-links: no DATABASE_PRIVATE_URL / DATABASE_URL" >&2
    exit 1
fi

# Maintain existing verified graph history and source outcomes. Retired Editor
# storyline authority and part promotion are not a Classifier fallback.
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
SELECT 'FOOTBALL' AS sport, now() AS ran_at, refresh_source_performance('FOOTBALL') AS sources
UNION ALL
SELECT 'NBA', now(), refresh_source_performance('NBA')
UNION ALL
SELECT 'NFL', now(), refresh_source_performance('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, promote_narrative_persons('FOOTBALL') AS promoted
UNION ALL
SELECT 'NBA', now(), promote_narrative_persons('NBA')
UNION ALL
SELECT 'NFL', now(), promote_narrative_persons('NFL')" -c "
DO \$\$
DECLARE r RECORD;
BEGIN
    -- mig 234: after promotion, reconcile the graph layer with verified persons —
    -- link unique surface matches, nominate the rest into the Investigator path.
    IF to_regprocedure('public.reconcile_narrative_persons(text)') IS NOT NULL THEN
        FOR r IN SELECT s.sport, rp.linked, rp.nominated
                 FROM (VALUES ('FOOTBALL'),('NBA'),('NFL')) s(sport),
                      LATERAL public.reconcile_narrative_persons(s.sport) rp LOOP
            RAISE NOTICE 'reconcile_narrative_persons % linked=% nominated=%', r.sport, r.linked, r.nominated;
        END LOOP;
    ELSE
        RAISE NOTICE 'reconcile_narrative_persons not installed yet (mig 234) — skipped';
    END IF;
END \$\$;" -c "
DO \$\$
DECLARE r RECORD;
BEGIN
    -- mig 236: the dynamic-metadata clock. News-active entities >30 days since their
    -- last look re-enter investigate_entity, capped per class per night — the drain
    -- sets the pace (leisurely by design).
    IF to_regprocedure('public.refresh_dynamic_entities(text, integer)') IS NOT NULL THEN
        FOR r IN SELECT s.sport, rd.persons_reopened, rd.players_enqueued, rd.teams_enqueued
                 FROM (VALUES ('FOOTBALL'),('NBA'),('NFL')) s(sport),
                      LATERAL public.refresh_dynamic_entities(s.sport, 25) rd LOOP
            RAISE NOTICE 'refresh_dynamic_entities % persons=% players=% teams=%',
                r.sport, r.persons_reopened, r.players_enqueued, r.teams_enqueued;
        END LOOP;
    ELSE
        RAISE NOTICE 'refresh_dynamic_entities not installed yet (mig 236) — skipped';
    END IF;
END \$\$;" -c "
DO \$\$
DECLARE r RECORD;
BEGIN
    -- mig 237: keep the reporting calendar current — a new season's weeks appear
    -- the night its schedule lands in fixtures.
    IF to_regprocedure('public.rebuild_season_weeks(text)') IS NOT NULL THEN
        FOR r IN SELECT s.sport, public.rebuild_season_weeks(s.sport) AS weeks
                 FROM (VALUES ('FOOTBALL'),('NBA'),('NFL')) s(sport) LOOP
            RAISE NOTICE 'rebuild_season_weeks % weeks=%', r.sport, r.weeks;
        END LOOP;
    ELSE
        RAISE NOTICE 'rebuild_season_weeks not installed yet (mig 237) — skipped';
    END IF;
END \$\$;"

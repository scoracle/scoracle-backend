#!/usr/bin/env bash
# Reporting maintenance: source outcomes and Investigator refresh.
# Keep the historical filename for installed cron entries; Graph refresh is retired.

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

exec psql "$DB" -v ON_ERROR_STOP=1 -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, refresh_source_performance('FOOTBALL') AS sources
UNION ALL
SELECT 'NBA', now(), refresh_source_performance('NBA')
UNION ALL
SELECT 'NFL', now(), refresh_source_performance('NFL')" -c "
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
END \$\$;"

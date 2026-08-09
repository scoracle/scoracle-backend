#!/usr/bin/env bash
# investigate.sh — queue work for the Investigator on demand (Scott, 2026-08-09:
# "I want the Investigator to be easy to queue up").
#
#   scripts/investigate.sh name "Jerry Jones" NFL          # mystery candidate by name
#   scripts/investigate.sh enrich NBA [limit]              # players missing dob/photo/weight
#   scripts/investigate.sh status [SPORT]                  # queue + recent verdicts
#
# `name` seeds an entity_candidates row exactly as the Editor's sweep would (person-kind,
# pending) and enqueues it; re-running re-opens a decided candidate on purpose — a manual
# queue is an explicit request for a fresh look. `enrich` uses the vetting-seed shape
# (tier-gated, metadata-gap-filtered). Both are idempotent on pipeline_work.
#
# Runs anywhere .env.local + psql exist (archbox: ~/scoracle/scoracle-backend).
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; source .env.local; set +a
PSQL=(psql "${DATABASE_PRIVATE_URL:?DATABASE_PRIVATE_URL missing from .env.local}" -v ON_ERROR_STOP=1)

cmd="${1:?usage: investigate.sh name|enrich|status ...}"
case "$cmd" in
  name)
    who="${2:?usage: investigate.sh name \"Full Name\" SPORT}"
    sport="$(echo "${3:?usage: investigate.sh name \"Full Name\" SPORT}" | tr '[:lower:]' '[:upper:]')"
    "${PSQL[@]}" -v who="$who" -v sport="$sport" <<'SQL'
BEGIN;
INSERT INTO entity_candidates (idempotency_key, norm_name, kind_hint, sport, state, mention_count)
VALUES (lower(:'sport') || ':' || public.nrm(:'who'), public.nrm(:'who'), 'person', :'sport', 'pending', 1)
ON CONFLICT (idempotency_key) DO UPDATE SET state = 'pending', last_seen_at = NOW();

INSERT INTO pipeline_work (stage, entity_type, entity_id, sport, status, available_at, updated_at)
SELECT 'investigate_entity', 'candidate', c.id, :'sport', 'pending', NOW(), NOW()
FROM entity_candidates c
WHERE c.idempotency_key = lower(:'sport') || ':' || public.nrm(:'who')
ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
    status = 'pending', attempts = 0, available_at = NOW(), updated_at = NOW(), last_error = NULL;

SELECT c.id AS candidate_id, c.norm_name, c.state
FROM entity_candidates c
WHERE c.idempotency_key = lower(:'sport') || ':' || public.nrm(:'who');
COMMIT;
SQL
    ;;
  enrich)
    sport="$(echo "${2:?usage: investigate.sh enrich SPORT [limit]}" | tr '[:lower:]' '[:upper:]')"
    limit="${3:-50}"
    "${PSQL[@]}" -v sport="$sport" -v lim="$limit" <<'SQL'
INSERT INTO pipeline_work (stage, entity_type, entity_id, sport, status, available_at, updated_at)
SELECT 'investigate_entity', 'player', p.id, p.sport, 'pending', NOW(), NOW()
FROM players p
WHERE p.sport = :'sport' AND p.team_id IS NOT NULL
  AND (p.date_of_birth IS NULL OR p.photo_url IS NULL OR p.weight IS NULL)
ORDER BY p.tier, p.name
LIMIT :'lim'
ON CONFLICT (stage, entity_type, entity_id, sport) DO NOTHING;
SELECT count(*) AS queued FROM pipeline_work
WHERE stage = 'investigate_entity' AND status = 'pending' AND sport = :'sport';
SQL
    ;;
  status)
    sport="${2:-}"
    "${PSQL[@]}" -v sport="$sport" <<'SQL'
SELECT stage, entity_type, status, count(*) FROM pipeline_work
WHERE stage = 'investigate_entity' AND (:'sport' = '' OR sport = upper(:'sport'))
GROUP BY 1,2,3 ORDER BY 4 DESC;
SELECT r.outcome, r.rejection_reason, c.norm_name, r.finished_at::timestamp(0)
FROM acquisition_runs r JOIN entity_candidates c ON c.id = r.candidate_id
ORDER BY r.finished_at DESC LIMIT 10;
SQL
    ;;
  *)
    echo "unknown command: $cmd (name|enrich|status)" >&2
    exit 1
    ;;
esac

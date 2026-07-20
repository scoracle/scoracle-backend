#!/usr/bin/env bash
# rollout-relational-layer.sh — consolidated apply of the relational-enrichment layer
# (migrations 154–170) to a NON-LOCAL Postgres, in ledger order, idempotently.
#
# WHY THIS EXISTS: the whole relational layer (narrative graph, memory cards, transfer
# likelihood, graph extraction stage, typed links, person promotion, junction-event
# partition) shipped against the local self-hosted DB across one long session. Every
# migration is `INSERT INTO schema_migrations ... ON CONFLICT DO NOTHING`-guarded and uses
# CREATE OR REPLACE / IF NOT EXISTS, so re-applying an already-present migration is a
# no-op. This script bundles 154–170 so a fresh environment reaches parity in one command.
#
# It ALSO reminds you of the two non-SQL rollout requirements that a migration can't carry:
#   1. the cognition binary must be built from a commit that includes the graph stage
#      (rust/src/graph.rs GraphHandler) and the memory-card prompt versions;
#   2. COGNITION_STAGES must include `graph` wherever it is pinned (systemd unit +
#      any .env), or the graph stage never drains and no typed events are ever written.
#
# Usage:
#   TARGET_DATABASE_URL='postgres://…' scripts/hosting/rollout-relational-layer.sh
#   TARGET_DATABASE_URL='…' scripts/hosting/rollout-relational-layer.sh --dry-run
#
# --dry-run: print which migrations WOULD apply (not yet in the target's schema_migrations)
# and exit without writing. Safe to run against production first.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null && pwd)"
MIG_DIR="$REPO_ROOT/sql/migrations"

DRY_RUN=0
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        -h|--help) sed -n '2,33p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "rollout: unknown argument '$arg'" >&2; exit 2 ;;
    esac
done

DB="${TARGET_DATABASE_URL:-}"
if [[ -z "$DB" ]]; then
    echo "rollout: set TARGET_DATABASE_URL to the NON-LOCAL target (never defaults to the local DB)" >&2
    exit 1
fi

# The relational-layer migration span. Listed explicitly (not a glob) so the rollout set
# is auditable and a stray file in sql/migrations/ can't silently join the batch.
MIGRATIONS=(
    154_narrative_graph
    155_narrative_memory
    156_stat_matchups
    157_confirmed_seal_source_performance
    158_ground_truth_view_retroactive_seal
    159_roll_own_team_exclusion
    160_episode_backfill
    161_transfer_likelihood
    162_narrative_context_for_pair
    163_narrative_context_for_entity
    164_stat_context_for_entity
    165_graph_stage
    166_person_promotion
    167_typed_links
    168_outputs_as_memories
    169_person_team_vote_gate
    170_junction_events_partition
)

# schema_migrations must already exist (base schema). If it doesn't, this is not a
# relational-layer rollout target — it needs the full schema.sql first.
if ! psql "$DB" -tAc "SELECT to_regclass('public.schema_migrations')" | grep -q schema_migrations; then
    echo "rollout: target has no public.schema_migrations — apply sql/schema/schema.sql first" >&2
    exit 1
fi

applied="$(psql "$DB" -tAc "SELECT version FROM public.schema_migrations")"

pending=()
for m in "${MIGRATIONS[@]}"; do
    if ! grep -qxF "$m" <<<"$applied"; then
        pending+=("$m")
    fi
done

if [[ ${#pending[@]} -eq 0 ]]; then
    echo "rollout: target already at 170 — nothing to apply."
    exit 0
fi

echo "rollout: ${#pending[@]} migration(s) pending on target:"
printf '  %s\n' "${pending[@]}"

if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "rollout: --dry-run — no changes written."
    exit 0
fi

# Apply in ledger order (the pending list preserves MIGRATIONS order). Each file is its
# own BEGIN/COMMIT; ON_ERROR_STOP aborts the batch on the first failure so a partial
# rollout is loud, not silent.
for m in "${pending[@]}"; do
    f="$MIG_DIR/$m.sql"
    [[ -f "$f" ]] || { echo "rollout: missing $f" >&2; exit 1; }
    echo "==> applying $m"
    psql "$DB" -v ON_ERROR_STOP=1 -f "$f" >/dev/null
done

echo "rollout: applied ${#pending[@]} migration(s); target now at $(psql "$DB" -tAc "SELECT max(version) FROM public.schema_migrations")"
cat <<'REMINDER'

NEXT (non-SQL — a migration cannot carry these):
  1. Deploy the cognition binary from a commit >= 523dbcd (graph stage + all memory cards
     + junction-event producer). scripts/hosting/release.sh builds + restarts it.
  2. Ensure COGNITION_STAGES includes `graph`:
       scrub,graph,peak,momentum,transfers,narratives,vibe,sigil
     Check the systemd unit AND any .env / .env.local override on the target.
  3. Install the nightly relational cron (cron-narrative-links.sh @ 00:45,
     cron-stat-matchups.sh @ 03:30) — see scripts/hosting/crontab.example.
REMINDER

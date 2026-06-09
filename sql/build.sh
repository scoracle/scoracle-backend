#!/usr/bin/env bash
# Stand up a fresh database (sandbox.scoracle, fantasy.scoracle, a dev clone) that matches
# production's EXACT schema. This is the reliable path: it clones the schema rather than
# replaying migrations from the canonical sql/*.sql base (which is the BASE only — the
# rating/fantasy engine lives in migrations, and several migrations carry data-dependent
# gates that fail on an empty DB).
#
# Schema only (no data). public.schema_migrations is cloned too, so sql/migrate.sh is
# incremental from this point on the new env.
#
# Usage: ./sql/build.sh "$SOURCE_URL" "$TARGET_URL"
#   SOURCE_URL = a healthy source (prod or a known-good clone)
#   TARGET_URL = the new, empty database
set -euo pipefail

SRC="${1:?source (prod / known-good) connection string required}"
TGT="${2:?target (new env) connection string required}"

echo ">> cloning schema-only: source -> target"
pg_dump --schema-only --no-owner --no-privileges "$SRC" | psql "$TGT" -v ON_ERROR_STOP=1
echo "done — schema + schema_migrations cloned. Run sql/migrate.sh on the target for any"
echo "migrations added after the clone."

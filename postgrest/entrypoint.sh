#!/bin/sh
# PostgREST entrypoint — resolves the DB connection string using the same
# priority chain as the Go API (see go/internal/config/config.go):
#
#   PGRST_DB_URI (explicit) > NEON_DATABASE_URL_V2 > DATABASE_URL > NEON_DATABASE_URL
#
# This ensures both services always connect to the same database regardless of
# which env var the operator populates. Change the connection string in any one
# of these variables and both services pick it up.

set -e

if [ -z "$PGRST_DB_URI" ]; then
    PGRST_DB_URI="${NEON_DATABASE_URL_V2:-${DATABASE_URL:-${NEON_DATABASE_URL:-}}}"
    export PGRST_DB_URI
fi

if [ -z "$PGRST_DB_URI" ]; then
    echo "FATAL: no database URL found." >&2
    echo "Set one of: PGRST_DB_URI, NEON_DATABASE_URL_V2, DATABASE_URL, NEON_DATABASE_URL" >&2
    exit 1
fi

exec /bin/postgrest "$@"

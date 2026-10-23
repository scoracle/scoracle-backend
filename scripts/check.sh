#!/usr/bin/env bash
# CI and local runtime contracts. The explicit URL MUST be an empty disposable DB.
# The full suite writes destructive fixtures; never use a production database.
set -euo pipefail

TEST_DATABASE_URL="${1:?usage: scripts/check.sh EMPTY_DISPOSABLE_POSTGRES18_URL}"
export TEST_DATABASE_URL
unset DATABASE_URL DATABASE_PRIVATE_URL
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null && pwd)"
cd "$ROOT"

for tool in psql go cargo python3; do
    command -v "$tool" >/dev/null || { echo "Missing required tool: $tool" >&2; exit 1; }
done
psql --version | grep -Eq 'PostgreSQL\) 18\.' || {
    echo "PostgreSQL 18 client required for the checked-in baseline" >&2; exit 1;
}
version="$(psql "$TEST_DATABASE_URL" -X -Atv ON_ERROR_STOP=1 -c 'SHOW server_version_num')"
[[ "$version" =~ ^18[0-9]{4}$ ]] || {
    echo "PostgreSQL 18 disposable server required" >&2; exit 1;
}

database="$(psql "$TEST_DATABASE_URL" -X -Atv ON_ERROR_STOP=1 -c 'SELECT current_database()')"
[[ "$database" == classifier_test_coverage* ]] || {
    echo "Dedicated empty classifier_test_coverage database required" >&2; exit 1;
}

SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
./sql/build.sh "$TEST_DATABASE_URL"
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 <<'SQL'
DO $$ BEGIN
    IF (SELECT count(*) FROM coverage_schedule WHERE NOT enabled) <> 1
       OR (SELECT count(*) FROM coverage_publication WHERE edition_date IS NULL AND revision=0) <> 1
       OR (SELECT count(*) FROM classifier_reconciliation_cursor WHERE article_id=0 AND sport='') <> 1 THEN
        RAISE EXCEPTION 'Fresh baseline must initialize disabled coverage and empty publication/reconciliation state';
    END IF;
END $$;
SQL
./sql/migrate.sh "$TEST_DATABASE_URL"
./sql/migrate.sh "$TEST_DATABASE_URL" | tee "$SCRATCH/migrations.log"
grep -q 'done — 0 migration(s) applied' "$SCRATCH/migrations.log"
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 -f sql/tests/253_rating_evidence_contract.sql
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 -f sql/tests/264_rating_source_identity.sql
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 -f sql/tests/318_coverage_freeze.sql
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 -f sql/tests/319_nba_free_throw_measure.sql
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 -f sql/tests/321_qualified_visibility.sql
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 -f sql/tests/323_headline_selection.sql
psql "$TEST_DATABASE_URL" -X -v ON_ERROR_STOP=1 -f sql/tests/324_terminal_failures.sql

python3 scripts/ops/test_headline_budget_recovery.py
python3 scripts/ops/test_headline_budget_recovery.py --backlog

export SCORACLE_MEMORY_STUDY_BIN="$SCRATCH/scoracle-memory-study"
(
    cd go
    unformatted="$(gofmt -l .)"
    if [ -n "$unformatted" ]; then
        printf 'gofmt needs to run on:\n%s\n' "$unformatted" >&2
        exit 1
    fi
    go vet ./...
    go build ./...
    # go build ./... checks packages but does not emit executable dependencies.
    CGO_ENABLED=1 go build -o "$SCORACLE_MEMORY_STUDY_BIN" ./cmd/memory-study
)
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo test --manifest-path rust/Cargo.toml --lib --bins
# Live model, publisher and private-data replays require explicit operator execution.
cargo test --manifest-path rust/Cargo.toml --lib --bins -- --ignored --skip local_real_model --skip live_ --test-threads=1
(
    cd go
    go run ./cmd/validate-stmts -db "$TEST_DATABASE_URL"
    go test ./... -race
)

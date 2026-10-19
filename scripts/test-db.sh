#!/usr/bin/env bash
#
# Recreate the TEST database, and nothing else.
#
# The development database is not touched: it is the one the screens read, its
# ledger is append-only by trigger, and the only way to clean it is
# `docker compose down -v`, which takes the owner's own data. This script exists
# so that a test run never needs that decision.
#
# The schema is not applied here. `sqlx::migrate!` runs when the backend boots, and
# `scripts/test-stack.sh` always starts one, so the next test run migrates it.
#
# `DROP DATABASE` cannot run while other sessions use the database, so they are
# terminated first. Without this, an aborted run's leftover connections fail the
# drop with a message that says nothing about the cause.
#
# Usage: scripts/test-db.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> recreating the test database"

# The script goes to the container's shell on stdin, so nothing here needs to
# survive Make's or bash's quoting. The names are expanded INSIDE the container,
# where compose set them from `POSTGRES_*`; this host does not have them.
docker compose exec -T postgres sh -s <<'INNER'
set -e

testdb="${POSTGRES_DB}_test"

# Refuse to drop the development database, whatever the naming claims.
if [ "$testdb" = "$POSTGRES_DB" ]; then
  echo "refusing to drop the development database: $testdb" >&2
  exit 1
fi

psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -q <<SQL
SELECT pg_terminate_backend(pid) FROM pg_stat_activity
 WHERE datname = '$testdb' AND pid <> pg_backend_pid();
DROP DATABASE IF EXISTS "$testdb";
CREATE DATABASE "$testdb";
SQL

echo "recreated $testdb"
INNER

echo "==> the next test run applies the migrations on boot"

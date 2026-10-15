#!/usr/bin/env bash
#
# Run the backend test suites against a test database instead of the one the
# screens read.
#
# Why this exists
# ---------------
# The integration tests record real sales, products and customers, and each one
# writes a `stock_movements` row. That table is append-only by trigger, so every
# row a test run creates is permanent: there is no "delete the test data"
# afterwards, and the only way to clear the development database is to destroy
# it entirely. A run therefore leaves the owner's own sales mixed in with rows
# named `w4-rice-<uuid>`, and cleaning up means losing the real data too.
#
# Two routes need redirecting, not one. Three of the eleven tests connect with a
# pool over `DATABASE_URL`; the other seven (and six of those overlap) drive the
# HTTP API, so they write through whatever backend answers `BACKEND_BASE`. Giving
# the tests a second database is therefore only half of it — they also need a
# backend pointed at that database, which is why `backend-test` exists on port
# 3002 in docker-compose.yml.
#
# `tempox_test` lives in the same postgres container as `tempox`: no extra RAM,
# no extra port, no extra service. It persists between runs on purpose, so only
# the first run pays for creating it.
#
# Usage
# -----
#   scripts/test-backend.sh prepare   # create the database and start the backend
#   scripts/test-backend.sh run       # run every suite against it
#   scripts/test-backend.sh reset     # drop and recreate it (after schema work)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TEST_DB="${TEST_DB:-tempox_test}"
TEST_PORT="${TEST_PORT:-3002}"

# Credentials and port come from `.env`, which is the file docker-compose reads
# to create the database; a fresh clone has not copied `.env.example` yet, so
# fall back to the compose file's own defaults. Real environment variables win.
env_value() {
  [ -f "$REPO_ROOT/.env" ] && sed -n "s/^$1=//p" "$REPO_ROOT/.env" | tail -1
}
first_of() {
  for value in "$@"; do
    if [ -n "$value" ]; then
      printf '%s' "$value"
      return
    fi
  done
}

DB_USER="$(first_of "${POSTGRES_USER:-}" "$(env_value POSTGRES_USER)" skilluv)"
DB_PASSWORD="$(first_of "${POSTGRES_PASSWORD:-}" "$(env_value POSTGRES_PASSWORD)" skilluv)"
DB_PORT="$(first_of "${POSTGRES_PORT:-}" "$(env_value POSTGRES_PORT)" 5432)"

psql_admin() {
  docker compose exec -T postgres \
    psql -U "$DB_USER" -d postgres -v ON_ERROR_STOP=1 -t -A -c "$1"
}

require_postgres() {
  if ! docker compose ps postgres --status running --format '{{.Name}}' | grep -q .; then
    echo "error: the postgres service is not running." >&2
    echo "       start it first: docker compose up -d postgres" >&2
    exit 1
  fi
}

database_exists() {
  [ "$(psql_admin "SELECT 1 FROM pg_database WHERE datname = '${TEST_DB}';")" = "1" ]
}

start_backend() {
  echo "--- test backend on :${TEST_PORT} (database ${TEST_DB}) ---"
  # The backend applies every migration on boot via `sqlx::migrate!`, so starting
  # it is what migrates this database. Nothing else has to know how — and a
  # second migrator is not an option, because the binary reads
  # `_sqlx_migrations` and would re-run migrations it does not find there.
  docker compose --profile test up -d backend-test >/dev/null

  ready=""
  for _ in $(seq 1 60); do
    if curl -sf "http://localhost:${TEST_PORT}/health" >/dev/null; then
      ready=1
      break
    fi
    sleep 1
  done

  if [ -z "$ready" ]; then
    echo "error: the test backend did not become healthy in 60s." >&2
    docker compose --profile test logs backend-test 2>&1 | tail -20 >&2 || true
    exit 1
  fi
}

command="${1:-run}"

case "$command" in
prepare)
  require_postgres
  if database_exists; then
    echo "--- ${TEST_DB} already exists (persistent; use 'reset' to recreate) ---"
  else
    echo "--- creating ${TEST_DB} ---"
    psql_admin "CREATE DATABASE ${TEST_DB};" >/dev/null
  fi
  start_backend
  echo "ready: run 'make test-backend'"
  ;;

reset)
  require_postgres
  echo "--- recreating ${TEST_DB} ---"
  docker compose --profile test stop backend-test >/dev/null 2>&1 || true
  psql_admin "DROP DATABASE IF EXISTS ${TEST_DB} WITH (FORCE);" >/dev/null
  psql_admin "CREATE DATABASE ${TEST_DB};" >/dev/null
  start_backend
  ;;

run)
  require_postgres
  if ! database_exists; then
    echo "error: the ${TEST_DB} database does not exist." >&2
    echo "       create it once with: make test-db-prepare" >&2
    exit 1
  fi
  start_backend

  # The tests reach the database two ways and both have to leave the development
  # stack: the pool over DATABASE_URL, and the HTTP API over BACKEND_BASE. Their
  # own defaults point at the development stack (`localhost:5432/tempox`,
  # `localhost:3001`), which is exactly what must not be written to.
  export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_PORT}/${TEST_DB}"
  export BACKEND_BASE="http://localhost:${TEST_PORT}"

  echo "--- cargo test --all-features -- --include-ignored --test-threads=1 ---"
  cd "$REPO_ROOT/backend"
  cargo test --all-features -- --include-ignored --test-threads=1
  ;;

*)
  echo "usage: $0 {prepare|run|reset}" >&2
  exit 2
  ;;
esac

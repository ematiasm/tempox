#!/usr/bin/env bash
#
# Recreate the TEST database, and nothing else.
#
# The development database is not touched: it is the one the screens read, its
# ledger is append-only by trigger, and the only way to clean it is
# `docker compose down -v`, which takes the owner's own data. This script exists
# so that a test run never needs that decision.
#
# The target comes from `TEST_DATABASE_URL`, and from nowhere else. Deriving it as
# `${POSTGRES_DB}_test` would be a second source of truth: a caller pointing
# TEST_DATABASE_URL at a different database would get a cheerful "recreated"
# message for the wrong one, and the database the suite actually uses would stay
# stale or absent.
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

# An explicitly exported value must win: bash's `. file` OVERWRITES the
# environment, unlike `dotenvy`, so the caller's choice is restored afterwards.
# Same rule as scripts/test-stack.sh, for the same reason.
_CALLER_TEST_DATABASE_URL="${TEST_DATABASE_URL:-}"
if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi
if [ -n "$_CALLER_TEST_DATABASE_URL" ]; then
  TEST_DATABASE_URL="$_CALLER_TEST_DATABASE_URL"
fi

if [ -z "${TEST_DATABASE_URL:-}" ]; then
  echo "error: TEST_DATABASE_URL is not set." >&2
  echo "       Add it to .env (see .env.example) and run again." >&2
  exit 1
fi

# ── Parsing, without ever echoing the URL ─────────────────────────────
#
# A URL is `scheme://[user[:password]@]host[:port][/path]`. The password lives in
# the authority, so ANY error message that prints the raw value, the authority, or
# the last path segment of an authority-only URL publishes the password into the
# terminal and into CI logs. That is not hypothetical: two review rounds found it,
# in messages this script used to print.
#
# So the rules below are structural rather than defensive:
#
#   1. Strip the credentials BEFORE anything else looks at the value.
#   2. Never interpolate the URL, the authority or the path into a message. Errors
#      name what is wrong and what to do, not what the value was.
#   3. Reconstruct the components deliberately, and validate the database name
#      against a strict charset, so a stray ':' or '@' cannot survive as a name.

# Query string and fragment are irrelevant to the database name.
_url="${TEST_DATABASE_URL%%[?#]*}"

# Separate the authority from the path. A URL with no '/' after '://' has no path
# at all, which is a configuration error, not a database name.
_rest="${_url#*://}"
if [ "$_rest" = "$_url" ]; then
  echo "error: TEST_DATABASE_URL is not a postgres URL." >&2
  echo "       Expected 'postgres://user:password@host:port/<name>_test'." >&2
  exit 1
fi
case "$_rest" in
  */*) : ;;
  *)
    echo "error: TEST_DATABASE_URL has no database name at the end." >&2
    echo "       Expected 'postgres://user:password@host:port/<name>_test'." >&2
    echo "       The value is not printed: it carries credentials." >&2
    exit 1
    ;;
esac

_authority="${_rest%%/*}"
_path="${_rest#*/}"
# The path may carry further slashes; the name is its last segment.
TESTDB="${_path##*/}"

# Credentials are dropped here, and never used: this script reaches postgres
# through `docker compose exec`, which authenticates with the container's own
# POSTGRES_* environment. Only the host and port matter, and only to refuse a
# target this script cannot reach.
_hostport="${_authority##*@}"
_host="${_hostport%%:*}"
_port="${_hostport#*:}"
if [ "$_port" = "$_hostport" ]; then
  _port=""
fi

# The database name must be a plain identifier. This is what makes the earlier
# credential leak impossible to reintroduce: an authority fragment such as
# `user:secret@host:5432` contains ':' and '@', so it can never be accepted as a
# name, and the rejection message below prints the name only after this check.
case "$TESTDB" in
  *[!A-Za-z0-9_]*|"")
    echo "error: TEST_DATABASE_URL's database name is not a plain identifier." >&2
    echo "       Expected 'postgres://user:password@host:port/<name>_test'." >&2
    echo "       The name is not printed when it fails this check." >&2
    exit 1
    ;;
esac

case "$TESTDB" in
  *_test) : ;;
  *)
    # Safe to print: the case above proved it is `[A-Za-z0-9_]+`.
    echo "error: TEST_DATABASE_URL names '$TESTDB', which is not a test database." >&2
    echo "       It must end in '_test'; the development database is never reset." >&2
    exit 1
    ;;
esac

# This script runs psql inside the container, so it can only reset a database on
# this compose postgres. A URL naming another server would have it drop and create
# a same-named LOCAL database, print success, and leave the configured one
# untouched. The port is checked too: `localhost` on a different port is a
# different server.
SERVICE="${POSTGRES_SERVICE:-postgres}"
EXPECTED_PORT="${POSTGRES_PORT:-5432}"
case "$_host" in
  localhost|127.0.0.1|"$SERVICE") : ;;
  *)
    echo "error: TEST_DATABASE_URL does not point at this compose postgres." >&2
    echo "       This script runs psql inside the postgres container, so it can" >&2
    echo "       only reset a database on that server. Point it at localhost:$EXPECTED_PORT" >&2
    echo "       or at the compose service name '$SERVICE'." >&2
    exit 1
    ;;
esac
if [ -n "$_port" ] && [ "$_port" != "$EXPECTED_PORT" ]; then
  echo "error: TEST_DATABASE_URL names port $_port, but this stack publishes $EXPECTED_PORT." >&2
  echo "       A database on another port is another server, which this script" >&2
  echo "       cannot reach through 'docker compose exec'." >&2
  exit 1
fi

# Refuse to drop the development database, whatever the caller asked for.
if [ "$TESTDB" = "${POSTGRES_DB:-}" ]; then
  echo "error: TEST_DATABASE_URL names the development database '$POSTGRES_DB'." >&2
  echo "       It must end in '_test'; the ledger is append-only and is never reset." >&2
  exit 1
fi

echo "==> recreating the test database: $TESTDB"

# The script goes to the container's shell on stdin, so nothing here has to
# survive Make's or bash's quoting. `POSTGRES_USER` is expanded INSIDE the
# container, where compose set it; this host does not have it.
docker compose exec -T -e TESTDB="$TESTDB" postgres sh -s <<'INNER'
set -e

# Refuse to drop the development database, whatever the caller asked for.
if [ "$TESTDB" = "$POSTGRES_DB" ]; then
  echo "refusing to drop the development database: $TESTDB" >&2
  exit 1
fi

psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -q <<SQL
SELECT pg_terminate_backend(pid) FROM pg_stat_activity
 WHERE datname = '$TESTDB' AND pid <> pg_backend_pid();
DROP DATABASE IF EXISTS "$TESTDB";
CREATE DATABASE "$TESTDB";
SQL

echo "recreated $TESTDB"

# Leftovers from `#[sqlx::test]`. Each converted test creates its own database,
# named `_sqlx_test_<sha512 of the test path>`, and drops it when it finishes. A
# run that is killed mid-test leaves it behind, and the name is stable per test, so
# the next run reuses it with whatever state the dead run had.
#
# This is the only place that sweeps them, and it is safe: the prefix is reserved
# by the framework, and nothing else in this project names a database that way.
stale="$(psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -tA -c \
  "SELECT datname FROM pg_database WHERE datname LIKE '\_sqlx\_test\_%';")"

if [ -n "$stale" ]; then
  while IFS= read -r d; do
    [ -n "$d" ] || continue
    # Two statements, two calls: DROP DATABASE cannot run inside a transaction
    # block, and psql wraps a multi-statement -c in one.
    psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -q -c \
      "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
        WHERE datname = '$d' AND pid <> pg_backend_pid();" >/dev/null
    psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d postgres -q -c \
      "DROP DATABASE IF EXISTS \"$d\";" >/dev/null
    echo "dropped leftover $d"
  done <<EOF
$stale
EOF
fi
INNER

echo "==> the next test run applies the migrations on boot"

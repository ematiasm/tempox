#!/usr/bin/env bash
#
# Run a command against a backend that belongs to the test run.
#
# Why this exists, in one sentence: the tests and the backend they talk to must
# not be able to point at different databases, so the run owns the process
# instead of borrowing one.
#
# The shape:
#
#   1. Build the branch's binary (or reuse it when up to date).
#   2. Refuse to start if anything already answers on the test port. A suite that
#      talks to a leftover server is exactly the defect this guards against.
#   3. Start the binary with DATABASE_URL set to the TEST database.
#   4. Wait for /health. Reaching it proves the connection landed: a missing or
#      unreachable database stops the binary at boot, before it can serve.
#   5. Run the caller's command with BACKEND_BASE and PUBLIC_API_BASE pointing at
#      this backend.
#   6. Kill it and wait for the exit, so connections close before anything drops
#      the database.
#
# The development backend on :3001 is never stopped, restarted or disturbed. That
# is the whole reason for a dedicated port: `docker compose up` and this script
# can be true at the same time.
#
# Usage: scripts/test-stack.sh <command> [args...]
# Env:   TEST_DATABASE_URL  required, read from .env when absent
#        TEST_BACKEND_PORT  optional, default 3099

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${TEST_BACKEND_PORT:-3099}"
BASE="http://localhost:${PORT}"

if [ "$#" -eq 0 ]; then
  echo "usage: $0 <command> [args...]" >&2
  exit 2
fi

# `.env` is the single source of truth for the test database, but an explicitly
# exported value must win: bash's `. file` OVERWRITES existing variables, unlike
# `dotenvy`, which preserves them. Without the restore below, a caller asking for a
# specific database is silently overridden by `.env`, which makes the guards
# unmeasurable and hides the very mistake they exist to catch.
_CALLER_TEST_DATABASE_URL="${TEST_DATABASE_URL:-}"
if [ -f "$ROOT/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  . "$ROOT/.env"
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

# The development database must never be the target of a test run. The names come
# from this repository's own `.env`, so this is a belt-and-braces check: the
# primary guard is that `TEST_DATABASE_URL` must end in `_test`.
case "$TEST_DATABASE_URL" in
  *_test|*_test?*) : ;;
  *)
    echo "error: TEST_DATABASE_URL does not name a test database." >&2
    echo "       It is '$TEST_DATABASE_URL'; it must end in '_test'." >&2
    echo "       Tests never write to the development database: its ledger is" >&2
    echo "       append-only, so a test row there cannot be deleted individually." >&2
    exit 1
    ;;
esac

if curl -sf "$BASE/health" >/dev/null 2>&1; then
  echo "error: something is already answering on $BASE." >&2
  echo "       A suite would talk to it instead of this run's build." >&2
  echo "       Stop it, or set TEST_BACKEND_PORT to a free port." >&2
  exit 1
fi

echo "==> building the branch's binary"
# Built from `backend/`, never with `--manifest-path` from the root: cargo reads
# `.cargo/config.toml` from the CURRENT DIRECTORY up, not from the manifest's, so
# building from the root silently skips `backend/.cargo/config.toml` — and that is
# where `SQLX_OFFLINE=true` lives. Without it the `sqlx::query!` macros open a live
# connection at compile time and fail with "relation ... does not exist" for every
# table, which reads like a broken schema and is a misplaced working directory.
( cd "$ROOT/backend" && cargo build --all-features )

BIN="$ROOT/backend/target/debug/tempox"
if [ ! -x "$BIN" ]; then
  echo "error: expected the binary at $BIN and it is not executable." >&2
  exit 1
fi

LOG="$(mktemp -t tempox-test-backend.XXXXXX.log)"
# shellcheck disable=SC2064
trap 'kill "${SERVER_PID:-}" 2>/dev/null || true; wait "${SERVER_PID:-}" 2>/dev/null || true' EXIT

echo "==> starting the test backend on :$PORT"
DATABASE_URL="$TEST_DATABASE_URL" \
  BACKEND_PORT="$PORT" \
  RUST_LOG="${RUST_LOG:-warn}" \
  "$BIN" >"$LOG" 2>&1 &
SERVER_PID=$!

ready=""
for _ in $(seq 1 60); do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    break
  fi
  if curl -sf "$BASE/health" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done

if [ -z "$ready" ]; then
  echo "error: the test backend did not become healthy on $BASE in 60s." >&2
  echo "       It was started with DATABASE_URL from TEST_DATABASE_URL." >&2
  echo "       If the database does not exist, create it: make test-db" >&2
  echo "--- backend log ---" >&2
  cat "$LOG" >&2
  exit 1
fi

echo "==> backend healthy; running: $*"
status=0
# `DATABASE_URL` is overridden for the CHILD command too, not only for the server
# above. The 14 pool-only tests open their own connection and read this variable;
# leaving them on the development database while the backend served the test one
# is precisely the desync this script exists to make impossible.
DATABASE_URL="$TEST_DATABASE_URL" \
  BACKEND_BASE="$BASE" \
  PUBLIC_API_BASE="$BASE" \
  "$@" || status=$?

# Explicit teardown rather than relying on the trap alone: the caller's exit code
# is the one that matters, and the server must be gone before the next step.
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
SERVER_PID=""

if [ "$status" -ne 0 ]; then
  echo "--- backend log (tail) ---" >&2
  tail -n 40 "$LOG" >&2 || true
fi

exit "$status"

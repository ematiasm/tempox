.PHONY: dev test test-unit test-frontend test-backend test-e2e test-db \
        lint build clean fmt sqlx-prepare sqlx-check help

help:
	@echo "Targets:"
	@echo "  dev             — start the whole stack (postgres + backend + frontend)"
	@echo ""
	@echo "  test            — every level: unit + integration + e2e"
	@echo "  test-unit       — backend + frontend unit tests (needs nothing running)"
	@echo "  test-frontend   — frontend unit tests only (needs nothing running)"
	@echo "  test-backend    — backend unit + integration, against the TEST database"
	@echo "  test-e2e        — Playwright, against the TEST database"
	@echo "  test-db         — drop and recreate the TEST database (disposable)"
	@echo ""
	@echo "  sqlx-prepare    — regenerate .sqlx after changing a query (needs postgres)"
	@echo "  sqlx-check      — verify .sqlx matches the current queries (needs postgres)"
	@echo "  lint            — clippy + eslint + svelte-check"
	@echo "  build           — production build for both backend and frontend"
	@echo "  fmt             — auto-format code"
	@echo "  clean           — remove build artifacts"
	@echo ""
	@echo "Test layout, and why it is this way: docs/en/testing.md"

dev:
	docker compose up --build

# ── Test plumbing ────────────────────────────────────────────────────
#
# Three levels, and they are not a ladder from cheap to expensive: each covers
# something the others cannot see. `docs/en/testing.md` is the authority.
#
# The tests never write to the development database. `tempox` is the database the
# screens read, and the ledger is append-only by trigger, so a row a test left
# there could not be removed individually — only by discarding the whole volume
# with `docker compose down -v`, which takes the owner's own data with it.
#
# So `tempox_test` lives in the SAME postgres container, on the same volume, next
# to `tempox`. Same stack, same image, one container. `make test-db` recreates it;
# it is disposable and needs nobody's permission.
#
# Neither level needs a backend from outside any more. `make test-backend` gives each
# test its own database and lets it start its own process (F4); `make test-e2e` lets
# Playwright start the branch's binary on `:3099` against the test database (F5).
# The development backend on `:3001` is never stopped or disturbed, and there is no
# way for the suite and the server to end up on different databases: the run owns
# the process. That property — not a check — is what prevents the desync.
test: test-unit test-backend test-e2e

test-unit: test-frontend
	# `--lib` and `--bins` both: the unit tests live in the library now (`src/lib.rs`,
	# plus `day` and `money`), while `--bins` still covers anything added to the
	# binary. Running only `--bins` silently reported "0 tests" once the crate grew
	# a `[lib]` target, which is a green that proves nothing.
	cd backend && cargo test --all-features --lib --bins

test-frontend:
	cd frontend && npm run test:unit -- --run

# No external backend, and no `--test-threads=1`.
#
# Both went in F4. Every test now owns what it needs: `#[sqlx::test]` creates a
# database per test, and the ones that need a real socket start `CARGO_BIN_EXE_tempox`
# themselves on an ephemeral port (see `common::spawn_backend`). Nothing is shared, so
# nothing has to be serialized, and no separate process has to be started first.
#
# This is what the `test-stack.sh` backend used to do, and why `test-stack.sh` is no
# longer in this path: it exists for the levels that legitimately borrow a process —
# the e2e suite, which drives the real screen against an external backend.
#
# `DATABASE_URL` is what `#[sqlx::test]` reads — `sqlx-postgres` calls
# `dotenvy::var("DATABASE_URL")` — not `TEST_DATABASE_URL`. Exporting only the
# latter meant the suite silently took `DATABASE_URL` from `.env`, which names the
# DEVELOPMENT database, and on a clean checkout with no `.env` it failed outright
# with "DATABASE_URL must be set". Both are mapped here, from `TEST_DATABASE_URL`,
# so the database under test is the test one whether or not a `.env` exists.
#
# It depends on `test-db`, which is what the baseline got as a side effect of
# starting the stack: `make test-db` recreates the test database AND sweeps the
# `_sqlx_test_*` leftovers a killed run leaves behind. Dropping the external
# backend without carrying this over would have made the target require a database
# a fresh checkout does not have — R3-001 and R3-002, both CRITICAL, both right.
#
# `TEST_DATABASE_URL` is resolved ONCE, as a make variable, before any prerequisite
# runs — `test-db` needs it too, and a default set inside this recipe arrives too
# late.
#
# Three sources, most specific first: the caller's value, then `.env`, then
# `.env.example`. The last one is what makes a fresh checkout work: it is the file a
# new checkout copies, so it names the URL that checkout's database will actually
# have. Hardcoding credentials here instead was wrong within a minute of trying it —
# this repository's volume was created with a password the compose defaults do not
# produce, and the run failed with "password authentication failed for user tempox".
# Exiting when the value was absent was worse still: the baseline reached the tests
# (R3-001, third round).
#
# The recipe still refuses a URL that is not a test database. That is the one failure
# worth stopping for: the development ledger is append-only and tests must never
# reach it.
# Each file is resolved SEPARATELY, then combined. Passing both to one `sed` and
# keeping `tail -1` looked equivalent and was not: the last match comes from the
# last FILE, so `.env.example` silently overrode a customized `.env` — the reverse
# of the precedence this comment claims. Measured with two files holding different
# values: the wrong one won. Resolving each separately is what makes "caller, then
# .env, then .env.example" true rather than aspirational.
TEST_DATABASE_URL_ENV := $(shell sed -n 's/^TEST_DATABASE_URL=//p' .env 2>/dev/null | tail -1)
TEST_DATABASE_URL_EXAMPLE := $(shell sed -n 's/^TEST_DATABASE_URL=//p' .env.example 2>/dev/null | tail -1)
TEST_DATABASE_URL ?= $(or $(TEST_DATABASE_URL_ENV),$(TEST_DATABASE_URL_EXAMPLE))
test-backend: export TEST_DATABASE_URL := $(TEST_DATABASE_URL)
test-backend: test-db
	@url="$$TEST_DATABASE_URL"; \
	case "$$url" in \
		*_test|*_test\?*) : ;; \
		*) echo "error: refusing to run the suite against '$$url': it must end in '_test'." >&2; \
		   echo "       The development database is append-only; tests never touch it." >&2; \
		   exit 1 ;; \
	esac; \
	cd backend && TEST_DATABASE_URL="$$url" DATABASE_URL="$$url" \
		cargo test --all-features -- --include-ignored

# The binary the e2e level serves. Separate on purpose: `cargo build` from
# `backend/`, never `--manifest-path` from the root, because cargo reads
# `.cargo/config.toml` from the current directory up and that is where
# `SQLX_OFFLINE=true` lives.
test-backend-binary:
	cd backend && cargo build --all-features

# Playwright builds and serves the frontend itself on :4173, and reaches the
# backend through PUBLIC_API_BASE, which the harness sets. Chromium is a
# machine-level artifact, not a repo one, so its absence is reported in one line
# instead of as a wall of context launches that never happen.
#
# The check reads the browser binary rather than `playwright install --dry-run`:
# the dry run prints where it WOULD install and exits 0 either way, so it cannot
# tell an installed browser from a missing one. Measured, not assumed.
# Playwright now owns the backend (see `frontend/playwright.config.ts`): it starts
# the branch's binary against the test database, waits for `/health`, runs the
# frontend build, serves the preview, and tears all of it down. So this target no
# longer needs a backend from `scripts/test-stack.sh`.
#
# It still prepares the database (`test-db`, which recreates it and sweeps the
# `_sqlx_test_*` leftovers) and still exports `PUBLIC_API_BASE`, because `vite`
# inlines that value into the bundle at BUILD time and Playwright's `webServer.env`
# only reaches the process it spawns. Measured: a build given
# `PUBLIC_API_BASE=http://probe-1234` contains that literal in
# `_app/immutable/chunks/*.js`.
#
# Building the binary here rather than inside Playwright's command keeps the build
# failure legible: a compile error should read as a compile error, not as
# "webServer failed to start".
test-e2e: test-db
	@ls ~/.cache/ms-playwright/chromium-*/chrome-linux*/chrome >/dev/null 2>&1 || { \
		echo "error: Playwright's chromium is not installed." >&2; \
		echo "       install it once: cd frontend && npx playwright install chromium" >&2; \
		exit 1; \
	}
	@$(MAKE) --no-print-directory test-backend-binary
	cd frontend && TEST_DATABASE_URL='$(TEST_DATABASE_URL)' \
		PUBLIC_API_BASE="http://localhost:$${TEST_BACKEND_PORT:-3099}" \
		npx playwright test

# Recreating the test database. The SQL and the safety checks live in
# `scripts/test-db.sh`: a heredoc cannot span Make's recipe lines, because Make
# runs each line in its own shell. See docs/en/testing.md.
test-db:
	@./scripts/test-db.sh

# `make build` and `make lint` need no database at all. `backend/.cargo/config.toml`
# turns on `SQLX_OFFLINE`, and `backend/.sqlx/` carries the metadata the
# `sqlx::query!` macros verify against.
#
# The two targets below are the exception: they answer the compiler's questions by
# asking a live database. `DATABASE_URL` in `.env` already names `localhost`, which
# is the host's view of the published port, so no rewriting is needed. Override
# with `make sqlx-prepare DATABASE_URL=postgres://...` for a different layout.
HOST_DATABASE_URL ?= $(shell sed -n 's|^DATABASE_URL=||p' .env 2>/dev/null | tail -1)

sqlx-prepare:
	@test -n "$(HOST_DATABASE_URL)" || { echo "error: no DATABASE_URL; copy .env.example to .env, or pass DATABASE_URL=..." >&2; exit 1; }
	cd backend && SQLX_OFFLINE=false DATABASE_URL='$(HOST_DATABASE_URL)' cargo sqlx prepare -- --all-targets

sqlx-check:
	@test -n "$(HOST_DATABASE_URL)" || { echo "error: no DATABASE_URL; copy .env.example to .env, or pass DATABASE_URL=..." >&2; exit 1; }
	cd backend && SQLX_OFFLINE=false DATABASE_URL='$(HOST_DATABASE_URL)' cargo sqlx prepare --check -- --all-targets

lint:
	cd backend && cargo clippy --all-targets -- -D warnings
	cd frontend && npm run lint && npm run check

build:
	cd backend && cargo build --release
	cd frontend && npm run build

fmt:
	cd backend && cargo fmt
	cd frontend && npm run format

clean:
	cd backend && cargo clean
	cd frontend && rm -rf node_modules .svelte-kit build .vite dist

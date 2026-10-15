.PHONY: dev test test-frontend test-backend test-db-prepare test-db-reset lint build clean fmt sqlx-prepare sqlx-check help

help:
	@echo "Targets:"
	@echo "  dev             — start the whole stack (postgres + backend + frontend)"
	@echo "  test            — run every suite (backend tests use the test database)"
	@echo "  test-frontend   — frontend unit tests (needs nothing running)"
	@echo "  test-backend    — backend unit + integration tests, on the test database"
	@echo "  test-db-prepare — create the test database once, after cloning"
	@echo "  test-db-reset   — recreate the test database from scratch"
	@echo "  sqlx-prepare    — regenerate .sqlx after changing a query (needs postgres)"
	@echo "  sqlx-check      — verify .sqlx matches the current queries (needs postgres)"
	@echo "  lint            — clippy + eslint + svelte-check"
	@echo "  build           — production build for both backend and frontend"
	@echo "  fmt             — auto-format code"
	@echo "  clean           — remove build artifacts"

dev:
	docker compose up --build

# `make build` and `make lint` need no database at all. `backend/.cargo/config.toml`
# turns on `SQLX_OFFLINE`, and `backend/.sqlx/` carries the metadata the
# `sqlx::query!` macros verify against. Before that cache existed, every `cargo`
# command needed PostgreSQL reachable, and `.env` names the `postgres` service —
# a hostname that only resolves inside the compose network, so all three targets
# died on the host before doing any work.
#
# The integration tests are the part that genuinely needs a live database, and
# they write: real sales, products and stock movements, the last of which is
# append-only. They run against `tempox_test` — a second database inside the same
# postgres container — through a second backend on port 3002, so none of those
# rows land in the database the screens read.
#
# You have to have created it once: `make test-db-prepare`.
test: test-frontend test-backend

test-frontend:
	cd frontend && npm run test:unit -- --run

test-backend:
	./scripts/test-backend.sh run

test-db-prepare:
	./scripts/test-backend.sh prepare

test-db-reset:
	./scripts/test-backend.sh reset

# The offline cache is what every other cargo command trusts. When a query
# changes and the cache is not regenerated, compilation fails on a stale type
# instead of on the query — a confusing error to chase. These two targets are the
# fix: one regenerates, the other proves the committed cache is current. CI runs
# the check, so a stale cache cannot merge.
#
# `SQLX_OFFLINE=false` opts these single commands back into the live database;
# cargo's `[env]` never overrides a variable that is already set.
#
# These two are the only commands that need a connection, and they run on the
# host — where the `postgres` service name in `.env` does not resolve. So the
# host is rewritten to localhost. Override with `make sqlx-prepare
# DATABASE_URL=postgres://...` for a different layout.
HOST_DATABASE_URL ?= $(shell sed -n 's|^DATABASE_URL=||p' .env 2>/dev/null | sed 's|@postgres:|@localhost:|' | tail -1)

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

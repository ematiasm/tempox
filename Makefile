.PHONY: dev test test-frontend test-backend lint build clean fmt sqlx-prepare sqlx-check help

help:
	@echo "Targets:"
	@echo "  dev             — start the whole stack (postgres + backend + frontend)"
	@echo "  test            — run every suite (needs the dev stack up)"
	@echo "  test-frontend   — frontend unit tests (needs nothing running)"
	@echo "  test-backend    — backend unit + integration tests (needs the dev stack up)"
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
# The integration tests need a live database, and they write to it: real sales,
# products and stock movements, the last of which is append-only by trigger. They
# run against the same `tempox` database the screens read, on purpose: one stack,
# one database, one backend. Rows a run creates are therefore visible in the app,
# and `docker compose down -v` is how they are cleared. CI does the same thing —
# a single postgres and a single backend it starts itself.
#
# They reach that stack from the outside, over the ports compose publishes:
# `localhost:3001` for the API and `localhost:5432` for the pool. So the stack has
# to be up first. The guard below says so in one line instead of leaving eight
# tests to fail with a connection error.
test: test-frontend test-backend

test-frontend:
	cd frontend && npm run test:unit -- --run

test-backend:
	@curl -sf http://localhost:3001/health >/dev/null || { \
		echo "error: the backend is not answering on http://localhost:3001/health." >&2; \
		echo "       start the stack first: make dev" >&2; \
		exit 1; \
	}
	cd backend && cargo test --all-features -- --include-ignored --test-threads=1

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

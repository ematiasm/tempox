# Getting started — Tempox

## Prerequisites

- Docker + Docker Compose 2.24+
- (Optional, for local dev without Docker) Rust 1.89+, Node 24+, PostgreSQL 18

## First run

```bash
git clone https://github.com/ematiasm/tempox.git
cd tempox
cp .env.example .env
docker compose up --build
```

The first build takes ~3-5 minutes (Rust compile + npm install). Subsequent runs are cached.

Once up:

- Frontend at <http://localhost:5173>
- Backend health at <http://localhost:3001/health>
- Adminer DB UI at <http://localhost:8080> (server `postgres`; user, password and database are the `POSTGRES_USER`, `POSTGRES_PASSWORD` and `POSTGRES_DB` values from your `.env`)

## Making changes

- **Backend** (`backend/`): edit sources under `backend/src/`. In dev, restart the container (`docker compose restart backend`) or rebuild.
- **Frontend** (`frontend/`): Vite HMR is enabled; changes hot-reload automatically.
- **DB schema**: add a new migration file `backend/migrations/000X_*.sql`. `sqlx::migrate!` runs it on boot.

## Running tests

Create the test database once, then run both suites:

```bash
make test-db-prepare
make test
```

`make test` runs the frontend unit tests and the backend unit + integration
tests. Nothing else has to be running: `make test-db-prepare` starts the postgres
service if it is down.

### Where the tests write

The integration tests record real sales, products and stock movements, and
`stock_movements` is append-only by trigger. Every row a run creates is
therefore permanent: there is no "delete the test data" afterwards, and the only
way to clear the development database would be to destroy it entirely, losing
the real sales with it.

So they run against `tempox_test` instead — a second database inside the same
postgres container, reached through a second backend on port 3002. Two facts
make the second backend necessary, not optional: seven of the eleven tests drive
the HTTP API rather than connecting directly, so pointing `DATABASE_URL`
somewhere else is not enough on its own.

The database persists between runs, so only the first run pays for creating it.
It accumulates rows like any other database; when that bothers you (or after
touching the schema), recreate it:

```bash
make test-db-reset
```

### Why compiling needs no database

The `sqlx::query!` macros check their SQL while the crate compiles. `backend/.sqlx/`
carries the metadata they need, and `backend/.cargo/config.toml` turns on
`SQLX_OFFLINE`, so `make build` and `make lint` work with PostgreSQL stopped —
which matters, because `.env` names the `postgres` service, a hostname that only
resolves inside the compose network.

After changing a query, refresh that cache:

```bash
make sqlx-prepare
```

Compiling against a stale cache fails on a type that no longer matches, in a
place that names the query rather than the cause. CI runs `sqlx prepare --check`
so a stale cache cannot merge.

### One suite at a time

```bash
make test-frontend   # frontend unit tests, needs nothing running
make test-backend    # backend unit + integration, on the test database
```

## Deploying

Tempox is designed for self-hosted deployment via [Coolify](https://coolify.io/). Point Coolify at the repo, and its Docker Compose recipe will build both services.

For production, at minimum change:

- `POSTGRES_PASSWORD` to a strong secret
- `PUBLIC_API_BASE` to your public backend URL
- Restrict CORS in `backend/src/main.rs`

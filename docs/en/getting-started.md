# Getting started — Tempox

## Prerequisites

- Docker + Docker Compose 2.24+
- (Optional, for local dev without Docker) Rust 1.89+, Node 26+, PostgreSQL 18

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

The integration tests need the stack running, so start it first:

```bash
make dev      # postgres + backend + frontend
make test
```

`make test` runs the frontend unit tests and the backend unit + integration
tests. The integration tests talk to the API at `localhost:3001` and to the
database at `localhost:5432` — the ports compose publishes — so they need that
stack up. If the backend is not answering, `make test-backend` says so and stops
instead of letting eight tests fail with a connection error.

### Where the tests write

The tests record real sales, products and stock movements, and `stock_movements`
is append-only by trigger. Every row a run creates is permanent: there is no
"delete the test data" afterwards.

They run against the same `tempox` database the screens read — one stack, one
database, one backend. Two consequences worth knowing:

- The rows show up in the app. After a run you will see products named
  `w4-rice-<uuid>` and their sales in the catalogue and the sales list. They also
  cannot be deleted individually: the ledger guards reject that.
- `docker compose down -v` is how they go away. That drops the `pgdata` volume,
  so your own data goes with it.

CI does the same thing: a single postgres and a single backend, started by the
job itself.

### One suite at a time

```bash
make test-frontend   # frontend unit tests, needs nothing running
make test-backend    # backend unit + integration, needs the dev stack up
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

## Deploying

Tempox is designed for self-hosted deployment via [Coolify](https://coolify.io/). Point Coolify at the repo, and its Docker Compose recipe will build both services.

For production, at minimum change:

- `POSTGRES_PASSWORD` to a strong secret
- `PUBLIC_API_BASE` to your public backend URL
- Restrict CORS in `backend/src/main.rs`

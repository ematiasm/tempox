# Getting started — Tempox

## Prerequisites

- Docker + Docker Compose 2.24+
- (Optional, for local dev without Docker) Rust 1.89+, Node 26+, PostgreSQL 18

## First run

```bash
git clone https://github.com/ematiasm/tempox.git
cd tempox
cp .env.example .env
docker compose up -d                        # the database and adminer
cd backend && cargo run                     # API on :3001
cd frontend && npm install && npm run dev   # UI on :5173
```

Nothing is built as an image for development: `docker-compose.yml` holds the
database and adminer only. The first `cargo run` compiles the backend (~3-5
minutes) and `npm install` fetches the frontend's dependencies; both are cached
afterwards.

Once up:

- Frontend at <http://localhost:5173>
- Backend health at <http://localhost:3001/health>
- Adminer DB UI at <http://localhost:8080> (server `postgres`; user, password and database are the `POSTGRES_USER`, `POSTGRES_PASSWORD` and `POSTGRES_DB` values from your `.env`)

## Making changes

- **Backend** (`backend/`): edit sources under `backend/src/`; `cargo run` picks
  them up on restart. The deliverable stack (`docker-compose.prod.yml`) is where a
  built backend image exists, and it is not the edit loop.
- **Frontend** (`frontend/`): Vite HMR is enabled; changes hot-reload automatically.
- **DB schema**: add a new migration file `backend/migrations/000X_*.sql`. `sqlx::migrate!` runs it on boot.

## Running tests

```bash
make test        # unit + integration + e2e
```

The three levels cover different things, and the integration and e2e suites do
**not** touch your development database:

```bash
make test-unit       # backend + frontend unit tests, needs nothing running
make test-backend    # integration, against tempox_test, no external backend
make test-e2e        # Playwright, against tempox_test on :3099
make test-db         # drop and recreate the test database (disposable)
```

`tempox_test` lives in the **same** postgres container as `tempox`, on the same
volume. There is no second stack and no second image. The integration suite gives
each test its own database and spawns the binary itself on an ephemeral port; the
e2e suite lets Playwright own one backend on `:3099`. Either way the backend you
are using on `:3001` keeps serving, and nothing in a run points at it.

Why the tests do not use `tempox`: its ledger is append-only by trigger, so a row
a test left there cannot be deleted individually — only by discarding the whole
volume with `docker compose down -v`, which takes your own data with it. A test
run therefore never needs that decision.

The full rationale is [`testing.md`](./testing.md), which is the authority on the
test layout. What follows here is only the part that affects compiling.

### Why compiling needs no database

The `sqlx::query!` macros check their SQL while the crate compiles. `backend/.sqlx/`
carries the metadata they need, and `backend/.cargo/config.toml` turns on
`SQLX_OFFLINE`, so `make build` and `make lint` work with PostgreSQL stopped —
which matters, because `.env` names the host's view of the database and the
container's own value only resolves inside the compose network.

After changing a query, refresh that cache:

```bash
make sqlx-prepare
```

Compiling against a stale cache fails on a type that no longer matches, in a
place that names the query rather than the cause. CI runs `sqlx prepare --check`
so a stale cache cannot merge.

## Deploying

The deliverable stack is `docker-compose.prod.yml`, separate from the development
one because the two differ in almost every value: it runs the backend and the
frontend as built images, publishes only the frontend, has no adminer, and keeps
its data in `pgdata_prod`.

```bash
cp .env.prod.example .env.prod          # fill in real credentials
docker compose --env-file .env.prod -f docker-compose.prod.yml up -d --build
```

**`--env-file .env.prod` is not optional**, and the reason was measured. Without
it Compose reads the development `.env`, and the `${VAR:?}` guards do not catch it:
they refuse a variable that is _absent_, not one that is present and wrong. Plain
`${POSTGRES_PASSWORD:?}` resolved to the development `changethis`. That is why the
production variables are `PROD_`-prefixed — the development file cannot satisfy
them.

What the stack assumes:

- **Only the frontend publishes a port.** Put it behind whatever fronts the server
  and set `FRONTEND_PORT`.
- **`BACKEND_URL` is set in the compose file**, to `http://backend:3001`, because
  it names a service on the Compose network. The browser never sees it: it calls
  its own origin and the server forwards, which is what makes the app work from
  another machine on the LAN.
- **CORS**: `backend/src/main.rs` allows any origin. Behind the proxy it matters
  less, since the browser only ever talks to its own origin, but it is still the
  thing to tighten before exposing the backend directly.
- **`NODE_ENV=production` in the frontend service is load-bearing**: with
  `BACKEND_URL` missing, the proxy throws instead of falling back to a developer's
  localhost.

**No deployment has been exercised end to end yet.** What has been verified: the
image builds, serves the app, refuses to expose its own source, ships no
development tree, and proxies to a live backend — measured locally, not on a
server.

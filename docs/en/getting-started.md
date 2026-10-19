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

- **Backend** (`backend/`): edit sources under `backend/src/`. Run it from the host
  with `cargo run` for a fast edit cycle, or keep it in the container and rebuild
  (`docker compose up -d --build backend`). They cannot both hold `:3001`.
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
make test-backend    # integration, against tempox_test on :3099
make test-e2e        # Playwright, against tempox_test on :3099
make test-db         # drop and recreate the test database (disposable)
```

`tempox_test` lives in the **same** postgres container as `tempox`, on the same
volume. There is no second stack and no second image. The suite starts its own
backend on `:3099` so the one you are using on `:3001` keeps serving, and it
refuses to start if `TEST_DATABASE_URL` names the development database.

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

Tempox is designed for self-hosted deployment: `docker-compose.yml` already builds
both services, and its `${VAR:-default}` values let the same file serve a laptop
and a server. Nothing here is a separate production compose file.

**No deployment has been exercised yet.** The checklist below is what the
configuration requires, measured from the file rather than from a run, and it is
stated that way on purpose: an untested deploy guide that reads like a tested one
is worse than no guide.

For a production deployment, these have to come from the environment the container
runs in, not from `.env` (which is development-only and never leaves the laptop):

- `POSTGRES_PASSWORD` — the compose default is `tempox`, deliberately, so a clean
  clone starts. A server that inherits that default runs with a published password
  and no warning.
- `PUBLIC_API_BASE` — the frontend's default points at `localhost:3001`.
- **CORS**: `backend/src/main.rs` allows any origin. That is right for development
  and wrong for a public server.

Each of those three fails **silently** when left unset: the stack starts and works.
Nothing in the repository currently turns that into an error at boot, because the
binary has no notion of "which environment am I in" — there is no `ENVIRONMENT`
variable and no deploy to derive one from. Adding that guard is tracked as a
follow-up, not built here.

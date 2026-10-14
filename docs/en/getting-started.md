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
- Adminer DB UI at <http://localhost:8080> (server `postgres`, user `skilluv`, password `skilluv`, database `skilluv`)

## Making changes

- **Backend** (`backend/`): edit sources under `backend/src/`. In dev, restart the container (`docker compose restart backend`) or rebuild.
- **Frontend** (`frontend/`): Vite HMR is enabled; changes hot-reload automatically.
- **DB schema**: add a new migration file `backend/migrations/000X_*.sql`. `sqlx::migrate!` runs it on boot.

## Running tests

```bash
make test
```

Unit tests (backend + frontend) run without any external service. To also run backend integration tests that hit the DB:

```bash
docker compose up -d postgres backend
cd backend && cargo test -- --ignored
```

## Deploying

Tempox is designed for self-hosted deployment via [Coolify](https://coolify.io/). Point Coolify at the repo, and its Docker Compose recipe will build both services.

For production, at minimum change:

- `POSTGRES_PASSWORD` to a strong secret
- `PUBLIC_API_BASE` to your public backend URL
- Restrict CORS in `backend/src/main.rs`

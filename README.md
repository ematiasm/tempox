# Tempox

> Business management software in active development — Rust (Axum + sqlx) backend, SvelteKit 5 frontend, PostgreSQL 18.

[![CI](https://github.com/ematiasm/tempox/actions/workflows/ci.yml/badge.svg)](https://github.com/ematiasm/tempox/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

## Status

**v1 — Sales (backend).** Register a multi-item sale; stock drops per line and cash rises by the sale total, in one transaction. Products, customers, and the append-only movement ledger behind them are live, covered by unit and integration tests.

**Next: v1a — the Sales screen.** The UI is deferred on purpose: the slice is verifiable through the API and curl, so the interface gets built on a backend that already works. See [`plan-v1a.md`](./plan-v1a.md).

## What this is

- **Backend**: Rust (Axum, sqlx, PostgreSQL 18)
- **Frontend**: SvelteKit 5 (runes) + Tailwind
- **Orchestration**: Docker Compose (postgres + backend + frontend + adminer)
- **Tests**: `cargo test` + Vitest + Playwright
- **CI**: GitHub Actions

## Quickstart

Everything in Docker:

```bash
git clone https://github.com/ematiasm/tempox.git
cd tempox
cp .env.example .env
docker compose up --build
```

- Frontend: <http://localhost:5173>
- Backend: <http://localhost:3001/health>
- Adminer (DB): <http://localhost:8080>

## Development

The backend and database run in Docker; the frontend runs locally for a fast reload loop:

```bash
docker compose up -d --build postgres adminer backend

cd frontend && npm install && npm run dev
```

- Frontend: <http://localhost:5173> (local, hot reload)
- Backend: <http://localhost:3001/health>
- Adminer (DB): <http://localhost:8080>

After changing backend code, rebuild the container: `docker compose up -d --build backend`.

Integration tests write to a test database, never the development one. Create it
once, then run the suites:

```bash
make test-db-prepare   # once, after cloning
make test
```

`make test` runs the frontend unit tests and the backend unit + integration
tests. The backend integration tests record real sales, products and stock
movements, and `stock_movements` is append-only, so those rows land in
`tempox_test` — a second database inside the same postgres container — through a
second backend on port 3002.

Compiling needs no database at all: `backend/.sqlx/` carries the metadata the
`sqlx::query!` macros verify against, so `make build` and `make lint` work with
PostgreSQL stopped. After changing a query, run `make sqlx-prepare` to refresh
that cache; CI fails if it is stale. See
[`docs/en/getting-started.md`](./docs/en/getting-started.md).

## Structure

```
backend/     Axum server + sqlx migrations + integration tests
frontend/    SvelteKit app (Svelte 5 runes, Tailwind)
docs/        Documentation (en)
.github/     CI + dependabot + PR template
```

## What's inside

- `GET /health` — liveness probe
- `GET /api/hello?name=Ada` — dummy greeting endpoint
- `GET/POST/DELETE /api/notes` — CRUD demo backed by PostgreSQL
- **Sales slice (v1)**: multi-item sales with stock and cash derived from an append-only movement ledger — API surface in [`docs/en/architecture.md`](./docs/en/architecture.md)
- SvelteKit pages `/` (hello demo) and `/notes` (CRUD demo)
- FR/EN i18n via a runes-backed store

## Docs

- [`docs/en/getting-started.md`](./docs/en/getting-started.md)
- [`docs/en/architecture.md`](./docs/en/architecture.md)

---

## License

MIT — see [LICENSE](./LICENSE).

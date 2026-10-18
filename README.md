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

Integration tests run against the same stack, so start it first:

```bash
make dev        # postgres + backend + frontend
make test
```

`make test` runs the frontend unit tests and the backend unit + integration
tests. The integration tests drive the real API on `localhost:3001` and connect
to the `tempox` database on `localhost:5432`, so they need that stack up; without
it `make test-backend` stops with a message saying so rather than failing eight
tests with a connection error.

The tests record real sales, products and stock movements, and they land in the
database the screens read. Expect to see test rows (`w4-rice-<uuid>` and friends)
in the app after a run, and clear them by recreating the volume:

```bash
docker compose down -v
```

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
- **Sales reversal (v1.5)**: a mistaken sale is annulled by a credit note, in whole or in part — a second document that returns stock and money while the sale stays exactly as recorded. `POST /api/sales/{id}/credit-notes`, and `/api/cash` is net of the refunds
- **Purchases (v2, backend)**: goods that arrive raise stock and lower cash, mirroring sales. `GET/POST /api/suppliers` and `POST`/`GET /api/purchases`, with `one-off-supplier` seeded as the default supplier the way `walk-in` is the default customer. A purchase is a **fact**, not an intention: it has no `status`, so "the row exists" and "it counts" are the same statement. A purchase order is a different document, deferred to v2.5. `GET /api/cash` is now net of purchases as well as refunds
- **Supplier returns (v2.5, backend)**: goods that turn out to be wrong go back to the supplier. A return is a **second document** that takes stock out and brings money back, while the purchase stays exactly as recorded — the mirror of the sales credit note, in the other direction. `POST /api/purchases/{id}/supplier-returns`, `GET /api/purchases` carries each purchase's returns, and `/api/cash` now adds them back
- SvelteKit pages `/sales` (the ledger and the draft sheet) and `/products` (the catalog); `/` redirects to `/sales`, and `/notes` stays reachable by URL
- **es-AR / en** i18n via a runes-backed store: one `Dict` type, two complete dictionaries, and a per-language formatter bundle (money, quantities, dates). The shop's language is the default and the choice is not persisted; a reload always returns to es-AR

## Docs

- [`docs/en/getting-started.md`](./docs/en/getting-started.md)
- [`docs/en/architecture.md`](./docs/en/architecture.md)

---

## License

MIT — see [LICENSE](./LICENSE).

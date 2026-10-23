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
- **Orchestration**: Docker Compose — `postgres` + `adminer` for development,
  `docker-compose.prod.yml` with all three services for delivery
- **Tests**: `cargo test` + Vitest + Playwright
- **CI**: GitHub Actions

## Quickstart

The database in Docker, the code on your machine:

```bash
git clone https://github.com/ematiasm/tempox.git
cd tempox
cp .env.example .env
docker compose up -d          # postgres + adminer (the only two services)

cd backend && cargo run       # API on :3001, from .env
cd frontend && npm install && npm run dev   # UI on :5173, hot reload
```

- Frontend: <http://localhost:5173>
- Backend: <http://localhost:3001/health>
- Adminer (DB): <http://localhost:8080>

## Development

Only the database and adminer run in Docker. The backend runs with `cargo run`
and the frontend with `npm run dev`, both on the host, because that is the edit
loop: a change is visible in seconds instead of after an image build.

```bash
docker compose up -d postgres adminer     # the database, in Docker

cd backend && cargo run                   # API on :3001
cd frontend && npm run dev                # UI on :5173, hot reload
```

The API origin needs no configuration: the browser calls its own origin and the
SvelteKit server proxies `/api/*` to `BACKEND_URL`, which defaults to
`http://localhost:3001` in development. Nothing about a deployment is compiled
into a build.

## Deployment

The deliverable stack is a separate file, with real images and no development
tool in the group: no adminer, no published Postgres, and a frontend built with
`npm run build` instead of served by `vite dev`.

```bash
cp .env.prod.example .env.prod   # fill in real credentials
docker compose --env-file .env.prod -f docker-compose.prod.yml up -d --build
```

Two things worth knowing before deploying:

- **`--env-file .env.prod` is not optional.** Without it Compose reads the
  development `.env`, and the `${VAR:?}` guards do not catch it: they refuse a
  variable that is _absent_, not one that is present and wrong. That is why the
  production variables are `PROD_`-prefixed.
- **Only the frontend publishes a port.** Put it behind whatever fronts the
  server and change `FRONTEND_PORT` to taste.

`.env` names `localhost` for `DATABASE_URL`, which is the host's view of the
published port, so `cargo run` needs no exported variables. The container does
not read that value: `docker-compose.yml` composes its own from `POSTGRES_*`.

Rebuilding the backend image is **not** part of the edit loop — it is a delivery
check. Rebuild it once before shipping, when you want to verify that the image
boots with its embedded migrations. CI never builds it, so a green CI says nothing
about `tempox-backend:latest`.

### Tests

```bash
make test        # unit + integration + e2e
```

`make test` runs all three levels. They never touch the development database:
they use `tempox_test`, a separate database in the **same** postgres container,
and each suite owns the backend it talks to — the integration tests spawn the
binary themselves, and Playwright starts one on `:3099` — so the one you are using
on `:3001` keeps serving. Nothing test-related needs `docker compose down -v`, and
the rows a run creates are discarded with `make test-db`.

The full rationale — which level covers what, how to reset the test database, and
how the six phases removed the last piece of plumbing — is
[`docs/en/testing.md`](./docs/en/testing.md). That document is the authority; this
section is a summary of it.

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
scripts/     Test database helpers (`test-db.sh`)
.github/     CI + dependabot + PR template
```

## What's inside

- `GET /health` — liveness probe
- `GET /api/hello?name=Ada` — dummy greeting endpoint
- `GET/POST/DELETE /api/notes` — CRUD demo backed by PostgreSQL
- **Sales slice (v1)**: multi-item sales with stock and cash derived from an append-only movement ledger — API surface in [`docs/en/architecture.md`](./docs/en/architecture.md)
- **Sales reversal (v1.5)**: a mistaken sale is annulled by a credit note, in whole or in part — a second document that returns stock and money while the sale stays exactly as recorded. `POST /api/sales/{id}/credit-notes`, and `/api/cash` is net of the refunds
- **Purchases (v2, backend)**: goods that arrive raise stock and lower cash, mirroring sales. `GET/POST /api/suppliers` and `POST`/`GET /api/purchases`, with `one-off-supplier` seeded as the default supplier the way `walk-in` is the default customer. A purchase is a **fact**, not an intention: it has no `status`, so "the row exists" and "it counts" are the same statement. A purchase order is a different document, deferred to v2.5. `GET /api/cash` is now net of purchases as well as refunds
- **Supplier returns (v2.5, backend)**: goods that turn out to be wrong go back to the supplier. A return is a **second document** that takes stock out and brings money back, while the purchase stays exactly as recorded — the mirror of the sales credit note, in the other direction. `POST /api/purchases/{id}/supplier-returns` and `GET /api/purchases` carries each purchase's returns
- **Accounts and payment methods (v3)**: every document now says **where its money moved**. A payment method belongs to exactly one account; an account has zero or more methods; a sale, a purchase and both of their corrections name the method, and the method resolves the account. `GET/POST /api/accounts`, `GET/POST /api/payment-methods`, and a `PATCH` for each. Account balances are **derived** from one view (`money_movements`, one row per effect, sign declared once) and never stored. `GET /api/cash` is **retired**: the till is the account marked `is_default`, read from `/api/accounts`, so a sale paid by bank transfer no longer counts as money in the till. `sales` also lost its reserved `status`/`reversed_at`: nothing ever wrote them, and "the row exists" and "it counts" are now one statement for every document
- SvelteKit pages `/sales` (the ledger and the draft sheet), `/purchases`, `/products` (the catalog) and `/accounts` (where the money is); `/` redirects to `/sales`, and `/notes` stays reachable by URL
- **es-AR / en** i18n via a runes-backed store: one `Dict` type, two complete dictionaries, and a per-language formatter bundle (money, quantities, dates). The shop's language is the default and the choice is not persisted; a reload always returns to es-AR

## Docs

- [`docs/en/getting-started.md`](./docs/en/getting-started.md)
- [`docs/en/architecture.md`](./docs/en/architecture.md)
- [`docs/en/testing.md`](./docs/en/testing.md) — the test layout and why it is this way

---

## License

MIT — see [LICENSE](./LICENSE).

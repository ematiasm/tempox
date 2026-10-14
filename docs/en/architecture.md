# Architecture — Tempox

## Opinionated choices

### 1. Axum over Actix

Axum aligns with the tokio + tower ecosystem and has become the de-facto standard for new Rust HTTP services. Its extractor-based API is easy for beginners while remaining flexible.

### 2. sqlx over Diesel or SeaORM

- Raw SQL kept close to the code (no query DSL to learn).
- Compile-time query checking (via `sqlx::query!`) against a live database. The `sqlx::query!` macros resolve while the crate compiles, so CI runs a PostgreSQL service and applies the migrations before the first build step.
- Async-native, no blocking pool.

### 3. SvelteKit 5 with runes

- Reactive primitives are explicit (`$state`, `$derived`, `$effect`) — easier to reason about than legacy stores.
- Zero-config TypeScript.
- Server- and client-side rendering in the same file, with the option to add server actions later.

### 4. Docker Compose as the single entrypoint

- One `docker compose up` covers db + backend + frontend + adminer.
- Same commands work on Linux, macOS, and Windows (via WSL2).

### 5. i18n via a runes-backed class

- No i18n library dependency.
- Language state is a single `$state`-holding class; components read `lang.t.<key>`.
- Fine for starter-scale apps; swap in `svelte-i18n` if you outgrow it.

## API surface (v1 — sales)

All routes are under `/api`. Money is sent and stored as **integer cents** (`1500` = $15.00); quantities are integers in the product's **base unit** (`unit` | `g` | `ml`).

| Route | Purpose |
|---|---|
| `GET /api/products` | Products with stock derived from the movement ledger |
| `POST /api/products` | `{name, list_price, unit?, initial_stock?}` — creates the product; an `initial` movement is seeded only when `initial_stock > 0` |
| `GET /api/customers` | Customers, including the seeded `walk-in` |
| `POST /api/customers` | `{name}` — 409 on duplicate name |
| `POST /api/sales` | `{customer_id?, items: [{product_id, quantity, unit_price?}]}` — **one transaction**: the sale, its lines (name/unit/price snapshots), and one negative stock movement per line. Omitted `customer_id` → walk-in; omitted `unit_price` → list price. A `unit_price` of `0` is a bonus line (regalo): it is recorded, the stock still drops, and it contributes nothing to cash. 404 unknown product/customer; 400 empty, non-positive quantity, or negative price |
| `GET /api/sales?limit=50` | Recent sales first, with nested lines and customer name. The limit is clamped to 1..200, so `limit=0` returns one sale rather than none |
| `GET /api/cash` | `{total_cents}` — sum of confirmed sale totals, derived |

Request decoding is axum's `Json` extractor, so its rejections keep their standard codes: `415` without a JSON `content-type`, `400` on malformed JSON, and `422` when the shape parses but a field is wrong (an unparseable UUID, a missing field, `"quantity": "two"`). The handler's own validation answers `400`.

### Invariants enforced by the database

- **Money is exact**: integer cents, with `line_total = quantity × unit_price` and `sales.total = Σ line_total` computed by unit-tested pure functions before any write. Amounts are never negative; a price of `0` is allowed, which is how a bonus line (regalo) is recorded.
- **Stock is a ledger**: append-only movements; balances are always derived (`SUM`), never stored. The ledger rejects `UPDATE`, `DELETE` and `TRUNCATE`, and every reason is one of the values the schema knows, so a balance change always has a nameable cause.
- **Events are immutable**: `sale_items` reject `UPDATE` and `DELETE`; a sale's economics (`total`, `customer_id`, `payment_method`) are locked while the status transition still works for reversal; a product's `unit` is locked once it has history.
- **Corrections are compensating rows** (a reversal), never edits. The sales lifecycle is designed toward `draft → confirmed → cancel → done`.

## What's out of scope for Tempox

- Authentication (add your own auth crate, e.g. `axum-login`, JWT, or an external IdP).
- Observability beyond `tracing` (no Prometheus/OTel here — see `starter-devops`).
- Multi-tenancy / RLS.
- Rate limiting / anti-abuse.

## Suggested first extensions

- Add auth: `axum-login` with a `users` table + session cookie.
- Add a background job queue: `apalis` or `sqlx-based-outbox` for lightweight cases.
- Add OpenAPI: `utoipa` + `swagger-ui`.

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
- Language state is a single `$state`-holding class; components read `lang.t.<key>` for words and `lang.fmt.<...>()` for money, quantities and dates.
- **Two languages: `es-AR` (default) and `en`.** The choice is deliberately not persisted — the shop opens in its own language every time, and the toggle exists so a developer can read the English side. `fr` was a placeholder that proved the slot worked and was never a product requirement.
- **The locale is a frontend concern with exactly one exception.** The wire stays locale-free: money is a decimal string with two decimals (`"1234.56"`), timestamps are ISO 8601 UTC instants, enums stay `unit|g|ml` and `confirmed|cancel`, and no response gains a translated string. Error messages are translated in the client by HTTP status. The exception is the calendar: which window of instants is "today" is a fact about where the shop is, not about how the browser was installed, so `GET /api/day` owns it (`SHOP_TIMEZONE`) and the client renders each document's time in the zone that endpoint names.
- **Both directions of money are per locale.** `formatMoney` writes `$ 1.234,56` in es-AR and `$1,234.56` in en, without ever parsing the amount into a float. `parseAmount` reads what the active language types and always returns the wire form; a value it cannot read unambiguously (`1,234.56` under es-AR) is refused rather than guessed, because the value it feeds becomes an immutable sale line. Anything that pre-fills an amount field goes through `amountInput`, so what the form shows can always be parsed back.
- Fine for starter-scale apps; swap in `svelte-i18n` if you outgrow it.

## API surface (v1 — sales, v1.5 — reversal and locale, v2 — purchases, v2.5 — supplier returns)

All routes are under `/api`. Money is sent and stored as a **decimal amount** (`"15.00"` = $15.00) — a string on the wire, `NUMERIC(14,6)` with a whole-cents guard in the database, `rust_decimal::Decimal` in Rust. Quantities are integers in the product's **base unit** (`unit` | `g` | `ml`).

| Route | Purpose |
|---|---|
| `GET /api/products` | Products with stock derived from the movement ledger |
| `POST /api/products` | `{name, list_price, unit?, initial_stock?}` — creates the product; an `initial` movement is seeded only when `initial_stock > 0` |
| `GET /api/customers` | Customers, including the seeded `walk-in` |
| `POST /api/customers` | `{name}` — 409 on duplicate name |
| `GET /api/suppliers` | Suppliers, including the seeded `one-off-supplier`. Listed like any other, because a purchase without a named supplier names it explicitly and a picker that hid it would make that fallback invisible |
| `POST /api/suppliers` | `{name}` — 400 on a blank name (trimmed first), 409 on duplicate. No edit and no delete: a purchase that named a supplier is history |
| `POST /api/sales` | `{customer_id?, items: [{product_id, quantity, unit_price?}]}` — **one transaction**: the sale, its lines (name/unit/price snapshots), and one negative stock movement per line. Omitted `customer_id` → walk-in; omitted `unit_price` → list price. A `unit_price` of `0` is a bonus line (regalo): it is recorded, the stock still drops, and it contributes nothing to cash. 404 unknown product/customer; 400 empty, non-positive quantity, or negative price |
| `GET /api/sales?from=&to=&limit=50&offset=0` | Sales newest first, with nested lines, customer name, and `credit_notes` (oldest first, each with its `items`; an empty array when the sale was never corrected). `from` is inclusive and `to` is exclusive, so adjacent ranges partition the ledger instead of double-counting the sale recorded at midnight; absent bounds keep the unscoped recent-documents behaviour. `offset` defaults to `0`, clamped up to `0` so a negative value is not a 500. The limit is clamped to 1..200, so `limit=0` returns one sale rather than none |
| `POST /api/sales/{id}/credit-notes` | `{items: [{sale_item_id, quantity}]}` — **one transaction**: a credit note, its lines (name/unit/price snapshots of the corrected sale line), and one **positive** stock movement per line. Reverses part or all of a sale; several notes may follow while each line's cumulative refunded quantity stays within what was sold. 404 unknown sale or a line that is not that sale's; 400 empty items or a non-positive quantity; 409 a refund that would exceed the quantity sold. Concurrent refunds of one sale are serialized with `SELECT ... FOR UPDATE`, so two notes cannot both pass the cumulative guard |
| `POST /api/purchases` | `{supplier_id?, items: [{product_id, quantity, unit_price?}]}` — **one transaction**: the purchase, its lines (name/unit/price snapshots), and one **positive** stock movement per line. Omitted `supplier_id` → `one-off-supplier`; omitted `unit_price` → list price. 404 unknown product/supplier; 400 empty items, a non-positive quantity, or a line total finer than a cent (a price of `0.005` with quantity 1 is refused; with quantity 2 the line total is `0.010`, which is whole cents, so the handler accepts it and the database's own whole-cents check rejects it as `500` — the same shape the sale path has). A rejected purchase writes nothing: every line is resolved before the first write, so an unknown product later in the list leaves the earlier ones unwritten too |
| `GET /api/purchases?from=&to=&limit=50&offset=0` | Purchases newest first, with nested lines and `supplier_returns` (oldest first, each with its `items`; an empty array when the purchase was never returned, never an absent field). The same half-open `[from, to)` contract, the same `limit` clamp (1..200) and the same `offset` clamp as the sales ledger — a day filter that meant one thing on one screen and another on the next would be a bug that reads as a UI fault |
| `POST /api/purchases/{id}/supplier-returns` | `{items: [{purchase_item_id, quantity}]}` — **one transaction**: the return, its lines (name/unit/price snapshots of the corrected purchase line), and one **negative** stock movement per line. Returns part or all of a purchase; several returns may follow while each line's cumulative returned quantity stays within what was bought. The client sends **no price**: what the return is worth is read from the purchase line, so a return can never be worth more than the purchase. 404 unknown purchase or a line that is not that purchase's; 400 empty items or a non-positive quantity; 409 a return beyond what the line has left. Concurrency is serialized with `SELECT ... FOR UPDATE` on the purchase, and the running total is summed inside the request so a request naming the same line twice is judged on its total |
| `GET /api/cash` | `{total}` — **confirmed sales − every credit note − every purchase + every supplier return**, derived. The response shape has never changed (one two-decimal string) but the meaning has, twice: money in the till is what came in from sales, less what went out in refunds and in buying stock, plus what suppliers gave back. Worked example: sell `100.00`, buy `40.00`, return `15.00` → the till reads `75.00` |
| `GET /api/day?at=<instant>` | `{from,to,prev,next,zone}` — the shop day containing `at` (ISO 8601 UTC instants; `from` inclusive, `to` exclusive) plus the IANA `zone` it was computed in, taken from `SHOP_TIMEZONE` (default `America/Argentina/Buenos_Aires`). Absent `at` means now; a malformed `at` is `400`. The zone is server configuration, never a query parameter, and it is returned because the client renders each document's time in it — a browser only knows its own |

Request decoding is axum's `Json` extractor, so its rejections keep their standard codes: `415` without a JSON `content-type`, `400` on malformed JSON, and `422` when the shape parses but a field is wrong (an unparseable UUID, a missing field, `"quantity": "two"`). The handler's own validation answers `400`.

### Invariants enforced by the database

- **Money is exact**: a decimal currency amount with two decimals, guarded by *value* (`v = round(v, 2)`) in a column that declares scale 6, so an over-precise value is refused instead of silently rounded. `line_total = quantity × unit_price` and `sales.total = Σ line_total` are computed by unit-tested pure functions before any write. Amounts are never negative; a price of `0` is allowed, which is how a bonus line (regalo) is recorded.
- **Stock is a ledger**: append-only movements; balances are always derived (`SUM`), never stored. The ledger rejects `UPDATE`, `DELETE` and `TRUNCATE`, and every reason is one of the values the schema knows (`initial`, `sale`, `credit_note`, `purchase`, `supplier_return`), so a balance change always has a nameable cause. A movement for a sale or a credit note must name the document that caused it, a `credit_note` movement must name its note, and a `purchase` movement must name its purchase **and be positive** — stock comes in on a purchase, and the ledger accepts negative quantities in general, so without that rule a purchase could silently remove stock while its document says the goods arrived. A `supplier_return` movement is the mirror: it must name **both** the purchase it corrects and the return, and it must be **negative**, because stock leaves.
- **Events are immutable**: `sale_items`, `credit_notes`, `credit_note_items`, `purchases`, `purchase_items`, `supplier_returns` and `supplier_return_items` reject `UPDATE`, `DELETE` and `TRUNCATE`; a sale's and a note's economics (`total`, `customer_id`/`sale_id`, `payment_method`) are locked. A purchase has **no permitted edit at all** — not even a status transition — because it has no status.
- **A return is a second document**: a wrong purchase is never edited or deleted, because its guards make that impossible and deleting it would erase a real movement of money and stock. Returning goods appends a `supplier_returns` document that names the purchase it corrects. Whether a purchase is fully returned is **derived** from its returns, never written onto it — the purchase keeps its own `total` and reads as a purchase either way.
- **A purchase is a fact, not an intention**: `purchases` carries no `status` column, so "the row exists" and "it counts" are the same statement in stock, in cash and in every derived number. A purchase *order* is a different document, still deferred (the owner's decision of 2026-10-18 put supplier returns in v2.5 and left orders for a later slice), and adding it later is a nullable `ALTER TABLE` with no backfill — which is why no reserved column anticipates it.
- **Corrections are compensating rows** (a reversal), never edits. Reversing a sale emits a **credit note** — a second document that returns stock and money — and leaves the sale byte-for-byte as recorded: `status` stays `confirmed` and `reversed_at` stays NULL. A partial refund is not an annulment; a sale reads as annulled when its lines are fully refunded, which the client derives from the notes.

## What's out of scope for Tempox

- Authentication (add your own auth crate, e.g. `axum-login`, JWT, or an external IdP).
- Observability beyond `tracing` (no Prometheus/OTel here — see `starter-devops`).
- Multi-tenancy / RLS.
- Rate limiting / anti-abuse.

## Suggested first extensions

- Add auth: `axum-login` with a `users` table + session cookie.
- Add a background job queue: `apalis` or `sqlx-based-outbox` for lightweight cases.
- Add OpenAPI: `utoipa` + `swagger-ui`.

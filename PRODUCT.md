# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

The owner of a small retail business, plus the employees who will use the system alongside them. Day-to-day job: record sales, check stock and cash, know what happened. The owner is also the sole developer (beginner, building for their own business); employees are future users who need explicit navigation and clear affordances rather than power-user density.

## Product Purpose

Tempox replaces paper-plus-spreadsheet bookkeeping for a small business: register a sale once and stock, cash, and history update together. Success = the owner stops double-entering the same event, and can answer "what's left?" and "how much did I take?" without manual calculation.

## Positioning

**Everything connected in one step**: a single sale writes stock, cash, and the record at once — no second entry anywhere. Built on an append-only ledger, so every number the system shows can be explained.

## Operating Context

Back office at a fixed PC in the shop: desktop-first, keyboard-friendly, mobile secondary. Stack in use: PostgreSQL in Docker, Rust/Axum API, SvelteKit 3 with `@sveltejs/adapter-node` 6 on Vite 8 and Svelte 5; the owner runs everything locally.

## Capabilities and Constraints

- v1 backend shipped and tested: multi-item sales with snapshot lines, products (list price + base unit + initial stock), customers (name-only, seeded `walk-in`), derived cash, derived stock.
- **Purchases shipped (v2 backend)**: recording a purchase raises stock and lowers cash, mirroring a sale in the opposite direction. Suppliers are name-only and seeded with `one-off-supplier`, exactly as customers are seeded with `walk-in`. Cash on hand is what came in from sales, less what was refunded, less what was spent — one number, derived, never stored, and net of a supplier return that brings money back to the till.
- **Supplier returns shipped (v2.5 backend)**: a wrong purchase is corrected by returning the goods, not by editing or deleting it. The return is its own document: stock leaves, the supplier's money comes back, and the purchase keeps every figure it was recorded with. What a purchase still owes — or how much of it came back — is read from its returns, never written onto it.
- The database enforces: decimal money with an exact whole-cents guard, exact totals, immutable events (no edit/delete — a mistaken sale is reversed by a credit note, in whole or in part, never edited).
- **The three surfaces shipped (v1a, v2a)**: `/sales` records a multi-item sale and reads the day's stamped ledger with its cash foot; `/products` is the paginated catalog with search and create; `/purchases` records a purchase with its supplier picker inside the draft, reads the day's purchases, and carries each purchase's returns under the purchase they correct. The rail names those three jobs. `/notes` exists but stays out of the rail, reachable by URL only.
- Compile-time checked queries. The crate compiles with **no database at all**, from the `.sqlx/` offline cache (`SQLX_OFFLINE=true`); CI verifies that cache is current against a real PostgreSQL, so the image build never reaches one.
- **The UI speaks es-AR by default, with an English side** (v1.5). Money is ARS written `$ 1.234,56`, the ledger's day is the shop's own (server-side, `SHOP_TIMEZONE`), and `walk-in` displays as "Consumidor Final" while the stored value stays `walk-in`. The language choice is not persisted: the shop opens in its own language every time.
- Undecided: auth and roles (v5 staff), expenses/accounting (v4–v6), multi-currency. **Decided for v2**: a purchase is a fact with no status, and a purchase order (with partial reception) is a later, separate document rather than a state of this one. **Decided for v2.5**: returns to a supplier are their own document too, and what a purchase is worth back is read from the purchase line, never sent by the client — the shop records what happened, and recording what was merely asked for is a different job.

## Brand Commitments

Name: **Tempox**. No logo and no identity asset. No voice is committed as a product decision — the UI's tone is described in `DESIGN.md`, not here. A visual world **is** committed and binding: "The Stamped Ledger" lives in [`DESIGN.md`](./DESIGN.md) and `.impeccable/design.json` (paper ground, hairline rules, ink, one crimson accent spent on the stamp). This record deliberately carries no palette of its own; when the two disagree, the design system wins on visual questions and this file wins on product truth.

## Evidence on Hand

Live API verified by curl, by integration tests against a real PostgreSQL, and by Playwright end to end over sales, purchases and supplier returns. There is no customer, benchmark, pricing or testimonial of any kind; future work must not invent one. The `/notes` demo is a walking skeleton left over from v0 — it is **not** proof of the product, and it stays out of the rail on purpose.

## Product Principles

1. **One action, one write**: the sale is the single entry point; stock and cash derive from it. A purchase is the same shape in the other direction — one action, one transaction, and the consequences derive.
2. **The ledger is the truth**: balances are sums, never stored fields.
3. **Immutable events**: a mistake is corrected by a reversal, never an edit — a credit note when a customer brings something back, a supplier return when the shop does.
4. **Explicit over clever**: a future employee must be able to use it without training.

## Accessibility & Inclusion

No product-specific requirement established; standard keyboard-and-screen-reader practice applies.

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

Back office at a fixed PC in the shop: desktop-first, keyboard-friendly, mobile secondary. Stack in use: PostgreSQL in Docker, Rust/Axum API, SvelteKit 5 frontend; the owner runs everything locally.

## Capabilities and Constraints

- v1 backend shipped and tested: multi-item sales with snapshot lines, products (list price + base unit + initial stock), customers (name-only, seeded `walk-in`), derived cash, derived stock.
- The database enforces: integer-cents money, exact totals, immutable events (no edit/delete — corrections are reversals), product unit locked once referenced.
- Compile-time checked queries; the image build verifies them against the live database.
- **Terminology and UI in English until the es-AR i18n layer lands** (user decision; `walk-in` displays as "Consumidor Final" later).
- Undecided: auth and roles (v5 staff), reversal endpoint (v1.5), purchases/expenses/accounting (v2–v6).

## Brand Commitments

Name: **Tempox**. No logo, palette, or voice commitments yet.

## Evidence on Hand

Live API verified via curl and integration tests; the notes demo doubles as the end-to-end walking-skeleton proof.

## Product Principles

1. **One action, one write**: the sale is the single entry point; stock and cash derive from it.
2. **The ledger is the truth**: balances are sums, never stored fields.
3. **Immutable events**: a mistake is corrected by a reversal, never an edit.
4. **Explicit over clever**: a future employee must be able to use it without training.

## Accessibility & Inclusion

No product-specific requirement established; standard keyboard-and-screen-reader practice applies.

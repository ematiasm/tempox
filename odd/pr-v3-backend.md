# PR 1 — v3 backend: accounts, payment methods and the destinations

## Chain Context

| Field | Value |
|---|---|
| Chain | v3 accounts and payment methods |
| Tracker PR | Not needed — this is a two-PR chain, not a feature branch |
| Position | 1 of 2 |
| Base | `main` |
| Depends on | None |
| Follow-up | PR 2 — frontend and the closing drop (`feat/v3-frontend-close`) |
| Review budget | **5 509 changed lines / 400** — `size:exception` requested, reason below |
| Starts at | `bae271c` (`main`) |
| Ends with | a backend where every money document says where its money moved, balances are derived from one view, and `/api/cash` is retired |

### Chain Overview

```text
main
 └── 📍 PR 1 — backend (T1, T0, T3, T4, T5, T6)
      └── PR 2 — frontend + the closing drop (T7, T8, T2)
```

## What this delivers

Six tasks, each reviewed and sealed independently by native review:

- **T1** — `0012`/`0013`: accounts, payment methods, the four destinations, and the
  guards that make a destination meaningful.
- **T0** — `GET`/`POST /api/accounts` and `/api/payment-methods`, plus a `PATCH` each.
- **T3** — `0014`: the `money_movements` view and derived balances.
- **T4** — a sale names where the money went; the wire goes flat.
- **T5** — purchases, the credit-note override, and supplier-return inheritance.
- **T6** — `/api/cash` retired; the till is the default account.

## Why `size:exception`

Two reasons, both structural and neither compressible:

1. **The ODD document is 1 364 lines of decision log** and it ships with the code it
   explains. `odd/tasks/v3-accounts.md` carries specs, task table, and 109 log entries.
   Compressing it to fit a review budget is explicitly forbidden by this repo's own
   chained-PR rule ("never delete comments, blank lines, docs, or tests, and never
   compress or restyle code, to fit under the budget").
2. **T1 alone is 2 221 lines** — two migrations, four guards, a backfill over immutable
   history, and twelve integration tests. It cannot shrink; the guards are the point.

The honest split was attempted: nine task-sized PRs were planned and rejected because
**the log entries follow the order decisions happened, not the order of tasks**, so
grouping them per task would mean rewriting content in commits already reviewed and
approved — destroying the correspondence between what was sealed and what is committed.

## Review order, if you read one thing

Read `0012` and `0015`-adjacent guards first — they are where a mistake is unrecoverable.
The view (`0014`) is the next most load-bearing: every displayed balance comes from it.

## Excludes

- The whole frontend (`/accounts` screen, the method columns, the sale picker) — PR 2.
- The lifecycle drop — PR 2, because it is the one irreversible act and deserves to
  close the feature rather than sit in the middle of it.

## Autonomy

- [x] CI is expected to pass for this branch — verified locally: 91 integration, 50 unit.
- [x] Migrations apply from scratch on an empty database.
- [x] `make sqlx-check`, `cargo fmt --check`, `cargo clippy -D warnings` clean.

## Known debt, named rather than hidden

An explicit method on a **credit note** for a sale already on credit is not checked
against settlement state: a sale never collected can be refunded naming `cash`, driving
the till negative while the debt stands. Reproduced end to end and handed to the
collections slice as a requirement — v3 has no notion of "this sale was paid".

# PR 2 — v3 frontend, the e2e that proves it, and the closing drop

## Chain Context

| Field | Value |
|---|---|
| Chain | v3 accounts and payment methods |
| Tracker PR | Not needed |
| Position | 2 of 2 |
| Base | `feat/v3-backend` (PR 1) |
| Depends on | PR 1 — every endpoint and migration this consumes |
| Follow-up | None; this closes v3 |
| Review budget | **1 798 changed lines / 400** — `size:exception` requested, same reason as PR 1 |
| Starts at | `8cc6e73` (PR 1's head) |
| Ends with | the screen the shop uses, an e2e proving a transfer is not cash, and a `sales` table with no lifecycle |

### Chain Overview

```text
main
 └── #PR1 backend
      └── 📍 PR 2 — frontend + T2
```

## What this delivers

- **T7** — the `/accounts` screen with its five write actions, the method column in both
  ledgers, the rail destination, and `Sale`/`Purchase` migrated to the flat wire.
- **T8** — `architecture.md` and `README.md` describing the schema that exists; the sale
  draft's method picker; and an e2e that records a sale on a bank transfer and asserts
  the bank holds it.
- **T2** — `0015`: `sales.status` and `sales.reversed_at` dropped, the view recreated
  without the filter, and the guard re-cut. **This is the only irreversible act in the
  slice**, which is why it closes rather than sits inside.
- One test-infrastructure commit: `workers: 1` for the e2e, because the suite shares one
  database.

## Two commits that belong to PR 1 and live here

`414db30` (T0's spec correction) and `41e67b2` (T6's approval) are **documentation for
PR 1's tasks** that landed after the split point. They are here because the log follows
the order decisions happened, and moving them would rewrite commits already sealed.
Declared, not hidden — see PR 1's `size:exception` note.

## The e2e earned its place

Two real defects that no other level could see:

- the accounts screen printed the raw wire string (`0.00` where every other money
  figure reads `$ 0,00`);
- `SaleDraft` had **no method picker at all** — PR 1 built the API field and the counter
  could not reach it.

Neither was visible to 91 integration tests, 232 unit tests, six native reviews or the
mutation runs, because every one of those tests the layer it was written for.

## Excludes

- Everything in PR 1.
- Any change to the credit-note settlement gap (documented in PR 1, owed to the
  collections slice).

## Autonomy

- [x] CI is expected to pass — verified locally: 91 integration, 50 unit, 232 frontend
  unit, 22 e2e serialized, `svelte-check`, `eslint`, `prettier`, `build` all clean.
- [x] Migrations 12→15 apply from scratch on an empty database.
- [x] `make test` green across all three levels.

## Risks

| Risk | Item | Why it is acceptable |
|---|---|---|
| `DROP COLUMN` on populated tables | 1 | Two pre-checks refuse rather than erase a fact; the migration captures per-account totals before the drop and refuses if any figure moved |
| A rail destination and a screen with five write actions | 3 | Native review approved it; the till-move without a confirmation step is carried as advisory debt |
| `payment_method` and `status`/`reversed_at` leave the wire | 3 | Breaking for one consumer, this app's own frontend, updated in the same PR |

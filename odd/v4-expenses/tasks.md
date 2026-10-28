# v4-expenses — feature tasks (Engram mirror)

Source of truth: `odd/tasks/v4-expenses.md`. This mirror exists so a later session resumes
from memory instead of re-deriving the design. Update both after every task transition.

**State: designed, not implemented.** 12 specs (S1-S12), 5 decisions (D1-D5), 8 tasks across
3 slices (E1 backend, E2 screen, E3 spending read model).

## The one sentence

```
a utility bill, a repair or a salary is recorded once, with a category and who was paid,
whether the money left today or leaves in three advances,
and the shop's spending for a day includes it — plus the purchases it already recorded.
```

## The five decisions a later session must not re-derive

- **D1** — a purchase is **derived** into the spending report, never copied into `expenses`
  (owner's verb: "derivar"). Two records of one fact is what `0017` killed for balances.
- **D2** — a category is a **row** (`expense_categories`), never a `CHECK`. Closed sets cost
  this schema three migrations (`0011`, `0020`, `0023`).
- **D3** — an owed expense is a **party to its own money**: `payments.party_kind = 'expense'`,
  `party_id = expenses.id`. No new settlement table, no third ceiling function.
- **D4** — the price of D3: one payment answers one expense. Stated, not discovered.
- **D5** — salaries are a **category** now; naming *which* employee needs a staff table and is
  deferred to the staff slice. **D5a**: when it lands it is `expenses.staff_id`, never a
  `salaries` document. **D5b**: not reserved now (`architecture.md:104`).
- **D6** — two alternatives measured and rejected: a field on `purchases` (refused by
  `reject_purchase_mutation`, `0001_schema.sql:774-794`) and expenses-as-`money_entries`
  (refused by four measurements; an owed expense has no movement at all).
- **D6a** — classifying purchases by rubro is **deferred**; when it returns it is
  `expense_classifications`, never a column on `purchases`.

## The schema claim that carries the design

An expense writes **no `money_entries` row of its own kind**. The money moved when its
`payments` row was written, and `payments_claim_their_entry` already guarantees exactly one
`'payment'` entry per payment. An `'expense'` kind would count the same coin twice — and the
first draft of the document had that defect, which is why S5 exists.

Consequence: this migration does **not** re-cut `money_entries.document_kind`. It re-cuts two
`payments` `CHECK`s, by whole member list, with a `DO` assertion naming every existing member.

## Tasks

| ID | Slice | What | Risk | State |
| --- | --- | --- | --- | --- |
| E1-T1 | backend | `expense_categories` + seed + GET/POST/PATCH | — | pending |
| E1-T2 | backend | `expenses` table + `payments.party_kind='expense'` + two-state trigger + `POST /api/expenses` | item 1 | pending |
| E1-T3 | backend | `POST /api/expenses/{id}/advances`, ceiling in route and trigger | item 1 | pending |
| E1-T4 | backend | `GET /api/expenses` + `GET /api/expenses/summary` (purchases as one derived group) | — | pending |
| E1-T5 | backend | tests proven to bite + `architecture.md`, `README.md`, `PRODUCT.md` | — | pending |
| E2-T1 | screen | `/expenses` draft sheet, day ledger, rail, version chip, both dictionaries | item 3 | pending |
| E2-T2 | screen | advance action in the expanded expense, spending foot, one e2e by identity | item 3 | pending |
| E3-T1 | read model | **deferred by the owner (L7)** — `GET /api/spending` grouped by category | — | deferred |

Order: E1-T2 needs T1; E1-T3 needs T2; E1-T4 needs T3; E2 needs E1 green. **E1 and E2 are
this feature's committed work**; E3 is the deferred remainder and drops without leaving a
half-built document.

## Slice sizes

E1 (~900 authored lines with tests, `size:exception` with its reason in the document), E2
(~700), E3 (~250, deferred). Not comparable to `v4-payables`' 3556 refused lines.

## Not in scope, with reasons

Salaries as a staff flow (D5-D5d) · one payment across several expenses (D4) · classifying
purchases by rubro (D6a, deferred) · a recurring template · reversing an expense (its own
slice, the mirror of `annulment`) · receipt images (no file storage) · `created_by`
(`v4-settlement.md` L24).

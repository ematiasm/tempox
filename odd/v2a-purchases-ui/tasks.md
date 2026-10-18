# v2a-purchases-ui — tasks (Engram mirror)

Full document: `odd/tasks/v2a-purchases-ui.md` (specs authority, verification).
Design authority: `plan-v2.md`; backend context: `odd/tasks/v2-purchases.md`.
Engram topic key: `tempox/odd/v2a-purchases-ui/tasks`.

**State: implemented, not delivered.** T7–T10 done on `feat/v2a-purchases-ui` off
`35d8878`. Three commits. Checks green.

## Specs (one line each — the document is authoritative)

S1 the rail carries the three jobs: sell, buy, catalog; `/notes` stays off it.
S2 `/purchases` records from a sheet at the head of the book, mirroring `/sales`.
S3 the supplier defaults to the seeded one, shown as `Proveedor ocasional`; the
   stored `one-off-supplier` never reaches the screen.
S4 a duplicate supplier is reported, not absorbed — unlike the customer path,
   because a supplier picker already lists them.
S5 the price prefill goes through `lang.fmt.amountInput`, never the wire form.
S6 the live total is the client's sum; the recorded total is the server's.
S7 the band owns the single next action: button, F2 and Ctrl+Enter, one path.
S8 the ledger shows the shop's day and pages by the server's neighbouring days.
S9 each document expands onto its snapshot lines.
S10 the supplier name is resolved from the supplier list, not a joined field.
S11 every key exists in types/es/en, written as es-AR shop vocabulary.
S12 empty, loading and error states are honest.

## Tasks

| ID | Task | Specs | State | Commit |
|---|---|---|---|---|
| T7 | Client layer + unit tests | S4, S5, S10 | done | `ff93a75` |
| T8 | Rail: third destination | S1 | done | `e52937f` |
| T9 | `/purchases` draft sheet | S2–S7 | done | `e52937f` |
| T10 | Ledger + i18n + e2e barrier | S8–S12 | done | `e52937f`, `pending` |

## Verified (commands and observed results)

- T7 RED first (8 tests failed on missing functions), then 79 unit tests pass.
- `svelte-check` 0/0, `eslint` clean, `prettier --check` clean.
- `npx playwright test`: 10 passed, including a purchase flow that re-reads the
  catalogue to prove stock rose by 7, and a two-language test of the screen.
- Mutation checks: the localized supplier label (raw `s.name` fails both
  directions); the client's `?? null` supplier omission.
- Three defects fixed, all found by writing the e2e: the wire-form price prefill
  (v1.5's exact defect), a selector resolving to the supplier `<select>`, and the
  product-create button staying disabled on `stock: 0`.

## Delivery

1 412 authored lines in two commits for the UI work. One PR per slice, with the
reviewable units inside it: `ff93a75` is the client layer plus tests (reviewable
alone), `e52937f` is the three UI units together because a rail tab with no screen
behind it 404s.

## Log (tail)

L1 (user, verbatim): "arranca con v2a en una nueva rama con su PR"

# Mirror the receipt pad onto /purchases

`/sales` became a pad and a stack of slips in `odd/tasks/receipt-pad-sales.md`.
`/purchases` is the same screen in the other direction — a purchase raises stock
and lowers cash where a sale does the reverse — so it must read the same way.
Today it still draws a form above a day table, which is what `/sales` stopped
being.

The direction is not invented here: `PurchaseDraft.svelte` and
`PurchaseLedger.svelte` are the mirror of `SaleDraft`/`Ledger` by construction, so
the target is those components as they now stand.

## Specs

**S1** — The purchase draft is the slip it becomes: a head carrying its state and
the supplier, the product entry, printed line rows, and a foot carrying the live
total. No table composes the draft's lines.

**S2** — The day is a stack of purchase slips, each carrying its own document
number, time, supplier, method, state and total. No table composes the day.

**S3** — The direction is preserved in the figures: a purchase takes money out of
the till, and a supplier return brings money back. Nothing about that arithmetic
changes; only how it is drawn.

**S4** — The supplier return block, the credit/return notes and the return form
behave exactly as before, including the "more than the line has left" refusal and
the per-line scope.

**S5** — Purchase figures stay visible and unmoved: the purchase total, the
returned amount and the period's net cost keep the same meaning.

**S6** — Behaviour is unchanged and no e2e assertion may be weakened. Selectors
that move go through helpers, and every migrated test must still bite.

## Tasks

- [x] **M1** — Rebuild `PurchaseDraft.svelte` as the slip. (S1)
- [x] **M2** — Rebuild the day in `PurchaseLedger.svelte` as a stack of slips. (S2)
- [x] **M3** — Recompose `routes/purchases/+page.svelte`: pad column, day column,
  action at the foot of the slip. (S5)
- [x] **M4** — Add `purchaseDraftLine`/`purchaseSlip`/`purchaseSlipDetail` helpers
  and migrate the selectors; prove each migrated test still bites. (S6)
- [x] **M5** — Prove S3/S4 on the direction: a purchase lowers cash, a return
  raises it, and both survive the redraw.

Evidence:

- `make test` — frontend unit 263/263, backend lib+bins 67/67, integration 157/157,
  e2e 34/34 (33 before this feature; the new direction test is the 34th).
- The direction test was proven to bite: flipping the sign of the till figure in
  `accounts/+page.svelte` makes it fail with `Expected: 50, Received: -50`.
- The backend was checked directly, not through the UI, when the direction test
  first disagreed with itself: `POST /api/purchases` for 50.00 moved the default
  account from `50.00` to `0.00`. The wrong reading was the test's, twice —
  first selecting `li.row` by the text "Caja" (which matches the *tick* "Caja del
  negocio" inside any row) and then stripping the minus sign while parsing the
  formatted figure. Both are recorded here because a money test that lies is
  worse than no test.

## Log

**L1** — user, verbatim:

> hace compras espejo de ventas

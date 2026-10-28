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

**L2** — Owner asked to consolidate the duplicated components ("si, encaralo"). The
measurement first, because "the same component twice" was the assumption and it
was only half true:

| | lines | script | style |
|---|---|---|---|
| `Ledger` | 1607 → 1478 | 524 | 701 |
| `PurchaseLedger` | 1042 → 960 | 269 | 455 |
| `SaleDraft` | 797 | 236 | 342 |
| `PurchaseDraft` | 573 | 208 | 230 |

The ledgers' **scripts diverge in 621 lines**, and the data models are genuinely
different: a sale carries `credit_notes`, a purchase carries `supplier_returns`,
and only the sale has a running balance. Folding them into one component behind
direction flags was rejected as the conditional monster that would make the next
change unreadable. That was the right reading of the numbers.

What *was* the same is the slip. Extracted to `DaySlip.svelte` (270 lines): head,
chevron, body, struck-through state, the whole keyboard contract, and the optional
`balance`. Each ledger keeps its own `who`/`meta`/`detail` as snippets, so it still
says in its own file what it prints.

**Correction to an earlier claim.** The first measurement said "157 identical CSS
lines" — that compared *individual lines* after `sort`, not rules. On inspection
`.lines td` is `padding: 6px 10px` on sales and `4px 8px` on purchases: the two
detail panels have genuinely different density. Merging them would have changed how
one screen looks, which nobody asked for. So `.lines` stays per-surface and is only
marked global, because the snippet renders in the parent and the style block must
still reach it.

Evidence:

- `make test` — 263 unit / 67 backend / 157 integration / **35 e2e**.
- Slip parity measured in the browser on both screens: `radius 8px`, `border 1px`,
  `headPad 10px 14px`, `slip-no 700`, `slip-total 20px`; the only difference is
  `hasBal` true on sales and false on purchases, which is the intended asymmetry.
- Two defects the refactor introduced and the suite caught:
  1. `PurchaseLedger` was left emitting its own `<article>` while its styles were
     deleted, so purchases rendered an **unstyled** slip (`radius 0px`,
     `border 0px`). Found by measuring the computed style, not by the tests.
  2. `idPrefix` plus the template produced `purchase-lines--<id>` (double hyphen),
     which broke four specs. Proven to bite by reintroducing it.
- A flake in the direction test was fixed rather than retried: it read the till
  before the written return had landed. It now waits for the `.return-block` to
  appear first.

**L3** — Owner asked whether to go on and consolidate `SaleDraft`/`PurchaseDraft`.
The answer was no, measured rather than assumed, and the measurement turned up two
real defects that had nothing to do with consolidation.

**Why consolidation is not the move.** The scripts diverge in 308 lines, and the
divergence is semantic: only `SaleDraft` has customers, payment methods and bonus
counting; only `PurchaseDraft` has suppliers and its own `describe`. `draftLines.ts`
already states the deliberate difference in its own header — a sale records what is
complete with at least one line, a purchase requires every line to parse. One
component behind a `mode` flag would put that rule in a conditional.

**What was worth doing.** `PurchaseDraft` re-implemented half of a rule the library
owns (`ready = parsedOk && !submitting` instead of using the shared guard), which is
the shape that drifts silently. `record()` now consults `submitting` on entry, so
the rule lives in one place and the guard is real.

**Two defects found while measuring, both real and both fixed:**

1. **`PurchaseDraft.record()` had no re-entry guard.** Only the disabled button
   prevented a second submission: a queued Ctrl+Enter or a form submit reached the
   handler regardless. Measured before the fix — two dispatched Ctrl+Enter presses
   produced **two purchases** (`posts: 2`), so stock rose twice and the till fell
   twice. `Catalog.svelte` already named this exact symmetry in its own comment
   ("`SaleDraft` opens with `if (!canRecord) return` … this one had lost the
   symmetry"). Now guarded, and covered by an e2e that proves it bites.
2. **`/purchases` advertises F2 and does not implement it.** The button prints
   `<kbd>F2</kbd>`, carries `aria-keyshortcuts="F2 Control+Enter"`, and the hint
   says "Ctrl+Enter registra" — but the route registers no key handler at all.
   `Ctrl+Enter` happens to work because the draft listens for it itself; `F2` does
   nothing. `/sales` wires F2 on the page. **Not fixed here** — it is a separate
   behaviour decision (wire F2, or stop advertising it) and it is recorded rather
   than silently guessed.

**Third finding, about coverage rather than code:** crippling `SaleDraft`'s guard
(`canRecordNow(lines, false)`) left all six sale specs green. The sale screen was
correct but untested on this exact risk, so a later refactor could have removed the
guard with the suite's blessing. Both directions now have the test, and both were
proven to bite by breaking the guard and observing `Received: 2`.

Evidence: `make test` — 263 unit / 67 backend / 157 integration / **37 e2e**.

**Deferred, with its criterion:** extracting the shared *frame* of the two drafts
(header, printed row, foot) as presentational components — the move that worked for
`DaySlip` — is worth doing **only when the redesign next touches a draft**, and only
for the markup, never for the validation rules. Not now: this branch is at ~7.4k
authored lines and adding a 1.4k-line refactor makes it harder to review, not easier.

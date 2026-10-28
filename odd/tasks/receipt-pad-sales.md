# Receipt pad for /sales

Implement `design/sales-directions/03-receipt-pad.html` in the real stack. The
composition changes; the behaviour does not. Nothing here asks the server for a
new field and nothing here changes what a sale is.

## Specs

**S1** — The draft is drawn as the slip it becomes. One closed sheet: a top band
carrying its state word and the two context fields, the product entry, the ruled
line rows, and a foot carrying the live total and the action. The artboard shows
it as the document being printed, not as a form that resembles one.

**S2** — Each typed line prints as a product name with its arithmetic beneath it
(`2 × $ 1.250,00 / unidad`) and its own line total aligned right. The two editable
fields stay on the line; the price field keeps its currency sign and unit suffix.

**S3** — The day is a stack of the documents it produced, newest first. Each slip
carries its own number, time, customer, method, state and total. No table composes
the day.

**S4** — The running balance appears in each document's own head, not as a column
of a table. It is still recomputed oldest-first, still struck through on a
document that contributes nothing, and still the statement's signature.

**S5** — Cash closes the day panel from its head, and the period's figures close
it from its foot. All three figures the screen shows today survive: cash on hand,
the period's sum, and what was refunded. None may be dropped to match a sketch.

**S6** — Behaviour is unchanged and no e2e assertion may be weakened to make the
new markup pass. Every selector that moves is migrated to a helper so the next
markup change is one edit; every migrated test must still fail when the behaviour
it covers is broken.

**S7** — A document number is never invented. The draft has no number until the
server assigns one, so the slip's number slot prints the draft's state word
instead of a predicted sequence. The recorded document keeps reporting its own id.

## Tasks

- [x] **T1** — Add `saleDraftLine` and `daySlip` helpers to `tests/e2e/unique.ts`,
  so a markup change is one edit rather than twenty-three. (S6)
- [x] **T2** — Rebuild `SaleDraft.svelte` as the slip: top band with state word and
  context fields, printed line rows, foot with the live total. (S1, S2, S7)
- [x] **T3** — Rebuild the day in `Ledger.svelte` as a stack of slips with each
  document's balance in its own head. (S3, S4)
- [x] **T4** — Recompose `/sales/+page.svelte`: the pad column and the day column.
  (S5)
- [x] **T5** — Migrate the e2e selectors to the helpers and prove each migrated test
  still bites. (S6)
- [x] **T6** — Update `DESIGN.md`'s layout and component sections to the built
  result.

Evidence:

- `make test` — frontend unit 261/261, backend lib+bins 67/67, integration 157/157,
  e2e 34/34.
- Mutation checks (each restored immediately after observing the failure):
  - slip balance changed from `balances.get(row.id)` to `row.total` → the new S4
    test failed with `Expected: 30, Received: 10`.
  - `data-draft-line` removed from the draft line → the stamp test failed at
    `expect(line).toContainText(name)`.
  - `data-doc-row` removed from the slip → the stamp test failed at
    `expect(doc).toHaveCount(1)`.
- `svelte-check`: 0 errors, 0 warnings. `eslint .`: clean. `prettier --check`:
  clean.

## Log

**L1** — user, verbatim:

> Implement the design artboard design/sales-directions/03-receipt-pad.html
>
> Artboard: design/sales-directions/03-receipt-pad.html
> Title: El borrador es el comprobante
> Tradeoff: Most legible per document and the most honest about what is being written; no table anywhere. Scanning a day of thirty slips is slower than thirty rows, and long names wrap.
>
> Implement it in the project's real stack. Use the artboard as the visual spec: map its parts to the project's existing components and styles, and do not copy the artboard HTML verbatim.
> Take colors, typography, spacing, radius and shadows from design/system.md, the project's token source.
>
> Report progress with the gentle_design tool: action "handoff", target "sales-directions/03-receipt-pad.html", status "in_progress" when you start; status "implemented" when it is done, with previewUrl set to the running app's loopback URL (localhost, 127.0.0.1 or [::1]) when one is running, so the canvas can compare it with the artboard.
> This design is not linked to an ODD feature document: size it under the project's ODD rules as usual.
> This handoff itself commits nothing.

**L2** — Slice boundary, recorded because the audit flagged it: this feature lands
as four commits totalling ~5.6k authored changed lines on
`design/receipt-pad-sales`. That is past the ~400-line review slice, and the
reason is structural rather than accidental: the artboard removes the table from
both the draft and the day, so the diff is mostly *removal and replacement* of
existing markup and styles across three files (`SaleDraft` 588, `Ledger` 399,
`sales/+page` 359 changed lines), not new behaviour.

The four commits are the slices, and each one is independently reviewable:

1. `docs(design)` — the design system sync and the three directions. No app code.
2. `feat(sales)` — the draft as the slip.
3. `feat(sales)` — the day as a stack of slips.
4. `test(sales)` — the selector migration, the new S4 test and `DESIGN.md`.

The delivery strategy is therefore a **chained PR of three steps** (1 → 2+3 → 4),
not one PR. Opening the pull request remains the owner's decision.

**L3** — Two follow-ups found while reporting, both fixed or filed on request:

1. **Fixed in `8856aab`** — `/accounts` consumed `var(--figure)`,
   `var(--rounded-control)` and `var(--rounded-panel)`, none declared. Measured in
   the browser: the h1 rendered in `Plex Text` and the panel at `0px` radius;
   after the fix, `Plex Condensed` and `8px`. The retired palette also survived as
   values (`#2b3162`, `#a81b2a`, `#084a6c`, `#f2f6fa`, `#f2efe6`, `#0a1622`) in
   six files, invisible to `retired-tokens.test.ts` because it bans names and
   these were numbers. `tests/unit/design-tokens.test.ts` now guards both halves,
   each proven to bite by reintroducing the defect.
2. **Filed as Gentleman-Programming/gentle-shell#2605** — the design-source walk
   in `gentle-pi` exhausts its 5000-file budget on `backend/target/` before it
   reaches `frontend/`, so this repo always reports "no design sources" and
   `design/system.md` can never be reported stale. Verified by calling the real
   exported function, not a replica. Not fixed here: it lives in a third-party
   package under `node_modules`.

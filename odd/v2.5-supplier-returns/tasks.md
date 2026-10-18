# v2.5-supplier-returns — tasks (Engram mirror)

Full document: `odd/tasks/v2.5-supplier-returns.md` (specs authority, verification).
Design authority: `plan-v2.5.md`. Engram topic key: `tempox/odd/v2.5-supplier-returns/tasks`.

**State: complete (T1–T8).** Backend merged in PR #47 (`4aaeb98`) and ledger
integrity in PR #48 (`a8d2fa6`); the UI (T6–T8) is on `feat/v2.5a-returns-ui`
off `a8d2fa6`.

Returning goods to a supplier: the mirror of v1.5's credit note with the direction
reversed. Stock leaves, money comes back, the purchase stays as recorded.
**Purchase orders are out of scope** (owner's decision, 2026-10-18).

## Specs (one line each — the document is authoritative)

S1 a return is a second document; the purchase is never written.
S2 name/unit/price snapshotted from the corrected purchase line.
S3 the price is the one paid, read server-side; no override, the client sends none.
S4 stock leaves: one **negative** movement per line, `reason='supplier_return'`,
   naming both the purchase and the return.
S5 `cash = sales − credit_notes − purchases + supplier_returns`.
S6 partial and cumulative, bounded per line, serialized under the purchase lock.
S7 `GET /api/purchases` carries `supplier_returns`, oldest first, never absent.
S8 404 unknown purchase/line, 400 empty or non-positive, 409 beyond what remains.
S9 a rejected return writes nothing.
S10 immutability triggers written in the creating migration.
S11 coherence re-cut for the fifth reason, two-sided equality, pre-check BEFORE
    the constraints it protects.
S12 compile-time checking; `.sqlx` current.
S13 test-first with RED observed; existing behaviour pinned where shared code moved.
S14 every assertion self-relative.
S15 docs follow behaviour, with a worked cash example.
S16 work-unit commits with tests and docs.

## Tasks

| ID | Task | Specs | State | Commit |
|---|---|---|---|---|
| T1 | Migration `0010` + integration tests | S1, S2, S4, S10, S11 | done | `f567307` |
| T2 | `POST /api/purchases/{id}/supplier-returns` | S3, S6, S8, S9 | done | `f5aa238` |
| T3 | `supplier_returns` in `GET /api/purchases` | S7 | done | `3b97e25` |
| T4 | `GET /api/cash` nets returns | S5, S13 | done | `c6740cf` |
| T5 | Docs | S15 | done | `1a9e1ec` |
| T6 | Return action in an expanded purchase | S3, S6 | done | `978ff81` |
| T7 | Returns under the purchase they correct | S7 | done | `978ff81` |
| T8 | i18n, both languages | S15 | done | `978ff81` |
| — | `PUBLIC_API_BASE` reaches the bundle again (pre-existing defect) | — | done | `b632431` |

## Verified for T1

- RED first: four tests failed with `relation "supplier_returns" does not exist`.
- **A real defect caught by the positive case**: 0009's
  `stock_movements_purchase_coherent` is `(reason = 'purchase') = (purchase_id IS NOT NULL)`,
  an enumeration a return violates because it legitimately carries a
  `purchase_id`. Re-cut to `(reason IN ('purchase','supplier_return')) = (purchase_id IS NOT NULL)`.
  A refusal-only test would have passed; the positive case named the defect.
- Re-applied cleanly: new tables verified empty, objects reversed, `_sqlx_migrations`
  row 10 deleted, migrator applied the corrected file → **10/10**.
- GREEN: 4 v2.5 tests, full integration suite **35 passed, 0 failed**.
- **Three mutations**, each measured and restored: direction rule, equality rule,
  immutability trigger. Each fails with a message naming the rule.
- 29 bin tests, `make sqlx-check`, `fmt`, `clippy` clean.
- The pre-check ordering is asserted against the migration text.

## Log (tail)

L1 (user, verbatim): "vamos con 2.5 el hueco visual lo cerramos en otro slice cuando definamos bien el UI/UX"
L2 (user, verbatim): "vamos solo con devoluciones, ordenes de compras lo dejamos para otro slice mas adelante"
L3 (user, verbatim): "Despues medimos y probamos. Dejalo como supplier-returns, la nota de de debito se usan para otra cosa, no para devoluciones."

## Independent verification of T6–T8

`assess` said **unassessable** (native binary absent), which the contract treats as
high risk, so a verifier ran over the committed range: **2 blockers + 5 risks**,
all reproduced, all closed.

- **B1** (`13887de`): the return field used `parseQuantity`, which is `null` for
  `0` and for blank — on this form zero means "nothing from this line". One
  emptied field cancelled the whole plan, so a multi-line purchase could only be
  returned **in full**. Measured: zero POSTs. `parseReturnQuantity` fixes it.
- **B2** (`f7bbbfb`): the spec titled "(S8,S9)" read the catalog row, so
  `toContainText('3')` also matched the generated product name — and with stock
  `"0"` the row still contained `"0"` because the price renders `$ 10,00`. It also
  never contacted the server. Retitled, and it now asserts the `.stock` cell.
- **Risks 1–5** (`0ff4453`): recorded return reported as not recorded; one row's
  error under every row; a slow reload undid paging; `loadDay` duplicated;
  `createPurchase` typed narrower than the server answers.

**Final: 14 e2e, 103 unit, lint and check clean.** R1 and R3 are failure-path and
timing behaviours with no e2e here — recorded as known gaps, not claimed covered.

**The lesson worth keeping**: the suite passed 13 e2e and 98 unit tests while
containing two tests that could not fail for what they claimed. Both specs
returned a purchase's only line, so "return everything" was the only path ever
exercised, and the refusal spec asserted against text that contained the digit it
looked for. A green suite is evidence about the paths it walks and nothing more.

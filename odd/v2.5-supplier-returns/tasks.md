# v2.5-supplier-returns — tasks (Engram mirror)

Full document: `odd/tasks/v2.5-supplier-returns.md` (specs authority, verification).
Design authority: `plan-v2.5.md`. Engram topic key: `tempox/odd/v2.5-supplier-returns/tasks`.

**State: in progress.** T1 done on `feat/v2.5-supplier-returns` off `82292a2`.

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
| T1 | Migration `0010` + integration tests | S1, S2, S4, S10, S11 | done | pending |
| T2 | `POST /api/purchases/{id}/supplier-returns` | S3, S6, S8, S9 | pending | — |
| T3 | `supplier_returns` in `GET /api/purchases` | S7 | pending | — |
| T4 | `GET /api/cash` nets returns | S5, S13 | pending | — |
| T5 | Docs | S15 | pending | — |
| T6 | Return action in an expanded purchase | S3, S6 | pending | — |
| T7 | Returns under the purchase they correct | S7 | pending | — |
| T8 | i18n, both languages | S15 | pending | — |

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

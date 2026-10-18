# v2-purchases — tasks (Engram mirror)

Full document: `odd/tasks/v2-purchases.md` (specs authority, verbatim user log).
Design authority: `plan-v2.md` (gitignored working plan, like `plan-v1.md`).
Engram topic key: `tempox/odd/v2-purchases/tasks`.

**State: in progress.** Backend slice v2 (T1–T6), then UI slice v2a (T7–T10).
`main` is `03dc2b2`; nothing pushed.

v2 is the mirror of v1 — a purchase raises stock and lowers cash, suppliers
mirror customers — except where the mirror does not fit, which is the whole
reason S1–S3 exist.

## Specs (one line each — the document is authoritative)

S1 a purchase is a fact: no `status`, no `draft`; if the row exists, it counts.
S2 no `purchase_order_id` reserved; adding the order later needs no backfill.
S3 cash terms only; accounts payable is v4, mirroring credit sales.
S4 every purchase has a supplier, seeded `one-off-supplier`, no delete endpoint.
S5 stored name is English (`one-off-supplier`); display is `Proveedor ocasional`.
S6 money inherited unchanged: `numeric(14,6)`, whole cents, decimal string wire.
S7 lines snapshot name/price/unit; `line_total = quantity * unit_price`;
   `total = SUM(line_total)` by the write path.
S8 a purchase raises stock: one **positive** movement per line, `reason='purchase'`.
S9 the reason vocabulary becomes `initial|sale|credit_note|purchase`, with the
   coherence rule recreated and `0007`'s defensive pre-check.
S10 `purchases`/`purchase_items` reject UPDATE, DELETE and TRUNCATE — written in
    `0009`, not repaired later (the `0007`→`0008` defect).
S11 a rejected purchase writes nothing.
S12 `GET /api/purchases` is day-scoped `[from, to)`, clamps limit/offset.
S13 `GET /api/cash` becomes `sales − credit_notes − purchases`; shape unchanged.
S14 cost derived from purchases, never stored on the product.
S15 compile-time query checking; `make sqlx-check` must pass.
S16 test-first per unit with RED observed; existing behaviour pinned by a test.
S17 integration assertions are self-relative, against the shared dev database.
S18 docs follow the behaviour.
S19 work-unit commits with tests and docs; the last slice closes.

## Tasks

| ID | Task | Specs | Status | Commit |
|---|---|---|---|---|
| T1 | Migration `0009_purchases.sql` + integration tests | S1, S4–S6, S9, S10 | done | `bc189cf` |
| T2 | `suppliers` endpoints | S4, S5 | done | `a61ba7d` |
| T3 | `POST /api/purchases` (transaction, positive movements) | S1, S7, S8, S11 | done | `8bc22fa` |
| T4 | `GET /api/purchases` (day scoping, paging) | S12 | done | pending |
| T5 | `GET /api/cash` nets purchases + existing-behaviour test | S13, S16 | pending | — |
| T6 | Docs | S18 | pending | — |
| T7 | Rail + suppliers screen | S4, S5 | pending | — |
| T8 | `/purchases` register | S7 | pending | — |
| T9 | Purchases ledger | S12 | pending | — |
| T10 | i18n: new strings in both dictionaries | S5 | pending | — |

## Verified for T1 (commands and their observed results)

- RED first: 3 tests failing for the intended reason (tables absent), and one of
  them exposed that a guard test can pass for the wrong reason — see L10.
- 3 v2 integration tests pass; full suite **25 passed, 0 failed**.
- **Three mutation checks**, each dropping a constraint/trigger and restoring it:
  immutable trigger, positive-quantity rule, two-sided coherence rule. The third
  initially **passed** under mutation, exposing a one-directional test; a probe
  for "a `sale` row carrying a `purchase_id`" was added and the mutation then
  fails.
- **The nine migrations apply from scratch** to a throwaway database created and
  dropped by the run, and the tests pass against it.
- `cargo test --bins`: 29 passed. `make sqlx-check`: passes, `.sqlx` unchanged.
- `cargo fmt --check` and `clippy -D warnings` clean.

## Log (tail)

L1 (user, verbatim): "arranquemos a diseñar el plan de v2. no escribas nada de codigo hasta que tengamos escrito y detallado el plan en un .md…"
L3 (user, verbatim): the unified-`status` idea (draft/complete/done/cancelled) — analysed and departed from, with four reasons.
L4–L8 (decisions): two documents; cash only; backend-then-UI slices; supplier seed; no reserved column.
L9 (user, verbatim): "si me cierra, arranca con T1"
L10 (discovery): a guard test can pass for the wrong reason; assert the positive case first.

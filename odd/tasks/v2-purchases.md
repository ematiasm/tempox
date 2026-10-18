# v2-purchases — tasks

Full design: `plan-v2.md` (gitignored working plan, like `plan-v1.md`/`plan-v1a.md`).
Engram topic key: `tempox/odd/v2-purchases/tasks`.

**State: in progress.** Backend slice v2 (T1–T6), then UI slice v2a (T7–T10).
Nothing is merged; `main` is `03dc2b2`.

v2 is the mirror of v1: a purchase raises stock and lowers cash, exactly as a sale
lowers stock and raises cash. Suppliers mirror customers. What the mirror does
**not** fit is stated in `plan-v2.md` and summarised in S1–S3 below.

## Specs

S1 a purchase is a **fact**, not an intention: no `status` column, no `draft`. If
   the row exists, it counts — in stock, in cash, in every derived number. The
   purchase-order document is deferred to v2.5 and is additive there.
S2 no `purchase_order_id` column is reserved in v2. Adding the order later is a
   nullable `ALTER TABLE` with no backfill; an all-NULL unused column is the same
   dead vocabulary S1 rejects for `status`.
S3 cash terms only: recording a purchase lowers cash by its total. Supplier
   payments (accounts payable) arrive with v4, mirroring credit sales.
S4 every purchase has a supplier — FK `NOT NULL`, seeded `one-off-supplier` with
   the fixed id `00000000-0000-0000-0000-000000000002` as the default, no delete
   endpoint on the seed. Mirrors the walk-in customer (v1 decisions 5, 11).
S5 the seeded supplier's **stored** name is the English string
   `one-off-supplier`; the es-AR display string (`Proveedor ocasional`) lives in
   the frontend dictionary. The stored string is a contract: it reaches the API
   and exports, so renaming it is a data migration.
S6 money is inherited unchanged: `numeric(14,6)`, `CHECK (v = round(v, 2))`,
   `CHECK (v >= 0)`, `Decimal` in Rust, a decimal string on the wire.
S7 a line snapshots `product_name`, `unit_price` and `unit`, and its
   `line_total = quantity * unit_price`; `purchases.total = SUM(line_total)` by
   the write path. Same invariants as v1, enforced by one pure tested function.
S8 a purchase raises stock: one **positive** `stock_movements` row per line per
   product id, `reason = 'purchase'`, carrying `purchase_id`.
S9 the ledger's reason vocabulary grows to `initial | sale | credit_note |
   purchase`, and the coherence rule is recreated for the four reasons, with
   `0007`'s defensive pre-check before the constraint is added.
S10 `purchase_items` and `purchases` are immutable: `UPDATE`, `DELETE` and
    `TRUNCATE` are all refused by triggers written in `0009` — not repaired in a
    later migration (the `0007`→`0008` defect).
S11 a rejected purchase writes nothing: no header, no lines, no movements.
S12 `GET /api/purchases` is day-scoped with the same half-open `[from, to)`
    contract as the sales ledger, and clamps `limit`/`offset`.
S13 `GET /api/cash` becomes `sales − credit_notes − purchases`. The response
    shape does not change, so no consumer breaks, but its meaning does and the
    docs must say so.
S14 cost is derived from the purchases themselves, never stored on the product.
S15 compile-time query checking is inherited: new queries regenerate `.sqlx` via
    `make sqlx-prepare`, and `make sqlx-check` must pass.
S16 test-first per unit, with RED observed before the implementation; existing
    behaviour preserved by a test where shared code changed.
S17 every integration assertion is **self-relative** (measure before, act,
    measure after) and never absolute, because the suite runs against the shared
    development database. A clean database is requested from the owner, who runs
    `docker compose down -v`; no parallel stack, no image rebuild.
S18 docs follow the behaviour: `docs/en/architecture.md`, `README.md`,
    `PRODUCT.md`.
S19 work-unit commits carry their tests and docs; the last slice closes.

## Tasks

| ID | Task | Specs | Route | Status | Commit |
|---|---|---|---|---|---|
| T1 | Migration `0009_purchases.sql` + its integration tests (guards, coherence, seed, sign) | S1, S4–S6, S9, S10 | inline | done | `bc189cf` |
| T2 | `suppliers` endpoints (`GET`, `POST`, 400/409) | S4, S5 | inline | done | `a61ba7d` |
| T3 | `POST /api/purchases` — the transaction: header, lines, positive movements | S1, S7, S8, S11 | inline | done | `8bc22fa` |
| T4 | `GET /api/purchases` — list with items, day scoping, paging | S12 | inline | pending | — |
| T5 | `GET /api/cash` nets purchases + its existing-behaviour test | S13, S16 | inline | pending | — |
| T6 | Docs: architecture, README, PRODUCT | S18 | inline | pending | — |
| T7 | Rail destination + suppliers screen | S4, S5 | inline | pending | — |
| T8 | `/purchases` register: supplier picker, lines, live total, keyboard | S7 | inline | pending | — |
| T9 | Purchases ledger: day-scoped, stamped | S12 | inline | pending | — |
| T10 | i18n: every new string in both dictionaries, shop vocabulary | S5 | inline | pending | — |

## Verification log

Each line is a command that ran and its observed result.

### T1 (2026-10-17)

- **RED observed first**, three tests, all failing for the intended reason (the
  tables did not exist), and one of them proved the trap this slice had to avoid:
  `v2_the_ledger_refuses_a_purchase_movement_without_its_document` **passed
  before the migration existed**, because `reason = 'purchase'` was not yet in
  the vocabulary, so its "refusal" was the wrong refusal. Fixed by asserting the
  **positive** case first (a coherent purchase movement must be accepted) so the
  test cannot be satisfied by a missing feature. That is the RED-is-not-enough
  lesson recorded here because it is cheap to state and expensive to rediscover.
- `cargo test --test integration v2_ -- --ignored --test-threads=1`:
  **3 passed**.
- **Mutation checks, each measured by dropping the constraint or trigger, running
  the test, and restoring it:**
  - `DROP TRIGGER purchases_immutable` → `v2_a_purchase_is_immutable…` fails with
    "purchase update was allowed".
  - `DROP CONSTRAINT stock_movements_purchase_is_incoming` →
    `v2_the_ledger_refuses…` fails with "a negative purchase movement was
    allowed".
  - `DROP CONSTRAINT stock_movements_purchase_coherent` → the test **passed**,
    which exposed a real hole: the two-sided rule was only tested in one
    direction. A probe for "a `sale` row that also carries a `purchase_id`" was
    added; the mutation then fails with "a sale movement carrying a purchase_id
    was allowed". The constraint is what refuses it — a one-sided
    `purchase => purchase_id` check would not.
- **The migration applies from scratch.** A throwaway database
  (`tempox_migcheck`, created and dropped by the run) received all nine
  migrations in order, and the three v2 tests pass against it unchanged. This is
  the environment CI uses, and the only one that proves the migration does not
  depend on this development database's history.
- `cargo test --test integration -- --ignored --test-threads=1` (full suite):
  **25 passed, 0 failed** — all 22 pre-existing tests plus the 3 new ones.
- `cargo test --bins`: **29 passed**.
- `make sqlx-check`: passes, `.sqlx` unchanged (`0009` adds no query).
- `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean
  after one real fix (`format!("{table}")` → `table`, flagged by
  `clippy::useless_format`).

## Log

L1 (user, 2026-10-17, verbatim): "arranquemos a diseñar el plan de v2. no escribas nada de codigo hasta que tengamos escrito y detallado el plan en un .md. Tanto Compras como proveedores son espejo del plan uno asi que gran parte ya esta decidido. Haceme preguntas si encontras alguna desicion de estructura o diseño"

L2 (user, 2026-10-17, verbatim): "si separemos en slices. Después a que te referis con no modelar orden de compra no recepción parcial?" — the question about purchase orders and partial reception, answered in `plan-v2.md`.

L3 (user, 2026-10-17, verbatim, the design idea this slice departed from):
"EN este slice quiero algo simple, pero existe la posibilidad de que el dia de mañana poder agregar una orden de compra sin romper todo? Mi idea original era usar el "status" de las compras para que draft signifique la orden de compra, complete signifique que llego, done que se pago y cancell que fue cancelada (con su documento de devolucion, espejo de nota de credito en ventas). Pero estoy abierto a que analicemos los diferentes pro y contras y si tenes o conoces otra opcion mas eficiente o robusta."

L4 (decision, 2026-10-17): two documents instead of one with a status — `purchases` as the fact, `purchase_orders` deferred to v2.5. The four reasons are in `plan-v2.md` ("Why not one document with a status"): `draft` breaks "if the row exists, it counts"; agreed vs actual price become ambiguous; partial reception becomes destructive; `done = paid` pulls v4's accounts payable forward and breaks symmetry with cash-only sales. Owner approved.

L5 (decision, 2026-10-17): cash terms only, mirroring v1's cash-only sales; supplier payments are v4.

L6 (decision, 2026-10-17): slices split — backend first (v2), UI after (v2a), the same shape v1→v1a proved.

L7 (decision, 2026-10-17): supplier mandatory with a seeded `one-off-supplier`, mirroring the walk-in customer; cost derived from purchases and never stored on the product; no `status` column on `purchases`.

L8 (decision, 2026-10-17): the reserved `purchase_order_id` column proposed during discussion was **dropped** — an all-NULL UUID with no FK is the dead vocabulary S1 rejects, and adding the order later needs no backfill. Owner approved.

L9 (user, 2026-10-17, verbatim): "si me cierra, arranca con T1"

L10 (discovery during T1, 2026-10-17): a guard test can pass for the wrong reason.
The coherence test was green before the migration existed, because the reason it
expected to be refused (`purchase`) was refused by the *vocabulary* instead of by
the *coherence rule*. Asserting the positive case first is what distinguishes a
working guard from a missing feature, and it is now the pattern for every guard
test in this slice.

### T2 (2026-10-17)

- **RED observed first**: `v2_suppliers_list_and_create` failed with a `404` from
  `POST /api/suppliers` — the route did not exist. The failure named the missing
  feature, not a broken assertion.
- **An incident, recorded because the repo already documented it and I repeated
  it anyway.** `0009` was first applied with `psql -f` instead of through the
  migrator. `plan-v1.5`'s L11 says exactly this: *"a migration is applied by its
  migrator, including the first time; `psql -f` is a syntax check at best"*. The
  schema existed with **no row in `_sqlx_migrations`**, so the migrator then
  refused `0009` with `constraint "stock_movements_purchase_coherent" … already
  exists` — while the real cause was that it had never been applied. The repair
  was safe because every table was verified empty first (`purchases` 0, `items` 0,
  purchase movements 0, suppliers 1 seed row): the 0009 objects were reversed by
  hand and the migrator applied the file from scratch, reaching 9/9.
- **The re-run bug that incident exposed is a real defect in `0009`, now fixed.**
  Because the file has `CREATE TABLE IF NOT EXISTS` at the top and the guard
  assertion in the middle, a second attempt recreated empty tables and reported
  the *constraint* that failed next instead of the *guard* that was absent. Two
  changes: every `ADD CONSTRAINT` is now preceded by `DROP CONSTRAINT IF EXISTS`
  so the file is re-runnable, and the guard assertion moved to **last**, after
  every `CREATE TRIGGER`, where its failure names the real cause.
- **The host binary, not an image.** The running backend container is the old
  build, and rebuilding it would re-tag the owner's `tempox-backend:latest` —
  the hazard `plan-v1.5`'s L21 records. So the branch's binary ran from the host
  on port 3099 against the same development database, which is the rule that
  governs this repo (L22: one stack, one database, one backend image; the binary
  is the branch's code). Stopped afterwards; the compose stack is untouched.
- `cargo test --test integration v2_suppliers…` against the host binary:
  **passed**; then the four v2 tests: **4 passed**.
- Full integration suite against the host binary: **26 passed, 0 failed**.
- `cargo test --bins`: 29 passed. `make sqlx-prepare` regenerated the cache (two
  new query files committed); `make sqlx-check` passes.
- `cargo fmt` clean; `cargo clippy --all-targets -- -D warnings` clean.
- All nine migrations apply **from scratch** through the migrator to a throwaway
  database created and dropped by the run, with `0009` last.

### T3 (2026-10-17)

- **RED observed first**: both new tests failed against the old binary — `404` on
  `POST /api/purchases` (route absent) and the rejection cases failing at the
  first assertion because every shape got `404`.
- **GREEN**: 6 v2 tests pass against the branch binary on port 3099; full
  integration suite **28 passed, 0 failed**; 29 unit tests.
- **Mutation check, and the first one was not the check I wanted.** Inverting the
  movement sign (`+quantity` → `-quantity`) made the test fail — but with `500`,
  because the database's `stock_movements_purchase_is_incoming` constraint
  refused it. That measures the *constraint*, not the handler's arithmetic, so
  it was not the evidence the slice needed. The mutation that does prove the test
  bites removes the movement write entirely: stock delta measured **0 instead of
  6**, failing on the assertion whose message names the rule. Handler restored
  byte-identical afterwards (`diff` against a saved copy: no output).
- **A false green caught and discarded.** One mutation run reported FAILED while
  the backend was actually down (`ConnectionRefused`). A test that fails because
  the server is absent is not evidence about the mutation. Re-run with the
  mutated binary genuinely serving, then with the restored one.
- The mixed rejection case — a valid line followed by an unknown product — is
  what proves every line is resolved *before* the first write. Checking per line
  inside the insert loop would have written the valid prefix and returned 404.
- `make sqlx-prepare` regenerated the cache (5 new query files); `make
  sqlx-check` passes. `cargo fmt --check` and `clippy -D warnings` clean.

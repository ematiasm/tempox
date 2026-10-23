# v3-accounts — accounts, wallets and payment methods

Full design and the long reasoning: `plan-v3.md` (gitignored working plan, like
`plan-v1.md`/`plan-v2.md`/`plan-v2.5.md`).
Engram topic key: `tempox/odd/v3-accounts/tasks`.

**State: designed, not implemented.** Every spec below is a decision; none is a
measurement. The schema facts quoted here were measured on `bae271c` and are
named with their file and line so they can be re-checked instead of believed.

v3 answers one question the system has never been able to answer: **where did the
money move?** Every amount since v1 says *how much* and nothing about *where*,
because every one of them was hardcoded to the till. `sales.payment_method` was
reserved at `0002` with `DEFAULT 'cash'` and **nothing writes it but the
default**; `0003`'s `guard_sale_economics` then froze it alongside `total` and
`customer_id`. `purchases`, `credit_notes` and `supplier_returns` have no
destination column at all.

## Specs

The `is_default` flag is configuration, not authorization. `POST` and `PATCH`
account writes may set it; PATCH can unset it on a different row; the partial unique
index guarantees at most one true. The seeded cash account is the default till.


S1 a payment method belongs to **exactly one** account; an account has zero or
   more methods. An event points at the **method**, and the method resolves the
   account. Never the reverse: a method shared across accounts makes every
   per-account balance unreconstructable without a further allocation table,
   which is the traceability this slice exists to create.

S2 a method's **identity** — its `account_id` — is immutable once any document
   names the method. Enforced by a trigger using `IS DISTINCT FROM`, modelled on
   `guard_product_unit` (`0003`): the guard fires only when the value actually
   changes, and it is conditioned on history existing. A method with no history is
   a typo and must be correctable, exactly as a seeded account created in the
   wrong place must be; a method any document named is history and its balance
   must never move underneath it. Because `name` is editable (S3), `account_id`
   has to be editable too while unreferenced, or a method attached to the wrong
   account could never be fixed.

S2a **`S2` is the guard `products` is missing, and that is a follow-up, not v3.**
   `guard_product_unit` (`0003`) covers only `unit`; `name` and `list_price` stay
   editable on a product that already has sales because those values are
   snapshotted into `sale_items`. The gap is the `initial` stock movement, which
   carries no snapshot of anything: editing a product's name rewrites what an
   `initial` movement appears to have been. Recorded here so it is not lost. It is
   **not** in v3's scope, and fixing it is a separate issue with its own tests.

S3 payment methods are **editable by name** — a typo must be fixable — and are
   **never deleted**. No price, no category, no delete. `is_active` retires a
   method from the picker while its history stays readable and its documents keep
   resolving. Renaming is safe because every document stores the method's **id**
   and the name is read by join (S6a); deleting is not, because it would erase the
   explanation of a balance. This differs from `suppliers`, where `POST` allows no
   edit at all, and the difference is stated on purpose: a supplier's name reaches
   no derived figure, while a method's account decides one.

S3a accounts are editable by name and never deleted, for the same reasons, and
   `is_active` retires one the same way. Creating accounts and methods is a
   **write path v3 must ship** (T0 below).

S4 accounts are seeded with fixed ids, exactly as `walk-in`
   (`…0000000001`) and `one-off-supplier` (`…0000000002`) are: account `cash` =
   `00000000-0000-0000-0000-000000000003` (seeded with `is_default = true`),
   method `cash` = `00000000-0000-0000-0000-000000000004`; account `credit` and
   method `credit` are also seeded in v3, with the method born inactive. The
   **stored** names are English and are a contract (they reach the API and any export); "Caja",
   "Efectivo" and "Crédito" live in the frontend dictionary, which is how
   `walk-in` survived es-AR unchanged. `accounts` is `id, name, is_default,
   created_at`, with `is_default BOOLEAN NOT NULL DEFAULT false` and the partial
   unique index `accounts_one_default_idx` allowing at most one true row. It carries
   **no `kind`**: see the note under S8 on why the classification was withdrawn.

S5 `payment_method_id` is `NOT NULL` on `sales`, `credit_notes`, `purchases` and
   `supplier_returns`. The migration backfills the whole existing history with the
   seeded `cash` method, and the justification is stated rather than assumed: that
   is the only destination which existed (`0002`'s `DEFAULT 'cash'` says so), so
   the backfill records a fact instead of inventing one. Excluding history by
   making the column nullable is refused: it would create two classes of document
   and every consumer would need two code paths forever.

S6 `VIEW money_movements` is the single definition of "money moved": one row per
   **effect**, carrying `account_id` resolved by join, a signed `amount`, the
   document kind and its id, and `occurred_at`. It is a read model — nothing is
   written, indexed or validated there, and the docs must say so.

S6a the method's **name is never stored on a document**: every document stores
   `payment_method_id` and the name is resolved by join, the way
   `c.name AS customer_name` is (`sales.rs:538`). The `product_name`-as-column
   precedent (`0002:33`) does **not** apply: a product's name is snapshotted so a
   rename never rewrites what was sold, while a method's rename is a correction the
   shop is allowed to make and the document's identity is the id, not the spelling.
   Join by id is the safer of the two, and this was corrected once already — an
   earlier draft made the name a column on the argument that methods have no edit,
   which S3 now contradicts.

S6b the wire carries the method **flat**: `GET /api/sales` and `GET /api/purchases`
   return `payment_method_id` beside `payment_method_name` at the top level of the
   document, with **no `account_id`** and no nested object:

       { "payment_method_id": "00000000-0000-0000-0000-000000000004",
         "payment_method_name": "Efectivo" }

   **One annotation on that sample, so the pair is not implemented wrongly:** S4 makes
   the **stored** name English (`cash`, `credit`) and puts "Efectivo"/"Crédito" in the
   frontend dictionary, so the join returns `cash` and the client renders "Efectivo".
   The shape is what this sample fixes — flat, side by side, no `account_id`, no
   nesting — and the string in it is the display, not the stored value.

   This is a **rename argument**, and that is the one that settles it. Renaming a
   method is a single `UPDATE` of one row (`PATCH /api/payment-methods/{id}`);
   documents store the id, so **nothing else changes**: no document, no balance, no
   `money_movements` row, no id. The only thing that changes is the value of
   `payment_method_name` in responses — and that is the **desired** behaviour:
   renaming **corrects** history instead of freezing it. A three-month-old typo
   shown on old documents becomes correct once it is fixed. A stored
   `payment_method_name` column (the `product_name` pattern, `0002:33`) would do the
   opposite: every old document would keep the typo forever, unfixable without a
   mass `UPDATE` over immutable rows. That is why the snapshot is right for a
   product — Coca-Cola 500ml becomes 473ml, and yesterday's sale must still say what
   was sold — and wrong for a method, whose name changes because it was mistyped.
   The hybrid — keeping the legacy `payment_method` string beside the id — was
   rejected because it creates **two sources of the name**, and the old one goes
   stale at the next rename: a bug waiting for the first reader of the legacy field.
   What a rename still cannot touch is `payment_methods.account_id`: with documents
   present that is blocked by S2. The name is free; the account is not. `account_id`
   therefore appears only in `money_movements` (L20), where method → account is
   translated once.

S7 the view is written to admit **N rows per document** from the start. A sales
   credit note and a supplier return each produce one effect today. A collection in
   the slice immediately after v3 is one document with N method items: its view
   expands those items into N destination-account inflows plus one debt-account
   outflow for the document total. A transfer (in a later slice) has one account
   outflow and another account inflow. A view written on "one row per document"
   would have to change shape the moment credit exists, which is exactly what
   choosing a view was supposed to avoid.

S8 an account balance is `SUM` over the view. **Never a column.** The till is
   the account whose `is_default` is true; the server exposes that configuration
   through `/api/accounts`, so the client needs neither a hardcoded UUID nor a
   classification enum. `is_default` has two real consumers — the `/sales`
   footer and the sales payment-method picker default — so this is used
   configuration, not preventive classification.

S8a **`kind` was withdrawn, and the reason is recorded because it was argued and
   then reversed.** An owner question asked what `kind` was for, and the honest
   accounting is that it bought three things and only one had a consumer: locating
   the till (now covered by `is_default`), validating a credit-document destination (the later
   credit/collection slice), and distinguishing cash from bank from wallet — which
   in v3 are **indistinguishable**, since nothing treats them differently. A
   three-value CHECK of which one value is used is the dead vocabulary
   `plan-v2.md`'s S2 rejected for a reserved `purchase_order_id`. The decisive
   asymmetry: `accounts` is a configuration table, so adding a classification in
   v4 is an `ALTER TABLE` with a default over a handful of rows, while adding it
   now costs dead vocabulary in an immutable-adjacent contract. `/accounts`
   therefore renders a flat list of accounts with their derived balances, with no
   "available total" figure — that number was invented by the plan, not requested.

S9 a credit sale may not name `walk-in`; this is an active v3 guard because the
   `credit` account and method exist. The test asserts the **positive** case first
   (a credit sale to a named customer is accepted) and only then the refusal
   (credit sale + `walk-in` returns 400), so it proves the intended guard rather
   than an earlier validation failure.

S9a `is_active` is a **picker concern only**: the API accepts an inactive method on
   a write. Otherwise the S9 refusal could pass merely because the seeded `credit`
   method is inactive — the wrong-reason trap `v2`'s T1 recorded, and the reason
   this is a spec rather than an implementation detail.

S9b the mirror guard also ships in v3: a **purchase may not name the `credit`
   method**, because its effect would subtract from the credit account ("what
   customers owe went down by 40,000"). Same shape as S9, positive case first.

S10 `credit_notes` **inherit** the method of the sale they correct, and **may name
    a different method explicitly** — both halves are the decision, and neither is
    optional. The inherited value is the default so the counter path stays free;
    the override exists so a sale already collected can be refunded out of a real
    account instead of leaving a credit balance. The client names **the method,
    never the `account_id`**; the server resolves the account, exactly as a
    supplier return never sends a price. `supplier_returns` inherit the purchase's
    method with **no override** in v3. The "a note may not target the debt account"
    rule is part of the credit-sales/collections slice immediately after v3.

S11 `sales.status` and `sales.reversed_at` are dropped. Both are dead: no write
    path in `backend/src` ever sets either, and `status`'s only use is
    `WHERE status = 'confirmed'` in two reads. Pre-check **before** any `ALTER`:
    0 rows with
    `status <> 'confirmed'` and 0 rows with `reversed_at IS NOT NULL`.
    `guard_sale_economics` is re-cut with `IS DISTINCT FROM` for the columns that
    remain, because `NULL <> value` is `NULL` and a guard that silently stops
    guarding is the defect `0011` was written to kill.

S12 a sale reads as annulled **only** from its credit notes. `Ledger.svelte:357`
    loses its `status === 'cancel'` branch, which is dead today and would be a
    lie the moment the column disappears.

S13 `GET /api/cash` is removed in v3. The `/sales` footer reads `GET
    /api/accounts` and selects the row with `is_default = true`; that row's balance
    is the till. The old endpoint and the Caja row would return the same number,
    since both read the same view, so removal eliminates duplicated surface, not
    duplicated logic. Security was considered and is not the reason: the app has
    no authentication (auth is v5), `GET /api/accounts` already returns every
    account and balance, and a UUID is an identifier, not an authorization token.
    The server, not the client, owns the business rule of which account is the
    till; client-side UUID hardcoding was rejected as a design defect.

S14 immutability guards ship **in the migration that creates the tables**
    (`0007`→`0008`'s lesson); every `ADD CONSTRAINT` is preceded by a defensive
    pre-check **before** it (`0009`'s ordering defect); and the guard-existence
    assertion runs **last** (`0009`/`0010`). Each `ADD CONSTRAINT` is preceded by
    `DROP CONSTRAINT IF EXISTS` so the file is re-runnable.

S15 `.sqlx` is regenerated by `make sqlx-prepare` and `make sqlx-check` passes; no
    query ships without compile-time checking.

S16 test-first per unit with RED observed before the implementation, and one test
    per existing behaviour that shares the code being changed. Specifically:
    `POST /api/sales` **without** a payment method must keep behaving exactly as
    it does today (default `cash`), proven by a differential run against the base
    commit in a `mktemp -d` scratch copy — never committed as a test.

S17 integration assertions are **self-relative** (measure before, act, measure
    after) and never absolute. No throwaway stack, no throwaway database, no
    backend image rebuild: `docs/en/testing.md` is the authority and this slice
    runs `cargo run` on the host like every other.

S18 docs follow the behaviour: `docs/en/architecture.md` (the API table, the
    invariants, and the breaking change of `payment_method` → `payment_method_id`),
    `PRODUCT.md`, `PLAN.md`, `README.md`.

S19 work-unit commits carry their tests and their docs; a unit whose Risk line
    names a Task Size high-risk item gets its own commit so native review can take
    it alone; the last slice closes the feature.

S20 **out of scope, stated so it is not rediscovered as a surprise**: expenses,
    transfers, partial reception, multi-currency, bank reconciliation, persisted
    sale drafts, and identity/permissions. And `stock_movements` is **untouched**: a
    destination is not a stock reason, so `0011`'s coherence rule stays as it is.
    Two deferred slices, **in this order**:

    1. **The next slice, immediately after v3**: credit sales plus
       `customer_collections` — the collection document with items per payment
       method, **stored** allocation of each collection to the sales it settles
       (S21), the resulting credit balance applicable to the customer's next sale
       (L43), and activating the v3-seeded `credit` account/method. This is the
       slice that also closes the mixed-payment gap v3 declares (S22).
    2. **A later, separate slice**: `supplier_payments` (buying on credit and
       paying later), which mirrors the collection document exactly — items per
       method plus stored allocation — and is **not** part of slice 1 (L39). The
       payables side therefore does not close in slice 1; today every purchase is
       cash-only, so nothing is broken yet.

    v3 additionally ships with a declared **gap in what it can record**: no mixed
    payment on a cash sale (S22, and the known-limitations block below). None of
    the deferred features ships inside v3.

S21 **the slice after v3 STORES the allocation; this corrects L11's model.** The
    research of `L41` found **three** layers where the plan had one:
    **obligation** (what is owed — a sale, a purchase, an invoice), **settlement**
    (the money moving on a rail, carrying the payment method) and
    **allocation/reconciliation** (which settlement paid which obligation, and how
    much). The plan let the obligation carry the method
    (`sales.payment_method_id`) and derived the third layer with FIFO at read
    time. Every system surveyed stores it: Odoo proposes matches and then
    **writes** `account.partial.reconcile` (`debit_move_id`, `credit_move_id`,
    `amount`); ERPNext's Payment Entry names the invoices it settles; SAP's
    clearing strategy is configurable while its **result** is stored; iDempiere
    allocates payments to invoices as rows. The rule that names the defect: **the
    matching policy is an algorithm; the matching result is a fact.** A derived
    answer lets a past invoice's state change retroactively when the policy or the
    data changes, and makes reassignment impossible — there is no row to change.

    The self-inconsistency that proves it is already in this repository: money
    going **out** has stored, explicit allocation — `credit_note_items.sale_item_id`
    declares which sale line a refund corrects, guarded by a row lock and a
    cumulative `refunded + requested <= sold` check (`sales.rs:300-357`,
    `0007_credit_notes.sql:46`). The plan applied the **opposite** criterion to
    money coming **in**. Mirror cases given opposite treatments is the defect
    behind `L40`.

    The shape (next slice, never v3):

    | table | columns |
    |---|---|
    | `customer_collections` | `id, customer_id, occurred_at` |
    | `customer_collection_items` | `collection_id, payment_method_id, amount` — N methods per act |
    | `customer_collection_allocations` | `collection_id, sale_id, amount` — the imputation |

    - The allocation is **stored**, never derived by policy. FIFO becomes a **UI
      proposal** ("apply to the oldest first") that the user confirms, and the
      confirmation is what is written. The cashier still never chooses an invoice,
      which is the affordance `PRODUCT.md` protects: the objection was to
      *asking*, not to *storing*.
    - `unallocated_amount` is **derived** as `total − SUM(allocations)`, and no
      column exists. This is safe where derived FIFO was not because it is exact
      arithmetic with no policy in it — the same reason `stock = SUM(movements)`
      is safe. A subtraction of stored rows cannot drift.
    - Two write-path guards, **inside the row lock**, mirroring
      `create_credit_note` (`sales.rs:300-357`): `allocation <= sale.total − its
      credit notes − its prior allocations`, and `SUM(allocations of a collection)
      <= collection.total`. The excess is **not rejected**; it stays unallocated,
      which is exactly how the accepted overpayment (`L13`) becomes representable
      instead of special-cased.
    - Allocations are **immutable**; a correction is a compensating negative row,
      never an edit — the rule every document here already follows (`0007`'s
      immutability trigger is the precedent).
    - **A credit balance can be applied to the customer's next sale** (`L43`), and
      to the supplier's next purchase in the mirror: a new allocation drawn from a
      collection that still has unallocated amount, against the new obligation. No
      money moves — the cash already moved when the collection was recorded — so
      it is pure allocation with **zero ledger effect** and therefore no
      `money_movements` row. Choosing *which* collection to draw from
      (oldest-first with free amount) is a policy whose result is stored, so the
      defect does not return through the credit-balance door.
    - Allocations write rows here, not in `money_movements`: the money moved when
      the collection was recorded, so `S6`'s sign convention is unchanged and an
      allocation row there would double-count the inflow.
    - Two origins of a credit balance are indistinguishable today and do not need
      separating yet: an **advance** (money handed over before a purchase) and a
      **not-yet-allocated collection**. A column telling them apart would be
      vocabulary with no consumer — `S8a`'s rule.

S22 **v3 ships with one declared gap: it cannot record a mixed payment on a cash
    sale.** `sales.payment_method_id` is a single `NOT NULL` FK (S5), so half cash
    and half card is unrepresentable; Odoo has modelled N payments per order all
    along (`pos.payment.pos_order_id`). Owner's decision (`L44`): declare the
    limitation and ship, because the settlement layer arrives with the next slice
    (S21) and re-cutting `sales` before then buys nothing. The declaration below is
    written as a real gap with **no workaround**, not as a preference.

### Known limitations of v3

Declared, dated, and not preferences. This block is the record; a lifted
limitation is annotated here, never deleted.

**Mixed payment on a cash sale cannot be recorded — declared 2026-10-22 (`L44`).**
`sales.payment_method_id` is one `NOT NULL` FK (S5), so a sale paid 5.000 in cash
and 5.000 by card has no representation. Writing one method loses half of where the
money went; writing two sales invents a second sale, which is `L38`'s argument
applied to the same problem — the counter act's identity cannot be recovered
without a grouping column, and the wall clock is forbidden as an identity source
(`AGENTS.md`). **There is no workaround in v3**: not a nullable column (S5 refuses
two classes of document), not a memo field (it would store a number nothing reads),
and not two documents. The fix is the settlement layer of the next slice — one
document with N method items, the shape `S7` already requires the view to admit —
and until it lands the shop records **one** method per cash sale. Consequence named
rather than discovered at the counter: a mixed cash/card sale understates one
account and overstates another by the split.

### What v3 gets right and must not be rebuilt

`sales.payment_method_id` is a valid **degenerate case** of the general model, not a
wrong design awaiting replacement: it says "the settlement was one payment,
complete, at the moment, by this method", which is exactly
`customer_collection_items` with N = 1. The next slice **extends** it; it does not
migrate away from it, and a rebuild that treats the single FK as the defect has
misread both S5 and S21. Credit is right for the same reason: the credit sale
debits the debt account and the later collection credits it, and **that is double
entry**, not a special case bolted onto one. Neither the column nor the credit
model is a candidate for rework when S21 lands.

## Tasks

Order is by layers on purpose (owner's choice): the whole backend lands before any
screen, which keeps integration risk in the backend where the tests already run.

**The rows below are in execution order; the ids are not a sequence.** Execution
order is **T1 → T0 → T3 → T4 → T5 → T6 → T7 → T8**, then **T2** as the closing
commit. The ids are kept stable on purpose — the Log cites them — so T0 sits after
T1 despite the smaller number, and T2 sits last despite being the middle id. An
earlier version of this table listed T0 first, and that was a defect rather than a
typo: T0 is the HTTP write path for the tables T1 creates, so it could not be built
or tested before T1; it sat there only because it was appended later (L46). **Do not
re-sort these rows by id.** T2 drops two columns and goes last because a destructive
migration must not sit on the critical path of the other tasks: placed last,
everything else is built against the current schema and the drop is reviewed alone,
with no other change competing for attention (L49).

| ID | Task | Specs | Route | Status | Commit |
|---|---|---|---|---|---|
| T1 | Migrations `0012` (accounts, payment methods, seeds, guards) and `0013` (four destinations, backfill, drop of `sales.payment_method`) | S1–S5, S8a, S14 | inline | done | `050779b` |
| T0 | `GET`/`POST /api/accounts` and `GET`/`POST /api/payment-methods`, plus `PATCH` for name, `is_active` and `is_default`; account creation may set `is_default`; PATCH can set it or unset it on another account; at most one true (partial unique index); 409 on duplicate name, 400 on blank (trimmed first). Depends on T1 — it writes the tables T1 creates | S2, S3, S3a, S4 | inline | pending | — |
| T3 | Migration `0014`: `VIEW money_movements` + derived `balance` on `GET /api/accounts` | S4, S6–S8 | inline | done | `da53a17` |
| T4 | Sales write path and flat wire (S6b): `payment_method_id` + `payment_method_name` side by side at the top level, resolved by join (never `account_id`, never a name column, never a nested object), picker defaults to an active method belonging to the `is_default` account; API accepts inactive methods; enforce S9 walk-in guard | S4, S5, S6b, S8, S9 | inline | pending | — |
| T5 | Purchases write path (same flat wire as T4), plus inheritance in credit notes (with override) and supplier returns; reject the `credit` method on purchases in the write path | S5, S6b, S9b, S10 | inline | pending | — |
| T6 | Retire `/api/cash`; the footer reads the default account's derived balance from `/api/accounts` | S8, S13 | inline | done | `6d96958` |
| T7 | Frontend: `/accounts` screen, method column in both ledgers, annulment derived from notes only | S12 | inline | pending | — |
| T8 | i18n both dictionaries, `architecture.md`/`PRODUCT.md`/`PLAN.md`, e2e over a sale on a non-cash method | S18, S19 | inline | pending | — |
| T2 | **Closing commit, alone, after T8.** Migration `0013`: drop `status`/`reversed_at` with pre-check, re-cut `guard_sale_economics`. Risk item 1 — destructive over immutable history | S11 | inline | pending | — |

Deferred work with its reason, so it is not rediscovered as a surprise:
**transfers** (owner's decision, 2026-10-22) belong with expenses in v4 because a
transfer and an expense share one shape — a document that moves money and touches
no stock — and building that machine twice is waste. The accepted consequence is
named: cash and bank balances drift from reality until v4 lands. The next slice,
immediately **after v3**, is credit sales plus `customer_collections`, by the
owner's explicit decision. It includes one collection document with items per
payment method, **stored** allocation of each collection to the sales it settles
with FIFO reduced to a confirmed UI proposal (S21), overpayment kept as
unallocated amount and therefore a credit balance in the customer's favour, that
credit balance applicable to the customer's next sale, and activating the
v3-seeded inactive `credit` account/method. Nothing in that credit/collection
slice ships inside v3. The mixed-payment gap is v3's own, declared in S22 and in
the known-limitations block; it closes with that slice.
`supplier_payments` (buying on credit and paying later) is a
separate, later slice, not part of that next slice; the payables side therefore does
not close in the same slice. Today every purchase is cash-only, so nothing is
broken yet. **Persisted drafts** are out (owner's decision, 2026-10-22): the cart
lives in the browser (`frontend/src/lib/draftLines.ts`) and nothing about it is a
document.

**T0 exists because the plan had a hole, found by the owner's question about S9.**
An earlier draft defined balance derivation but no way to create an account or a
method, which would have left `cash` as the only method in the system — a
restaurant with no write path. T0 is the write path that makes the rest of v3
testable, and it is upstream of T4 for that reason — and downstream of T1, which is
why it is second and not first (L46).

## Verification log

Each line is a command that ran and its observed result.

### T1 (2026-10-23)

**What landed.** `0012_accounts_payment_methods.sql`: `accounts` (`id, name,
is_default, created_at`) with the partial unique index `accounts_one_default_idx`;
`payment_methods` (`id, account_id, name, is_active, created_at`); the cash seeds
(`…0003` account / `…0004` method) and the credit seeds (`…0005` / `…0006`, method
inactive); `payment_method_id NOT NULL` on all four documents; `sales.payment_method`
dropped with `guard_sale_economics` re-cut; and four guards. Plus the minimum Rust
change that keeps the branch compiling — the name is now joined from
`payment_methods`, the wire is unchanged, and each write path names the seeded cash
method — so T4/T5 own the client-facing field.

**The design decision this slice had to make on its own, recorded because it was a
real hazard.** The obvious backfill is `ADD COLUMN nullable` → `UPDATE` → `SET NOT
NULL`. Measured while writing the file: it fails on three of the four tables, because
`0007`, `0009` and `0010` guard them with triggers that reject `UPDATE` outright
(`credit_notes are immutable: corrections are compensating rows, not edits`). Two
answers were available — disable those triggers for the length of the migration, or
use `ADD COLUMN … NOT NULL DEFAULT`, which fills existing rows as a catalog change
with no row update and no trigger fire. The second was taken: the guards most worth
trusting are exactly the ones a disable-and-restore would switch off while history is
written. The `DEFAULT` is dropped immediately afterwards, so it cannot become a second
way of writing a document.

**RED observed first.** The eight `v3_` tests were written against the migration
before it existed and failed for the intended reason (the tables and the column were
absent), not for a broken assertion.

- `cargo test --test integration v3_`: **8 passed, 0 failed**.
- Full suite: `make test-backend` → **66 integration + 50 unit, 0 failed**.

**Mutation checks, each measured by editing the migration, running the test, and
restoring the file (verified byte-identical afterwards with `diff -q`):**

| Mutation | Result |
|---|---|
| `guard_payment_method_account` → `IF false` | **FAILED** with "a referenced method must not move accounts" |
| default-account guard (deactivation path) → `IF false` | **FAILED** |
| `accounts_default_never_unset` → `IF false` | **FAILED** |
| `guard_new_default_account_has_active_method` → `IF false` | **FAILED** |
| partial unique index removed | **FAILED** |
| `payment_method_id` dropped from the economics guard | **FAILED** |
| `payment_methods_no_delete` disabled | **FAILED** |

**Two mutations survived the first attempt, and both were the same defect `v2`'s T1
recorded — a test passing for the wrong reason.**

1. `guard_payment_method_account` as a no-op left its test green, because the move
   would *also* have been refused by the default-account invariant (the till would
   lose its last active method). Fixed by giving the default account a second active
   method in the test, so only the guard under test can refuse the move.
2. The new-default guard and the partial unique index masked each other: with the
   guard disabled the index answered, and with the index removed the guard answered.
   Fixed by asserting the **error message** rather than the mere existence of a
   refusal, and by giving the second-default candidate an active method so the
   trigger of case (2) cannot be the one that fires.

Both masks were found by mutation, not by reading the test. That is the argument for
running them.

**The differential check (`S16`).** The base commit was extracted with `git archive`
into `/tmp/diffbase`, `POST /api/sales` was sent to a base backend (`:3901`) and to
the branch backend (`:3902`) against the same freshly migrated database, and the
responses were compared after normalising the per-run `product_id`, the name and the
timestamps. **Identical**, including `"payment_method": "cash"`. Probe rows and the
scratch tree were removed; the test database was recreated.

**Migration applies from scratch.** `./scripts/test-db.sh` recreated `tempox_test`
and the migrator applied 1→12 in order, with `0012` last and no error. `psql -f` was
never used — `plan-v1.5`'s L11 records the cost of that mistake.

**The development database was not touched.** `docker exec … tempox -tAc "SELECT
max(version) FROM _sqlx_migrations"` still reads **11**. Every test and every
migration run in this slice targeted `tempox_test`.

**Hygiene.** `make sqlx-check` passes against a migrated test database; the `.sqlx`
cache was regenerated with `make sqlx-prepare HOST_DATABASE_URL=…tempox_test`; the
crate builds with `SQLX_OFFLINE=true` as CI does; `cargo fmt --check` clean;
`cargo clippy --all-targets -- -D warnings` clean.

### T1 — native review, and the two critical defects it found (2026-10-23)

The slice went through **three** native review transactions. The first two each found a
**real, critical, candidate-caused** defect; the third approved. The rate is worth
recording: 2 of 3 revisions found something my own verification had passed, and in both
cases the reason is the same — the tests asserted the rule I *believed* was enforced,
not the rule actually enforced.

**`R3-001` (first occurrence) — over-restriction, a deadlock.**

`guard_default_is_never_unset` refused every true→false on `accounts.is_default`, and
`accounts_one_default_idx` refused a second true row while the first was still true.
Together they pinned the default to whichever account was seeded. Measured, no ordering
worked:

```
apagar cash primero       → "account …003 cannot stop being the default"
prender credit primero    → duplicate key on accounts_one_default_idx
```

The invariant was mis-stated: "the shop always has exactly one till with an active
method" is a statement about the **table at COMMIT**, and a BEFORE trigger cannot see
that state. The fix is a `CONSTRAINT TRIGGER ... DEFERRABLE INITIALLY DEFERRED`.
`CHECK constraints cannot be marked DEFERRABLE` on every Postgres version — only
UNIQUE/PK/EXCLUDE/FK may defer, and a partial unique index cannot be a deferrable
constraint at all. **The test that had encoded the bug** asserted that unsetting the
default was an error; it now asserts that the transition is legal and that ending with
zero defaults is not.

**`R3-001` (second occurrence) — under-enforcement, a race.**

The deactivation guard used `EXISTS` with no account lock. Two concurrent deactivations
of different active methods each saw the other as still active, both committed, and the
default account was left with **zero** active methods. Reproduced with two real `psql`
sessions before the fix (`count(... is_active)` returned 0); after adding
`SELECT ... FOR UPDATE` on the account row in **both** directions, the second writer
blocks, sees the committed state, and is refused. Mutation-proven: removing `FOR UPDATE`
makes the new test fail with "the second writer completed while the first held the
account".

The working shape of that test is worth keeping: two transactions driven from one task
deadlock each other; the shape is `tokio::spawn` for the second writer, hold the first,
assert the second is **still pending** after a timeout (that *is* the assertion), then
commit and assert the final state.

**The split.** After the second defect the owner chose to split the file, and the split
point is a dependency, not a line count: `guard_payment_method_account` queries the four
documents, so it can only live where `payment_method_id` exists. `0012` = vocabulary and
the guards about accounts/methods; `0013` = the four destinations, the backfill, the drop
and that one guard.

**Final state: approved.** Three lenses (`risk`, `resilience`, `reliability`), three
reviewers, `approved`, no blocking finding, authority burned. Four advisory findings
carried as later work, none of them a reason to re-review this candidate:

| ID | Lens | Severity | Where |
|---|---|---|---|

| `R1-001` | risk | SUGGESTION | `0012` guard comments |
| `R3-001` | reliability | WARNING | `integration.rs` test region |
| `R3-002` | reliability | WARNING | `integration.rs` test region |
| `R3-003` | reliability | SUGGESTION | `integration.rs` test region |

### Review practice for this slice (learned the hard way, three times)

**Corrections are new commits, never `--amend`, once a review has opened authority.**
Each amendment changes the candidate hash the authority is bound to, so the provider
keeps asking for a tree that no longer exists and the transaction dies unrecoverable
from Pi. Measured on all three: `review-7e70748cd932714a`, `review-a9b0b92eff7cdae1`
and `review-2b406848e8df8fdd` are stranded in `correction_required`, bound to
`0d4e6006…`, `af5db1a8…` and `247c13fe…` respectively, none of which can be produced
again. A visible correction commit is also better history than a silent rewrite of the
work being reviewed.

**Review each task in isolation, with an explicit `baseRef`.** The default projection
is the whole branch against `main`, and by T0 that was ~3,600 lines — over the
provider's reviewer context budget, which refuses without creating authority
(`lens_context_budget_exceeded`, `lineage_created: false`, nothing to repair). Isolated
against its parent, T0's candidate was 1,456 lines and started cleanly. Every task from
here is reviewed that way.

**A stranding is reported, not repaired.** `RESET`, `RECOVER` and `ABANDON` are
destructive native operations that need an explicit owner decision; Pi does not reach
for them to tidy up. The affected lineages are listed in `L59` and `L63`.


## Log

L1 (user, 2026-10-22, verbatim): "bueno arranquemos a planificar V3 "Accounts" o
"Wallets". Primero vamos a desarrollar todo el plan para V3 antes de escribir una
linea de codigo. Te paso lo que necesito: En esta parte vamos a desarrollar lo que
le va a dar base, trazabilidad y auditoria a todo el flujo de dinero del negocio.
Solo te voy a pasar que necesito que existan diferentes metodos de pagos, pero que
cada metodo de pago este ligado a una sola cuenta/billetera, a su vez una
cuenta/billetera puede tener varios metodos de pagos. Los metodos de pagos van a
servir para imputar pagos tanto en ventas como en compras y para gastos cuando
sigamos con el desarrollo de V4. Decime como lo implementarias antes de escribir el
plan y codigo."

L2 (decision, 2026-10-22): the ownership direction is taken literally — the method
belongs to the account — and it is the reason a per-account balance is derivable
at all. Presented before the plan, as L1 asked.

L3 (decision, 2026-10-22): the four documents gain a destination in v3, not only
sales. A sale with a method and a purchase without one leaves the account balance
unable to close, and every later event multiplies the write paths that have to be
retrofitted (`PLAN.md` "Current focus — v3 next" measured this cost).

L4 (decision, 2026-10-22): `payment_method_id` is `NOT NULL` and history is
backfilled with the seeded `cash` method. The nullable alternative was rejected
because it creates two classes of document; the backfill is not an invented fact
because `0002`'s default is the proof that the till was the only destination.

L5 (user decision, 2026-10-22): **transfers are out of v3**, and go with expenses
in v4 — "Fuera de v3, junto al gasto en v4". The same shape, built once.

L6 (user decision, 2026-10-22): the `/sales` footer becomes the **balance of the
cash account**, and the general total is read on `/accounts` — "Sí, y el total
general se ve en /accounts". Consequence recorded: a card sale did not enter the
till, an existing test pins the old meaning, and the new meaning gets its own test.

L7 (user request, 2026-10-22, verbatim): "antes de decidir por A, B o C necesito
que consideres las ventas/compras a credito (cuenta corriente). Primero como lo
implementarias, y segundo si siguen en pie tanto A, B o C con un sistema de
credito" — the question that changed the model, and the reason the credit design
was settled before the balance source.

L8 (decision, 2026-10-22): **the debt is an account.** A credit sale writes the
same `payment_method_id` a cash sale writes, pointing at a method whose account is
the debt account; "Crédito" is a payment method, not a flag on `sales`. At the
time, the `credit` account and method were planned for v4, following
`plan-v2.md`'s S2 argument about dead schema vocabulary. L33 corrects that decision:
the account and method are seeded in v3, with the method inactive, while credit
sales remain deferred. The stored
name is `credit` (English, the S4 contract); the display is "Crédito" / "Credit"
in the dictionaries.

L9 (decision, 2026-10-22): with credit considered, option **C (an append-only
`money_movements` table) was rejected**. A collection is one document with two
effects, so a physical table needs two rows plus a coherence trigger per document
kind, forever — and it requires backfilling 215k rows of immutable history, which
`PLAN.md` records this system does not do. **Option A degrades** from four
hand-written branches to seven or eight call sites each re-deriving the sign, and
its failure mode is a plausible wrong number. **Option B (`VIEW`) survives** and is
the owner's decision, with S7 added as its condition.

L10 (user decision, 2026-10-22): the balance source is **B — `VIEW
money_movements`** — recorded after A, B and C were argued with their scalability
in numbers, and after C was re-examined against the credit requirement.

L11 (user decision, 2026-10-22): collection **imputation is (iii)** — the
collection belongs to the customer, and "which sale is unpaid" is derived by
applying collections to the oldest sales first. The FIFO policy is declared and
tested once rather than stored per collection, which keeps the counter free of a
decision and honours `PRODUCT.md`'s "no report may justify a new field at write
time". Not built in v3; recorded because S7's shape depends on it.

**(Superseded by `L42`, kept for the reversal: the derived answer is the defect,
not the decision.)** "Keeps the counter free of a decision" and "the result is a
fact" are both satisfiable at once, because FIFO re-enters as a proposal the
cashier confirms rather than as an unstated rule every reader must re-derive. S21
replaces this entry's mechanism; the owner's judgment about not interrogating the
cashier survives intact.

L12 (user decision, 2026-10-22): a credit note on an already-collected credit sale
**inherits the method and may name another** — so the balance can be settled
explicitly instead of silently. S10's two guards are the cost of that option and
were accepted with it.

L13 (user decision, 2026-10-22): **overpayment is accepted** and leaves a negative
debt, i.e. a credit balance in the customer's favour. Consequence named rather
than discovered later: collections carry no "do not exceed" guard, and liquidating
a credit balance is v4/v5 work with its own document.

L14 (user decision, 2026-10-22): **`status` and `reversed_at` are dropped in v3**,
with the pre-check of S11 — "Dropearlas en v3, con pre-check". Measured before
deciding: no write path in `backend/src` sets either column, and the only real
read of them is `Ledger.svelte:357`, which is dead today and is corrected in T7.

L15 (decision, 2026-10-22, recorded as a correction to L14's own evidence): the
first measurement claimed `status`/`reversed_at` were read only by the sales list
and `/api/cash`. That was **wrong** — `Ledger.svelte:357` reads both. The task
count did not change (the frontend was already in scope) but S12 exists because the
false claim was found before it reached the plan, not because the work was known.

L16 (user decision, 2026-10-22): **nothing about drafts is stored** — "no guardes
nada de draft. lo dejo para un slice mas adelante, por ahora no es un problema".
The cookie path was measured and refused for v3 on a mechanical ground worth
keeping: a sale draft is ~110 bytes per line, a cookie is capped near 4 KB **and
is sent on every request**, so the cookie can only carry an identifier — which
implies a table anyway. `localStorage` is the plausible future step and
`sale_drafts` only if drafts ever need to move between terminals.

L17 (user decision, 2026-10-22): the plan is persisted as this document plus a
gitignored `plan-v3.md`, and the work is ordered **by layers** — the whole backend
first, then the UI.

L18 (state, 2026-10-22, pre-work): `main` = `bae271c`, working tree clean. The
measured facts this document depends on: `sales.payment_method` at
`backend/migrations/0002_v1_sales_stock.sql:22`; `guard_sale_economics` at
`0003_units_and_history_guards.sql:47`; the former `/api/cash` as one `SELECT`
over four tables at `backend/src/routes/sales.rs:709`; `Ledger.svelte:357` reading `status`
and `reversed_at`; `frontend/src/lib/draftLines.ts` holding the cart in the
browser; the fixed-id precedent at `WALK_IN_CUSTOMER_ID` (`sales.rs`) and
`ONE_OFF_SUPPLIER_ID` (`purchases.rs`); and the derived-versus-snapshot name
precedent — `c.name AS customer_name` (`sales.rs:538`, a join) against
`product_name TEXT` (`0002:33`, a column).

L19 (user, 2026-10-22, verbatim): "1 si, agrega /api/accounts. 2 Si, pero
explicame por que poner account_id si este ya esta ligado a payment_method_id. lo
mismo para payment_method_name. No es suficiente con payment_method_id? 3 si
agrega plan-v3.md a gitignore. Antes de escribir explicame esto La deuda es una
cuenta (kind = receivable); \"Cuenta corriente\" es un método │ Ningún documento gana
un flag de crédito. Se siembra en v4, no en v3. Mientras te paso algunos cambios
cosmeticos: no uses \"Cuenta corriente\" para el metodo, usa Credito. cuando decis
\"accounts: id, name, kind (cash | bank | wallet\" para que sirve el \"kind\"?"

L20 (correction, 2026-10-22, found by L19's question): `account_id` was listed in
`GET /api/sales` and `GET /api/purchases` in `plan-v3.md` §7. That was wrong and is
removed: it belongs only in `money_movements`, where the view translates
method → account once, and where a two-effect document needs **two different**
`account_id` values in two rows. A document with one effect does not carry it.

L21 (correction, 2026-10-22, found by L19's question): `payment_method_name` is
**derived by join, never a column**, following `c.name AS customer_name`. The
product name is snapshotted (`product_name`, `0002:33`) because a product can be
renamed and history must not move; a payment method has no edit by S3, so there is
nothing to snapshot.

L22 (decision, 2026-10-22, `kind` withdrawn): the owner asked what `kind` was for
and the answer did not survive. Only one of its three purposes had a consumer, and
locating the till was then understood as solved by the fixed id `…0003` — the
mechanism `WALK_IN_CUSTOMER_ID` and `ONE_OFF_SUPPLIER_ID` established. Its three
values are indistinguishable in v3, which makes the CHECK dead vocabulary in the
exact sense `plan-v2.md`'s S2 rejected. `accounts` is a small configuration table,
so v4 could add a classification with a default and no backfill. The later owner
decision replaces the fixed-id till lookup with `is_default`; L33 also returns S9
to v3 and seeds the `credit` account/method in v3. Credit-sale and collection behavior
remains deferred. The credit method's stored name is `credit`, display "Crédito".

L23 (user, 2026-10-22, verbatim): "explicame bien que hace /api/cash. Explicame
\"- /accounts: lista plana de cuentas con su saldo derivado. Sin \"total
disponible\".\" si mirando cada cuenta me muestra el saldo de cada una, agregando la
cuenta credit ya se que lo que me deben vive en esa cuenta o estoy entendiendo
mal? Con respecto a esto S9 (venta a crédito no puede ser walk-in) y el 400 de nota
sobre cuenta de deuda: se mudan a v4\" por que lo mudas a v4, no podes crear el guar
que el walk-in no acepte credito? \"- S10 en v3 queda sólo con lo que sí importa: la
nota hereda el método de la venta y la devolución el de la compra.\" acordamos que
hereda pero se puede cambiar el payment_method. cambia esto \"payment_method_name
es derivado por join, nunca columna...\" que el nombre si se pueda editar igual que
los productos, no es mas seguro que el join sea por id?"

L24 (correction, 2026-10-22, from L23): **the "no `total disponible`" note was
wrong and is withdrawn.** The owner's reading is right: a flat list that shows
`Caja: 125.000` beside `Crédito: 15.000` already tells the reader that the second
is debt, because the account's name is the semantic. A separate "available total"
would re-introduce the classification `S8a` had just removed, by the window. There
is no total available figure in v3.

L25 (correction, 2026-10-22, from L23): the chat summary of S10 dropped the
**override** — "hereda pero se puede cambiar el payment_method" is the decision, and
S10 already said "may name a different method explicitly". The spec was reworded so
both halves are unmissable, because a rule that disappears in summary disappears in
delegation.

L26 (correction, 2026-10-22, from L23): **`payment_methods.name` is editable**, and
`S6a` records that the name stays a join-only read. The earlier argument treated
"no edit" as the reason a snapshot was unnecessary; the owner is right that
editable names make the **id** the stable identity, and joining by id is the safer
of the two. `S2` was generalised with it: `account_id` is immutable only **while
referenced by a document**, which is the same rule, and a method with no history
must stay correctable.

L27 (discovery, 2026-10-22, from L23's S9 question, and the most consequential of
this round): **the plan had no write path for accounts or methods.** Neither
`plan-v3.md` nor this document defined `POST /api/accounts` or
`POST /api/payment-methods`, so v3 would have shipped `cash` as the only method in
the system and every "a method other than cash" test would have been unwritable.
Found by asking why S9 could not simply be a guard now. Added as **T0**, upstream
of T4. The initial S9 rationale said the guard needed a credit account that was
then deferred to v4; L33 corrects that ordering decision, seeds the account in v3,
and restores S9 as an active v3 write-path guard.

L28 (decision, 2026-10-22): the branch is `feat/v3-accounts`. Delivery is a chain
of PRs rather than one, because the agent contract sizes a review at about 400
authored changed lines and this slice is several times that. The exact split is in
`plan-v3.md` §8 and is decided when T1 is measured, not guessed here.

L29 (user, 2026-10-22, verbatim): "/api/cash es una sola consulta sin parámetros: suma toda la historia y devuelve un número. En v3 no cambia ni la ruta ni la forma — cambia de dónde sale el número: de "la fórmula de la caja escrita a mano" a "el saldo de la cuenta caja". esto es lo que pasas a /account/cash? con respecto a s9, no creas la cuenta credito en v3 aprobechando que ya sembras la cuenta cash? por que separas?"

L30 (user, 2026-10-22, verbatim): "1. ¿Sembramos la cuenta y el método credit en v3?" — answered with two questions: "antes de decidir decime que es (customer_collections)?" and "hay algun riesgo de seguridad que el frontend sepa la cuenta?"

L31 (user decision, 2026-10-22): credit sales and `customer_collections` are a slice of their own, immediately after v3 — "Slice propio inmediatamente después de v3". Nothing about credit shipping happens inside v3.

L32 (user decision, 2026-10-22): `is_default` on `accounts` — "is_default en accounts". The `/sales` footer and the sales picker default both use it; `/api/cash` is retired.

L33 (correction, 2026-10-22, from L29): the earlier decision to defer seeding `credit` to v4 conflated a column with a row. A seeded row is data and costs an INSERT later; a column with unused enum values is a schema contract. S9 had been moved to v4 as a consequence of that conflation rather than on its merits. `credit` is therefore seeded in v3 with `is_active = false`, S9 returns to v3, and the mirror purchase guard is added.

L34 (decision, 2026-10-22, with its rejected alternative): security was considered for letting the client identify the till account and is not the deciding factor — the application has no authentication and `GET /api/accounts` already returns every account and balance. `is_default` exists instead of a hardcoded client UUID because the server owns the business rule "which account is the till".

L35 (user, 2026-10-22, verbatim): "otra duda, como sabe el sistema que metodo de pago salda una venta/compra? Como esta contruido el plan solo lo sabemos cuando un metodo de pago apunta a la cuenta credit?"

L36 (discovery, 2026-10-22, from L35): the plan could only distinguish cash from credit, and it claimed credit "arrives later, possibly several times, possibly split across two methods" while defining no document for any of it. Collections and supplier payments were both missing; supplier payments had been mentioned in passing as if specified. The question exposed a real defect rather than a missing feature.

L37 (user question, 2026-10-22, verbatim): "tengo dos dudas, si el metodo vive en el documento puedo crear un recibo para mostrarle al cliente donde muestre lo que el entrego + documentos pagados + saldos? La misma pregunta para la opcion 1 de pago mixto"

L38 (decision, 2026-10-22, owner): a collection is ONE document with items per method — "Opción 2: un documento con items por método". Recorded with the reversal: the earlier "one method per document" was correct in schema and blind to the receipt, which cannot be stored and cannot be grouped without either a client-side decision or the wall clock. The receipt is the document.

L39 (user decision, 2026-10-22): supplier payments are a later slice — "Sólo cobros, pagos en un slice posterior". The next slice covers credit sales and collections only.

L40 (user, 2026-10-22, verbatim): "no lo veo 100% robusto al sistema de credito,
pagos y el flujo de dinero y credito. Busca arquitectura de Odoo u otros sistemas
POS o ERP de como resuelven este problema de ingieneria y enseñame los que te
parecen mas robustos e inteligentes"

L41 (research, 2026-10-22, from L40): Odoo, ERPNext, SAP and iDempiere separate
obligation / settlement / allocation, and store the allocation
(`account.partial.reconcile` with `debit_move_id`, `credit_move_id`, `amount`;
ERPNext Payment Entry with explicit allocation and `unallocated_amount`; SAP
clearing control with a configurable strategy whose result is stored). The decisive
rule: the matching policy is an algorithm, the matching result is a fact. Reference
detail is in Engram topic `tempox/architecture/money-flow-reference`.

L42 (decision, 2026-10-22, correcting L38's model and L11's FIFO declaration): allocation is **stored**, not
derived by FIFO. FIFO is a UI proposal the user confirms. `unallocated_amount` stays
derived because it is exact arithmetic (`total - SUM(allocations)`), which carries
no policy and therefore no drift. The repository's own
`credit_note_items.sale_item_id` is the precedent: explicit allocation for money
leaving, and the plan had applied the opposite criterion to money arriving.

L43 (user decision, 2026-10-22): a credit balance can be applied to the customer's
next sale (and the supplier's next purchase) — "Podemos aplicar el saldo a favor en
la proxima venta/compra del cliente/proveedor?" answered affirmatively after the
mechanism was explained: a new allocation against a collection that still has
unallocated amount, with no money moving.

L44 (user decision, 2026-10-22): v3 ships with the mixed-payment limitation
declared — "Declarar la limitación y seguir con v3". A cash sale paid half in cash
and half by card cannot be recorded until the settlement layer lands in the next
slice.

L45 (user, 2026-10-22, verbatim): "que tengo que confirmar del corte de tareas. De
que wire?"

L46 (correction, 2026-10-22, from L45): the task table listed T0 before T1, but T0
is the HTTP write path for tables T1 creates — it could not be built or tested
first, and sat there only because it was appended later. Execution order is
T1 → T0 → T3 → T4 → T5 → T6 → T7 → T8, with T2 last. The ids are kept stable rather
than renumbered, so a Log entry citing T0 still means the accounts/methods write
path; the order is stated beside the table so the defect cannot be re-introduced by
re-sorting rows by id.

L47 (user question, 2026-10-22, verbatim): "antes respondeme, si el name cambia
algun dia. por ejemple: en ves de poner mercadopago pongo Mervadopago y quiero
cambiarlo, no afeta al wire?"

L48 (decision, 2026-10-22, owner): the wire is flat — "A: plano, id + name".
`payment_method_id` plus `payment_method_name` resolved by join, no `account_id`,
no nesting. Recorded with the rename argument: documents store the id, so a rename
changes only the rendered name, and renaming **corrects** history instead of
freezing a typo into immutable rows — the opposite of what a snapshotted column
would do, and the reason the `product_name` pattern does not transfer. A stored
name column would make every old document keep the typo forever, and the hybrid
that keeps the legacy `payment_method` string beside the id would create two
sources of the name, the stale one waiting for its first reader. Renaming still
cannot touch `payment_methods.account_id` while documents exist (S2). Spec: S6b.

L49 (user decision, 2026-10-22): T2 goes last — "T2 al final, como cierre aislado".
A destructive migration stays off the critical path of the other tasks and is
reviewed alone.

L50 (user, 2026-10-22, verbatim): "Arranca con T1 y si surge alguna decision que se nos haya escapado del plan, paras y me preguntas"

L51 (blocker raised, 2026-10-22, before writing `0012`): three questions the plan did not answer and one that had to be corrected. (a) `S4` said `accounts` has no `is_active`, while `plan-v3.md` §2.2 and T0 both gave it a `PATCH` for `is_active` — the migration could follow one or the other, not both. (b) `S5` never said what happens to `sales.payment_method`, the TEXT column, and dropping it invalidates `guard_sale_economics` (`0003:47`) in the same file. (c) the fixed UUIDs for the `credit` seeds were unstated. Correction, not a question: the economics guard had to move from `payment_method` to `payment_method_id` or a recorded sale's destination would become rewritable.

L52 (user decision, 2026-10-22): `accounts` has **no** `is_active` — "No, va sin is_active, pero los metodos de pagos si llevan is_active. La cuenta default siempre tiene que tener un metodo de pago activo". The second sentence is a new invariant that no spec carried, and it is the one that shaped the migration's guards.

L53 (user decision, 2026-10-22): `sales.payment_method` is dropped **in `0012`**, with the economics guard re-cut in the same file — "Borrar en 0012 (con recut del guard)". The alternative (leave it for T2) would have kept the TEXT beside the FK for seven tasks and concentrated every destructive change into one commit that is hard to review.

L54 (discovery during T1, 2026-10-23, and it changed the migration's technique): the documented backfill shape — nullable `ADD COLUMN`, `UPDATE`, `SET NOT NULL` — **fails** on `credit_notes`, `purchases` and `supplier_returns`, because `0007`/`0009`/`0010` guard them with triggers that reject `UPDATE` outright. Measured, not predicted: the migrator reported `credit_notes are immutable: corrections are compensating rows, not edits`. `ADD COLUMN … NOT NULL DEFAULT` fills existing rows as a catalog change with no row update and no trigger fire, so the guards stay on; the `DEFAULT` is dropped straight after so it cannot become a second way to write a document. Disabling the triggers for the migration was refused: those are the guards most worth trusting, and switching them off while history is written is the wrong trade.

L55 (discovery during T1, 2026-10-23, the mirror of L10 in v2): **two guard tests passed for the wrong reason**, and mutation found both. Mutating `guard_payment_method_account` into a no-op left its test green because the default-account invariant refused the same statement for a different reason; fixed by giving the default account a second active method so only the guard under test can refuse. Separately, the new-default guard and the partial unique index masked each other in both directions; fixed by asserting the **error message** rather than the bare existence of a refusal, and by choosing candidates that leave exactly one guard able to fire. A guard test that only checks `is_err()` measures that *something* refused, which is not the claim.

L56 (state, 2026-10-23, T1 closed): `0012_accounts_payment_methods.sql` applied, 8 new `v3_` integration tests green, full suite 66 integration + 50 unit with 0 failures, 7/7 mutation checks biting after the two masks above were repaired, `sqlx-check` clean, `fmt`/`clippy` clean, the migration applies from scratch on a recreated `tempox_test`, and the differential check on `POST /api/sales` is byte-identical to `bae271c`. The development database is still at migration 11 — untouched, by design. **Known gap carried into T4/T5**: the write paths name the seeded `cash` method instead of accepting a client-supplied one, so the API cannot yet record a non-cash sale; the `payment_method` JSON field still carries the joined name and its shape is unchanged.

L57 (state, 2026-10-23, T1 reviewed and approved): three native review transactions ran.
The first found `R3-001` — the default-account guard deadlocked against the partial unique
index, making the till permanently immovable, with a test that had encoded the bug; fixed
with a DEFERRABLE constraint trigger (Postgres refuses deferrable CHECKs). The second
found `R3-001` again, a different defect: the deactivation guard had no account lock, so
two concurrent deactivations could leave the default account with no active method —
reproduced with two psql sessions, fixed with `SELECT ... FOR UPDATE` in both trigger
directions, mutation-proven. The owner then chose to split `0012`/`0013`, and the third
transaction approved with three lenses and zero blocking findings. Authority burned.
Four advisory findings carried as later work. Commit `050779b`.

L58 (discovery, 2026-10-23, worth more than either fix): **both critical defects were
invisible to the tests that claimed to cover them**, and for the same reason — the test
asserted the rule I believed was enforced. The deadlock test asserted that unsetting the
default was refused (one half of a two-sided rule). The race test exercised the transitions
in sequence and never concurrently. A guard test that only checks `is_err()` measures that
*something* refused; a concurrency invariant tested sequentially measures nothing. Both are
now written to the shape that would have caught the defect: assert the legal transition too,
and drive the concurrent writers from separate tasks with the interleaving made explicit.

L59 (decision, 2026-10-23, from L57): two review transactions remain open and unreconciled
— `review-7e70748cd932714a` (its target was replaced by the correction it requested) and
`review-a9a0b92eff7cdae1` (its target was replaced by the second correction). Neither is
recoverable by Pi: their authority is bound to candidate hashes that no longer exist, and
`recover`/`reset` remain destructive operations for the audited native CLI. Reported as open
authority rather than silently dropped.

L60 (state, 2026-10-23, T0 done): `GET`/`POST /api/accounts` (methods nested),
`GET`/`POST /api/payment-methods`, and `PATCH` for each, without balances — those
arrive with the view in T3, so this commit is about HTTP shape and its refusals.
Promoting an account to default clears the old one **in the same transaction**, which
turns "move the till" into one call instead of the two-statement deadlock T1's first
review found. 7 new integration tests; the `.sqlx` cache was regenerated because the
suite builds `SQLX_OFFLINE=true` and refused the new queries. Full suite 74 integration
+ 50 unit.

L61 (discovery, 2026-10-23, found by mutation again): **a rule asserted on one endpoint
is not a rule asserted on both.** Removing the trim-then-refuse from
`create_payment_method` left every test green, because the only blank-name assertion
covered `create_account`. This is the third time in this slice that mutation found a
test asserting less than the code claimed (after the two T1 guard masks), and the
pattern is consistent: when a rule appears in two handlers, it has to be asserted
twice, because each handler is a separate place to forget it.

L62 (decision, 2026-10-23, T0's test for the moved till): the test activates the
`credit` method before promoting its account, and that is the invariant working rather
than an obstacle. `credit` is seeded inactive because v3 cannot collect a credit sale,
so promoting it without activating a method would create a till nobody can take money
at — `guard_new_default_account_has_active_method` refuses exactly that, and the first
run of the test failed for the right reason.

L63 (state, 2026-10-23, T0 reviewed, one critical finding, correction applied, lineage
stranded again): native review of T0 in isolation (`review-2b406848e8df8fdd`, 1,456
lines) ran both lenses and a refuter, and found `R3-001`: `POST /api/accounts` accepted
`is_default: true`, which a fresh account can never satisfy because it has no payment
methods — the schema refused it and it surfaced as a **500**. Measured before the fix.
The route now refuses it with a 400 and an actionable reason, and the supported
sequence (create, attach a method, promote via PATCH) is asserted in the same test;
mutation puts the 500 back and the test fails. **The transaction is stranded** because
the fix was committed with `--amend`, moving the candidate hash from `247c13fe…` to
`538a95e9…`; the provider's correction binding still names the old target.

L64 (practice, 2026-10-23, from L59 and L63, recorded so T3-T8 do not repeat it):
corrections land as **new commits**, never `--amend`, once a review has opened
authority; each task is reviewed **in isolation with an explicit `baseRef`**, because
the branch-wide projection is now over the reviewer context budget; and a stranded
lineage is **reported to the owner, not repaired** with `RESET`/`RECOVER`/`ABANDON`,
which are destructive native operations.

L65 (discovery, 2026-10-23, the fourth of its kind in this slice): `R3-001`'s root cause
was again a test asserting less than the handler did. The only test of `POST
/api/accounts` never passed `is_default`, so an option that could only ever 500 had no
assertion at all. The pattern is now unmistakable across T1 and T0: **every critical
defect in this slice was a rule the code offered and no test exercised**, never a rule
the tests exercised wrongly. Mutation finds the ones where the assertion is too weak;
an independent reviewer finds the ones where the surface is wider than the tests.

L66 (state, 2026-10-23, T3 done): `0014_money_movements_view.sql` creates the view (one
row per effect, sign declared once, `status = 'confirmed'` kept on the sale branch until
T2 removes the column) and `GET /api/accounts` carries each account's derived balance
summed in one grouped statement. 5 new integration tests, suite at 80. Mutation checks:
flipping the sale sign, flipping the supplier-return sign, and removing the confirmed
filter each fail the suite after the assertions below were added.

L67 (discovery, 2026-10-23, two of them): mutations found **two** assertions missing.
(a) Removing `WHERE s.status = 'confirmed'` from the view left every v3 test green,
because every sale those tests create is confirmed by default — the predicate that
`/api/cash` carried over was unobservable. Added a test that moves a sale out of
`confirmed` and asserts the balance drops. (b) The `COALESCE` returning `0.00` for an
account with no movements is not a weak test but a type change: without it the query
returns `Option<Decimal>` and does not compile, which is a better guard than a test.

L68 (correction, 2026-10-23, my own mistake, recorded because it cost work): while
mutating the balance query I restored `accounts.rs` with `git checkout --`, which threw
away T3's uncommitted work in that file. It was rebuilt from the transcript. The lesson
is mechanical and belongs in the protocol: **mutate a committed tree, or back the file
up outside git before touching it** — `git checkout --` is a restore-from-commit, not an
undo of the last edit.

L69 (verification, 2026-10-23): the view's agreement with `/api/cash` is asserted
directly — the same scenario computed both ways returns `-40.00` — which is what makes
retiring that endpoint in T6 a safe change rather than a leap.

L70 (state, 2026-10-23, T3 reviewed and approved): lineage `review-c7b8c2e2b27cb95a`
reviewed **in isolation** (`baseRef` = T0's commit, 478 lines, 5 files) and returned
`approved` with two lenses and no blocking finding; authority burned. This is the first
transaction since T1 that closed on its first pass, and the isolation is why: the
branch-wide projector refused outright with `lens_context_budget_exceeded` and created
no authority at all, while the same change reviewed against its own parent fit in 40KB
of reviewer evidence.

L71 (confirmation, 2026-10-23, of L65's pattern): T3's two mutation gaps were found by
me before review, not by it, because I ran the mutations. The reviewer's value in this
slice has been finding what mutation cannot: a surface the code offers and no test
exercises, and — twice — a *concurrency* rule that sequential tests cannot see. The two
tools find different classes of defect and neither replaces the other.

L72 (state, 2026-10-23, T4 done): `POST /api/sales` accepts `payment_method_id`
(omitted = cash, so an older caller keeps its exact behaviour), reads the method's
account server-side rather than trusting a client-supplied one, returns 404 for an
unknown method, and enforces S9 — a sale resolving to the credit account must name a
real customer. The wire is now flat (`payment_method_id` + `payment_method_name`, no
`account_id`). 4 new tests, suite at 84.

L73 (practice, 2026-10-23, the S9 trap closed by mutation): the credit guard was
mutated two ways. Removing it fails the refusal assertion, as expected. But adding
`AND is_active` to the method lookup — a plausible "only usable methods" refinement —
fails the **positive** case, because `credit` is seeded inactive and the acceptance
would then be refused for the wrong reason. That is the trap S9a warned about, and it
is now guarded by a test rather than by a comment.

L74 (consequence, 2026-10-23, noted for T6-T8): `payment_method` as a bare string no
longer exists on the wire. The frontend's `Sale` type still declares it, so the
frontend build does not see the change until T7; the API is ahead of its only consumer
by design, which is why this is a breaking change with exactly one place to fix.

L75 (state, 2026-10-23, T4 reviewed and approved): lineage `review-72df70dc6c5e449d`
reviewed in isolation against T3's commit (763 lines, 9 files) and returned
`approved` with two lenses and no blocking finding; authority burned. Second
consecutive clean pass, and the isolation protocol held again.

L76 (advisory, 2026-10-23, non-blocking, carried as later work): two findings, neither
opening a correction.
- `R1-inactive-method-write-path-unenforced` (risk, WARNING) at
  `backend/src/routes/sales.rs:170-183`: the write path deliberately does not consult
  `is_active`, which is S9a's requirement. The reviewer is right that this means an
  inactive method can be used at write time and there is nothing else stopping it —
  that is the *intended* behaviour, and the finding is the price of it stated plainly.
  Whether a retired method should eventually be refused is a product question for
  after v3, not a defect here.
- `R3-001` (reliability, SUGGESTION) at `backend/tests/integration.rs:5827`: a test
  detail worth a later tidy.

L77 (observation, 2026-10-23, the shape of the slice so far): of four reviewed
candidates, two found real critical defects (T1 twice, T0 once) and two passed clean
(T3, T4). Every critical finding was either a **concurrency rule** or a **surface the
code offered and no test exercised** — never an assertion that existed and was wrong.
That is why the mutation runs and the independent review are both kept for the
remaining tasks: they do not overlap.

L78 (state, 2026-10-23, T5 done): purchases accept `payment_method_id` (omitted = cash,
404 unknown), S9b refuses the credit account on any purchase, a credit note inherits
the sale's method or names its own (an explicit credit one is refused), a supplier
return inherits the purchase's, and the purchases wire is flat. 5 new tests, suite at
89. Four mutations bite: removing S9b, removing the note override, removing the
note's credit refusal, and replacing the return's inheritance with cash.

L79 (design point, 2026-10-23, worth naming because an asymmetry looks like a bug
until it is stated): an **inherited** credit method on a note is legal and an
**explicit** one is not. The inherited case is the refund of an unpaid credit sale,
where crediting the debt account is the whole mechanism — the debt drops and no money
moves. The explicit case would move debt between customers, which is a different
document that does not exist yet. The rule therefore checks the account only on the
explicit branch, and that is deliberate rather than an oversight.

L80 (incident, 2026-10-23, mine, recorded because it cost a debugging round): a T5
test passed a payment **account** id where the note expects a payment **method** id,
and the 404 it produced looked like a code defect. Time went into instrumenting the
handler — with `eprintln!`, because `tracing` is not observable in the in-process test
harness — and into a raw `curl` against a spawned backend, which worked first try and
proved the handler correct. The names are the fix: the test now declares
`cash_account` and `cash_method` separately. **A 404 on a write route is as likely to
be a wrong id in the fixture as a wrong branch in the handler**, and the cheapest
discriminator is the out-of-process request, not instrumentation.

L81 (state, 2026-10-23, T6 done): `/api/cash` is removed and the till is the default
account's derived balance. Twelve test call sites and two frontend call sites were
rewired to read `/api/accounts`; the two frontend unit tests that covered the old
endpoint now assert the new path AND that the pick is by `is_default`, with a second
test proving there is no `0.00` fallback when no account is the default. Suite at 91
integration + 227 frontend unit, eslint/prettier/rustfmt/clippy clean.

L82 (why the retirement was safe rather than a leap, recorded because it is the whole
justification): T3 asserted that the view reproduces the figure `/api/cash` computed by
hand. That assertion did not disappear with the endpoint — it became the agreement
between the view and the same arithmetic read straight from the documents, so the claim
still holds and now has no endpoint to depend on. An endpoint can retire safely when its
number is already reproduced elsewhere and the reproduction is asserted; without that,
removing it would be a guess.

L83 (disposition, 2026-10-23, T5's review): the owner saw T5's reviewer forecast and chose
to continue with T6 instead of spending the two runs. Lineage `review-6d889c5d6ea7253d`
is therefore **open but not stranded** — it was created against the correct base and never
amended, so it can still be run later as-is. Recorded so a future session does not read
an unapproved candidate as approved: T5's code has 5 tests and 4/4 mutations, and no
independent review.

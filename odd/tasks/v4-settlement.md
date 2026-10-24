# v4-settlement — money that moves, money that is owed, and the book that proves it

Design settled in conversation on 2026-10-24, not yet implemented. This file is the
specification subagents read; `L1` onwards in the Log is where it came from, including
the three proposals that were rejected and why.

It exists because v3 closed a hole and left a worse one open: **the system cannot tell
"this sale was paid" apart from "this sale is owed", so it lets a refund of a sale
nobody paid take real money out of the till.** Measured in `v3-accounts.md` `L107`:

```
sale on credit, 100.00, never collected      credit +100.00   cash    0.00
refund naming `cash` (nobody paid anything)  credit +100.00   cash -100.00
                                             → HTTP 201
```

The business pays a customer who never paid it, and still believes it is owed the money.

## The diagnosis that shapes everything below

`credit` is not a wallet. It is an obligation wearing an account's clothes, and it does
three jobs badly at once: it is a destination that holds nothing, it answers "what am I
owed" while pretending to answer "where is the money", and it models the decision *this
sale is owed* as if it were a payment method.

The repo already suspected this. `v3-accounts.md` records a classification column that
was **designed and withdrawn** with the reason *"in v3 its values would be
indistinguishable"*. What was missing was the case that distinguishes them — and it is
not collections, it is **owing**.

Two consequences, and they are axioms for everything below:

- **Money and obligations are different things.** Money moves between real accounts. An
  obligation is a right that moves between nobody until money settles it.
- **A thing that is not how the money moved must not be a payment method.**

## Specs

S1 **The book is a table, not a view.** `money_entries` is append-only, written in the
   same transaction and the same statement as the document that causes it, guarded the
   way `stock_movements` already is (no `UPDATE`, no `DELETE`, no `TRUNCATE`). Its
   columns: `id`, `account_id`, `payment_method_id`, `document_kind`, `document_id`,
   `amount` (positive enters, negative leaves), `occurred_at`, `created_at`,
   `created_by`. `money_movements` becomes a **view over `money_entries`**, so balances
   and reports do not change.

   Why, in the owner's words: *"necesito que haya un libro de movimientos"*. A view is a
   calculation; if a `UNION` branch is ever lost, nothing records what should have been
   there. A row can be checked against its source.

S2 **An entry is not a document; it is the effect of one.** `document_id` is
   `NOT NULL`. There is no such thing as an entry without a causing document, and no
   route may create one. This is what makes "everything is a document" true rather than
   decorative: you write documents, the book is how their effects are read.

S3 **`accounts.kind ∈ { cash, digital }`.** Obligations are **not** accounts, so
   `obligation` is not a member. This is what answers *"tendriamos que poder discriminar
   entre lo que es efectivo y es digital"* without a table per account. The real
   discrimination between card and transfer lives in `payment_methods`; the book records
   **both** `account_id` and `payment_method_id` so "how much came in by card" is a query.

S4 **`payment_method_id` on `sales` and `purchases` becomes nullable, and the null
   means exactly one thing:** this document did not move money. It is not a free pass —
   a constraint trigger enforces the two states together:

   ```
   payment_method_id IS NULL      →  exactly one obligation/payable names this document
   payment_method_id IS NOT NULL  →  zero obligations, and at least one ledger entry
   ```

   No arbitrary nulls: the null is a claim, and a trigger is what holds it.

S5 **An obligation is a document that links the document that originates it.**
   `obligations.id`, `source_document_id` (UNIQUE), `party_kind ∈ {customer, supplier}`,
   `party_id`, `amount > 0`, `created_at`. One sale or purchase produces at most one.
   **It writes no ledger entry.** Neither does the sale that created it. Money appears
   only when it is settled.

S6 **Settlement is a table, not a column.** `settlements(obligation_id, payment_id,
   amount > 0)` with `SUM(amount) ≤ obligation.amount` per obligation and
   `SUM(amount) ≤ payment.total` per payment. Both guards exist because a payment may
   cover several obligations (*"un cobro puede pagar varias obligaciones"*) and an
   obligation may be paid in parts (*"una venta/compra puede quedar parcialmente
   pagada"*). Partial is not a state: it is `SUM(settlements) < amount`.

S7 **A payment and a collection are one document with a direction.**
   `payments(id, party_kind, party_id, payment_method_id NOT NULL, total, direction ∈
   {in, out}, created_at, created_by)`. `in` is a customer paying us, `out` is us paying
   a supplier, or refunding a customer's credit balance. Money in is a positive entry,
   money out negative.

S8 **An unapplied payment is the customer's credit balance.** No new table: it is a
   `payments` row whose `SUM(settlements) < total`. Consuming it is a new `settlements`
   row naming the same payment and a new obligation.

   **Manual, not automatic** (*"si, vamos por manual"*): the counter sees the balance and
   decides whether to apply it. A customer saving up must not lose their balance to a
   small sale.

S9 **The remaining reversal budget lives on the sale, not on the credit-note table.**
   `sale_items.reversed_quantity`, incremented by every document that reverses a line:
   credit notes and annulments alike. The guard is `SUM ≤ quantity`, in one place.

   This is not cosmetic. Today the budget is `SUM(credit_note_items.quantity)` against
   `sale_items.quantity` — a single ledger-side `SUM`. An annulment that also reverses
   stock (S10) would have to be counted there, or the same line could be reversed twice:
   once by an annulment and once by a credit note. Measured as a defect of the first
   design in `L12`.

S10 **An annulment is a document that reverses a sale reported wrongly, and it always
    returns stock** (*"si, siempre"*). `annulments(id, sale_id, reason ∈ {wrong_amount,
    wrong_method, wrong_customer, wrong_document}, created_at, created_by)`. It writes
    one reversing entry per account the sale touched, consumes the budget of S9, and
    never edits or deletes the sale.

S11 **Corrections are new documents, never edits** (*"yo iria por documento"*). The
    append-only objection was answered and accepted: *"si alguien le erra en un numero
    no se puede editar y queda registrado un movimiento erroneo permanente"* is the
    property auditing needs. The remedy is the reverse entry with a mandatory reason,
    never a rewrite.

    The rule that separates the three kinds of correction:
    **is the error in the system or in reality?**
    - in the system → annul and re-enter (S10)
    - in reality → the original document stands; the difference is another document

S12 **Documents that exist because money can arrive without a sale.** Each is a document
    with two entries, or one entry and a date:
    - **opening balance** — *"si, entra la apertura"* — the till's starting figure, so
      the book can reconcile against reality
    - **transfer** between accounts — money out of one, into another
    - **count adjustment** — "I counted 2,850 and the book says 3,000"; an adjustment
      document, never a rewritten balance

S13 **`created_by TEXT NOT NULL`** on the ledger entries and on the new documents, from
    the first slice. Basic and unauthenticated: *"agregamos algo basico que funcione,
    retomamos en v5 con la implementacion total"*. Adding it later means migrating
    tables that already have rows.

S14 **The refund hole closes by asking a question that now has an answer.** A credit note
    with an **explicit** method on a sale that was owed requires
    `SUM(settlements) ≥` the note's amount. Without enough settlement it is refused. The
    inherited-method path stays free: it only lowers the obligation.

    Same rule on the purchase side, where it does not exist at all today:
    `supplier_returns` may name its own method, and a return of an owed purchase settles
    the payable instead of taking money out of an account.

S15 **All eight slices are separate review candidates.** The five marked **item 1** touch
    identity rows of the ledger and cannot ride in the same commit as the others.

## Tasks

| ID | Spec | Slice | Route | Risk | Commit |
| --- | --- | --- | --- | --- | --- |
| T1 | S1, S2, S3 | S1 — the book | `money_entries` as a table, `money_movements` as a view over it, `accounts.kind`, backfill from the four existing document tables, write path in sale / credit-note / purchase / supplier-return | item 1 | `8de5280` |
| T2 | S13 | S2 — who | `created_by` on the entries and on the documents that will be created | — | — |
| T3 | S12 | S3 — money that is not a sale | opening balance, transfer, count adjustment (`0018`) | item 1 | `1a539d9` |
| T4 | S9 | S4 — the reversal budget | move the budget from `SUM(credit_note_items)` to `sale_items.reversed_quantity` (`0019`) | item 1 | `4ffb95e` |
| T5 | S10, S11 | S5 — annulment | the annulment document, stock always returned, budget consumed | — | — |
| T6 | S5, S6, S7, S8, S14 | S6a — what is owed to us | `obligations`, `payments(direction=in)`, `settlements`, sale on account, credit balance, the `L107` guard | item 1 | — |
| T7 | S5, S6, S7, S14 | S6b — what we owe | `payables`, `payments(direction=out)`, the `supplier_returns` override and its guard | item 1 | — |
| T8 | S4 | S7 — retire `credit` | drop the account and the method, and with them the four triggers `0016` added to protect them | item 1 | — |

Order is dependency, not preference: T4 before T5 (the annulment needs the budget), T1
before everything (nothing else can be audited without the book), T8 last (the only truly
destructive slice, and by then `credit` has no path in).

## Log

L1 (owner, verbatim): "con respecto a 1. como se te ocurre cerrar esa brecha de poder
devolver dinero cuando no se cobro? 1 hard, las ventas/compras a crédito sólo se pueden
devolver con crédito. 2 flag en las cuentas a metodos de true o false si paga o no paga
la venta/compra. 3 otra opción"

L2 **Option 1 was rejected on a scenario that does not exist, and the owner was right to
push back.** The rejection said "a customer who paid and returns is left with a credit
balance, the business keeps money that is not its". Measured afterwards: nothing can
collect an owed sale today. `0014`'s four branches are sale, credit-note, purchase and
supplier-return, and the last two are forbidden from touching the debt account (`S9b`).
So *"if it went to credit, it was not collected"* was **true by construction**. The
rejection stands only on the narrower ground that option 1 gives no way to know when it
stops being true.

L3 **Option 2 was rejected, then half-rescued, then retired.** A flag on the account
cannot distinguish two owed sales, one collected and one not — the flag knows nothing
about a sale. It was resurrected as `accounts.kind ∈ {cash, digital, obligation}`, and
retired again once the owner named the deeper problem.

L4 (owner, verbatim): "tenes razón en que el diseño está equivocado. no deberia existir
la cuenta credit. Que otra propuesta recomendas entonces para vender o comprar a crédito
o debiendo la venta o compra?"

L5 (owner, verbatim): "en realidad no es una cuenta ya que no genera movimientos de
dinero".

L6 (owner, verbatim): "las devoluciones y notas de crédito también tienen payment_method?"

L7 Measured in answer: all four document tables carry `payment_method_id NOT NULL`, but
they mean two different things. `sales` and `purchases` record **a movement that
happened**; `credit_notes` and `supplier_returns` record **a reversal that can move money
that never moved**. Having the column is not having the fact.

L8 The asymmetry that decides the design: `supplier_returns` has **no override** and
therefore has **no hole**; `credit_notes` has an override and therefore does. **The hole
was created by a capability, not by a lack.** `S10` of `v3-accounts.md` added the
override with the reason *"so a sale already collected can be refunded out of a real
account"* — a capability whose precondition was never checked.

L9 (owner, verbatim): "necesito que haya un libro de movimientos, con esta implementacion
se va a poder auditar eso?" — answered no: `money_movements` is a `VIEW`, the join to
`payment_methods` is `INNER` (a misaligned row vanishes from balances without error), and
of the 18 public tables none is a ledger. `stock_movements` is the counter-example: a real
append-only book.

L10 (owner, verbatim): "No tomes como maxima el NOT NULL, lo podemos cambiar, como tambien
cambiar el appen-only. Estamos en modo desarrollo donde nada esta fijo, todo en miras de
mejorar el producto. ... no estoy convencido total del append-only, si alguien le erra en
un numero no se puede editar y queda registrado un movimiento erroneo permanente. Lo mismo
estoy pensando para compras o ventas. Que pasa si un empleado registra mal la compra y
quiero corregir?"

L11 Answered by separating the two claims: append-only forbids **rewriting** history, not
**correcting** it. A correction is a reverse entry with a mandatory reason, then the
correct one. The owner accepted this, which is why S11 exists as a spec rather than as
advice.

L11b **A hole found while answering it, larger than the credit one:** a wrongly registered
sale cannot be corrected at all today. `sales` has no `status` and no `reversed_at` (v3
dropped them), and the only routes are create, list, and `POST /sales/{id}/credit-notes`.
The only exit is a **full credit note**, which means "the customer returned the goods" —
so the stock moves as if merchandise came back, and it never did. The same holds for
purchases. This is what S10 and S11 close.

L12 **The first design of the annulment was defective, measured before writing any code.**
The reversal budget is a single ledger-side `SUM(credit_note_items.quantity)` against
`sale_items.quantity`, and a credit note writes a **positive** stock movement. An
annulment that also reverses stock would have to be counted there or the same line could
be reversed twice. The budget therefore moves to the sale (S9). This is the proof that
"everything is a document" is not enough on its own: some state belongs to the resource
the documents act on, not to any one document.

L13 (owner, verbatim): "si, yo iria por documento, con esta idea de 'todo es un
documento' analogia con linux: 'todo es un archivo'"

L14 **The Linux analogy, taken to its second half.** Not every file is a regular file:
there is `/dev`, there are pipes. The book is `/proc` — the kernel's view over what is
happening, not the data itself. That is why S2 says an entry is an **effect of** a
document: an entry with no `document_id` is a bug, not a document.

L15 **`stock_movements` knows how to start; the money book does not.** `reason ∈ {initial,
sale, credit_note, supplier_return}` on the stock side, and no equivalent for money — no
route, no column, no document. The till's opening figure was never recordable, so the
book's starting point is the first sale rather than the first morning. Hence S12, and the
owner's *"si, entra la apertura"*.

L16 (owner, verbatim): "es indistinto los escenarios, la base de datos es una base de
prueba, podes hacer un drop y arrancar de cero." Accepted for the developer's own
database; **not** a licence to skip the backfill. CI starts empty, the delivery stack does
not, and S1's backfill has to work on a database with rows.

L17 (owner, verbatim): "1 Granularidad por venta. 2 cuenta y metodo. 3 si entran. 4
agregamos algo basico que funcione, retomamos en v5 con la implementacion total. Es
posible dividir en slices?" — the four answers are S5/S6, S1, S12, S13, and the eight
slices of S15.

L18 (owner, verbatim): "un cobro puede pagar varias obligaciones. como tambien una
venta/compra puede quedar parcialmente pagada." Both are why `settlements` is a table of
its own (S6) and why partial is a sum rather than a state.

L19 (owner, verbatim): "con esto tambien arreglamos lo mismo para compras, me refiero a
compras a creditos, pagos a proveedores, etc?" — the answer split it in two.
**Purchases on credit** are simply forbidden today (`S9b`: *"there is no purchase-side
counterpart to a credit sale until the payable document exists"*), and the symmetric
design supplies that counterpart: T7. **But a second gap appears the moment they exist:**
`supplier_returns` inherits the purchase's method with no override, and that inheritance is
only true for a purchase that was paid. Returning goods on an unpaid purchase credits the
debt; it does not hand money back. That is `L107` mirrored — v3 added the override to
credit notes and not to supplier returns — and it is why T7 carries the override and its
guard together.

L20 (owner, verbatim): "otra duda, con esta estructura que pasa si un cliente paga mas de
lo que debe? Lo ideal seria que le quede el saldo a favor y se consuma en la siguiente
venta. Podemos resolver eso con esta estructura?" — yes, as S8, with no new table.

L21 **Two consequences of S8 that shape the design.** First, `L107`'s guard changes from a
boolean to two numbers: not "was this sale collected" but "how much was collected, and how
much is unapplied". Second, a credit note **always** takes money out today (`0014`:
`-total` on the note's method account), so a refund of goods the customer wants as credit
is impossible — the same assumption that broke the purchase side. `direction ∈ {in, out}`
on payments (S7) is what makes "apply a credit balance" and "refund a credit balance" the
same document with opposite signs.

L22 (owner, verbatim): "si, vamos por manual. El aviso cuando supera el total no, dejamos
esa decision para mas adelante." The manual choice is S8. The warning is **deferred on
purpose**, and is recorded here so a later session does not read its absence as an
oversight: without it, a mistyped payment (150,000 for 1,500) becomes a credit balance no
document notices.

L23 **Open, and deliberately not decided here.** Two items were raised and left:
- the overpayment warning (L22)
- whether a credit balance can expire or must always be refundable. The design makes
  refunding it possible (S7, `direction = out`); the policy is not set. Without one, a
  business keeps money that is not its — the same defect as rejected option 1, under a
  different name.

## Verification log

V1 **S1/T1, `8de5280`.** `make test-unit` 50 + 232 (was 50 + 227), `make test-backend`
95 (was 93; +2 new tests), `make test-e2e` 23. The append-only guard proven to bite by
neutering `BEFORE UPDATE OR DELETE` into `BEFORE DELETE`: the test fails on the update.

V1b **The backfill verified on a database with data, not on an empty one.** A throwaway
database was migrated to `0016`, a sale, a credit note and a purchase were created through
the real API, and then `0017` was applied: three entries with the documented signs
(`+200`, `-100`, `-100`), `created_by = 'backfill'`, and
`SUM(money_entries) = SUM(money_movements)`. Doing it on an empty database would have
proven nothing — that is what V1 alone would have hidden.

V2 **Three defects this slice found, all measured first:**
1. `ALTER TABLE accounts` after an `UPDATE` on it aborted with *"cannot ALTER TABLE
   because it has pending trigger events"*. `0016`'s `accounts_exactly_one_default` is
   `DEFERRABLE INITIALLY DEFERRED`, so any row-touching statement queues an event checked
   at COMMIT, and a later `ALTER` in the same transaction cannot proceed. The constraint
   is now created before the only statement that touches rows. This is a *new* constraint
   on how `0016` shapes every future migration: **do your `ALTER`s before your `UPDATE`s
   on `accounts`**.
2. `CHECK (amount <> 0)` turned a bonus sale into a 500. `0005` allows `total = 0`, and a
   sale of only bonuses moved no money; the book records the zero rather than refusing
   the document. **An existing test caught it, not the new ones** — the value of the suite
   is exactly this.
3. The new test's own arithmetic was wrong: it assumed the refund covered the whole sale
   when it covers one unit. Fixed against document totals.

V3 **`SQLX_OFFLINE=true` is a trap in this loop.** `.cargo/config.toml` sets it, so
`cargo build` cannot see a new table until `cargo sqlx prepare` is re-run — and
`prepare` fails on a test database `make test-db` just recreated, because it has no
schema. The working order is: `make test-db`, then `cargo sqlx migrate run`, then
`cargo sqlx prepare -- --all-targets`, then the suite. Two full runs were lost to this.
Also: `target/debug/incremental` produces a rustc ICE on this toolchain after a
significant edit; `rm -rf target/debug/incremental` clears it.

V4 **S3/T3, `1a539d9`.** `make test-unit` 50 + 232, `make test-backend` 98 (was 95; +3),
`make test-e2e` 23, and the four CI commands clean in their order. Two guards proven to
bite: turning the `UNIQUE` index into a plain one lets a second opening balance through,
and neutralising `transfers_between_two_accounts` makes a same-account transfer answer
`201`. A differential run of `5ce9920` against the commit, both binaries on fresh
databases: the three routes go `404` → `201`/`400`/`201`, and `/api/accounts` and
`/api/sales` answer identically both sides.

V4b **A design question this slice answered by force of the schema, not by preference.**
A document that names an account directly (an opening balance, a transfer) still had to
carry `payment_method_id`, because `0017` derives `account_id` from it by trigger and that
derivation is what made the old `INNER JOIN` impossible to drop a row through. The
consequence is real and it is recorded rather than worked around: **an account with no
payment method cannot be opened**, because there is nowhere to derive its account from.
The route answers `404` — what is missing is the method, not the account — which is the
same shape as `0012`'s "the default account always has an active method".

V4c **The development database's 3,000 still has no opening document.** S3 makes one
possible, but the figure that is already there arrived from sales, and inventing an
opening for it would be fiction. Anyone who knows what the till held the first morning can
record it; nobody should guess.

V5 **S3 reviewed and sealed.** Lineage `review-caf2649251ed698b`, authority burned,
`gentle-ai.review-acknowledged/v1`. Approved with **no correction**; three advisory
findings, all `informational` and all in `ledger.rs`:

| id | location | what it is |
| --- | --- | --- |
| `R3-001` | `ledger.rs:75-121` | the opening-balance flow |
| `R3-002` | `ledger.rs:176-203` | the transfer flow |
| `R3-003` | `ledger.rs:283-295` | the count-adjustment flow |

They are non-blocking by the provider's own statement and are recorded here so a later
session does not read them as unexamined. The common shape across all three is worth
naming: **each route resolves the account through the account's first method and never
returns the method it used**, so a client cannot tell which rail the money took. That is
a real limitation of the three new endpoints, not a defect in the ledger, and it belongs
to the slice that gives the UI these documents.

V5b **A transport failure that was recovered, recorded because the rule matters.** The
first reviewer run aborted (`pi-host-relay-transport-failure`, `reviewer-aborted`, after
5.5s). The instruction was to call fresh STATUS and submit only an **exact reoffered**
one-slot binding, never to replay the capture from transcript inference. STATUS reoffered
the identical binding — same `subject-hash`, same revision — and the retry succeeded. The
distinction is the whole point: a *reoffered* binding is authority to try again; a
remembered one is not.

V6 **S4/T4, `4ffb95e`.** `make test-unit` 50 + 232, `make test-backend` 100 (was 98),
`make test-e2e` 23, four CI commands clean.

V6b **Two things `0019` broke on its first run, both found by running it rather than by
reading it.**
1. `sale_items_immutable` (`0003`) refuses **every** `UPDATE` with an unconditional
   `RAISE`, so the backfill aborted with *"sale_items are immutable"*. The guard is now
   narrowed the way `guard_sale_economics` already is: every column except
   `reversed_quantity` stays refused. **The rule for future slices: this table permits
   exactly one mutable column, and a second one is a design change, not a patch.**
2. The migration itself could not be tested on a fresh database — an empty
   `credit_note_items` hides every problem the backfill has. The real evidence came from a
   database with a sale, a credit note and a consumed budget.

V6c **A false green in my own test, and it is the second time this repo has paid for
this.** The bypass test asserted only `is_err()`. Neutralising the consumer guard left it
green, because the consumer's `UPDATE` still ran and the *budget* guard answered instead.
The test now pins the message and fails with
*"reversed 2 of 1 sold"* when the consumer is neutered — which proves it was measuring the
other guard. `v3_the_default_account_always_keeps_an_active_method` records the same
lesson in this repository; it has now been relearned here.

V6d **The differential measurement says something different from S1 and S3.** Running
`f2fb87f` and this commit side by side over HTTP gives the **same** answers — `201`, `409`
— because the handler already refused to over-reverse. The change is not visible through
the API at all; it is visible through `psql`, where 5 units against a line of 1 used to
succeed and now raises. **A differential over HTTP cannot prove this slice**, and saying so
is more useful than the table.

V7 **S4 reviewed, corrected and sealed.** Lineage `review-c20de858957fc384`, authority
burned, `gentle-ai.review-acknowledged/v1`. The review raised **two CRITICAL findings, both
introduced by `4ffb95e` and both real**; both were reproduced against a live database
before being fixed, and the fix is `f1ac49a`.

| id | what the guard missed | measured before the fix |
| --- | --- | --- |
| `R3-001` | the equality list omitted `id` | `UPDATE sale_items SET id = gen_random_uuid()` → `UPDATE 1` |
| `R3-002` | a budget increase was not tied to the refund path | `SET reversed_quantity = quantity` → 0 → 2, no note on record |

`R3-002`'s consequence is the worse of the two: the handler *reads* that counter, so a
wrong state would have produced a wrong `CONFLICT` on the next legitimate refund.

**The correction is a writer flag, and it is not claimed to be authentication.** A trigger
cannot ask who is calling, so `consume_reversal_budget` announces itself in the
transaction-local `tpox.budget_writer` and the immutability guard accepts a budget increase
only when that is set. Any caller who knows the name can set it; what the flag does — and
all the two findings require — is separate "the refund path moved it" from "something else
did".

V7b **The CLI is the route that works, and this is the second time it mattered.** The Pi
relay rejected the `correction-lines` slot in every form for `review-8f102ad38b64c089` in
v3, which is why that lineage is stranded. Here the same slot was submitted through
`gentle-ai review capture-correction-plan --correction-lines=18` and accepted on the first
attempt. Then the review asked for a **targeted validation**, which is also host-mediated:
`--execute` is refused for `pi` (*"it is host-mediated"*), `--materialize` prints the exact
role task, and the verdict goes back through `--input`. **When the facade blocks a slot,
the native CLI is not a workaround; it is the sanctioned route.**

V7c **What the validator concluded.** `original_criteria.passed = true` and
`correction_regression.passed = true`, with two follow-ups recorded rather than silently
carried: the backfill runs before the guards and does not set the writer flag (so a *future*
re-backfill would be refused — the safe order is the one this migration already uses), and
the flag is a session setting, not authorization.

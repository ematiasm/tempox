# v4-expenses — money leaving that touches no stock

The last v4 slice that was never specified. **State: designed, not implemented.** Every
spec below is a decision; none is a measurement of code that exists. The schema facts
quoted here were read on `87dfa5a` and are named with their line so they can be
re-checked instead of believed. Engram topic key: `tempox/odd/v4-expenses/tasks`.

The state that must hold when this feature closes, and it is one sentence:

```
a utility bill, a repair or a salary is recorded once, with a category and who was paid,
whether the money left today or leaves in three advances,
and the shop's spending for a day includes it — plus the purchases it already recorded.
```

v4 bought the machine this slice stands on and left it pointed at one case: `0018`'s three
documents (opening balance, transfer, count adjustment) move money and touch no stock, and
`0023`'s payables gave a debt its own shape. An expense is the first **recurring, everyday**
document in that family: it has a category, a payee, and a balance that survives the month
it belongs to.

## Why this is the last hole in v4, measured

`PLAN.md`'s roadmap has said since 2026-10-20 that the till is overstated by every salary
until expenses exist, and the reason it was placed after v3 is still the reason it is shaped
this way: an expense must name **where** the money left, and before v3 there was no place to
name. That is now true — `money_entries.payment_method_id` is `NOT NULL` and the account is
derived from it by `money_entry_resolves_account` (`0001_schema.sql:2261`).

What is measured absent today:

| what | measured |
|---|---|
| a table, route or screen for expenses | `grep -ri expense backend/src backend/migrations frontend/src` → **0** |
| a place a payee's name could live | `suppliers` exists but is a purchase counterparty, and its seed is `one-off-supplier` |
| a way to owe a non-supplier party | `payables` is closed to `source_document_kind = 'purchase'` (`0001_schema.sql:4146`) |
| a way for money to leave against a non-supplier debt | `payments.party_kind ∈ ('customer','supplier')` (`0001_schema.sql:3773`) |
| a category vocabulary | nothing; the only closed sets in the schema are the ones this repo paid three times to extend |

## The decisions this design rests on

**D1 — An expense is not a purchase, and purchases are not written as expenses.** The
owner's words: *"quiero que registre las compras como una categoria de gastos, asi que esos
gastos se pueden **derivar** de compras que ya tienen proveedor obligatorio"*. Derived is the
verb: a purchase keeps meaning "goods arrived and money left, with a supplier", and the
spending report adds it as one of its terms. Copying it into `expenses` would be two records
of one fact — the defect `0017` spent a migration killing for balances.

**D2 — A category is a row, never a `CHECK`.** This repository has paid for closed
vocabularies three times: `stock_movements.reason` (`0011` replaced six enumerating
constraints with one shape-derived trigger), `money_entries.document_kind` (`0020` copied its
members by hand and **silently dropped `annulment`**, turning every annulment into a 500), and
`obligations.source_document_kind` (`0023` paid an entire duplicated table to avoid re-cutting
it). A category is shop data — adding one must be an `INSERT`, not a migration.

**D3 — An expense is a party to its own money, so it needs no new settlement table.** An
owed expense is paid in advances: *"anoto el salario de un empleado del mes y este durante el
mes puede ir retirando adelantos"*. That is several payments against one debt, which
`payments` already expresses: `party_kind = 'expense'`, `party_id = expenses.id`. The
outstanding figure is `expenses.amount − SUM(payments out on that party)`, and it needs **no
third ceiling function** — the mirror of `payables` would have cost one, and `0023`'s own
comment records what re-cutting a money-guarding function has already cost (`0022` lost a
`FOR UPDATE` doing it once).

**D4 — The cost of D3, stated rather than discovered later.** One payment answers **one**
expense. Paying the rent and the electricity with a single bank withdrawal is two payments,
not one. Purchases keep the ability to spread one payment over several debts because
`payable_settlements` exists; expenses do not, and the asymmetry is real. It is bought
deliberately: the owner's case is many payments to one debt, not one payment to many.

**D5 — Salaries are in scope as a category and out of scope as a staff flow.** The owner's
words: *"registrar los salarios de empleados... todavia no tenemos user/staff creado, asi que
podemos dejar para mas adelante"*. A `Sueldos` category is seeded and usable today; naming
**which** employee a salary belongs to needs a staff table that does not exist
(`grep -ri "CREATE TABLE.*users\|TABLE.*staff" backend/migrations` → 0), and `expenses.payee`
carries the name as text until it does. Recorded so a later session does not read the absence
as an oversight.

**D5a — When staff lands, it is a column on `expenses`, not a document of its own.** The
owner's words: *"mi idea es que sea con empleado pero un documento con FK en user"*. That wish
is satisfied by `expenses.staff_id UUID REFERENCES staff (id)` — the document is `expenses` and
the FK names the employee. A separate `salaries` table would cost a **second** debt mechanism:
`party_kind = 'salary'` (or a settlements table of its own), a second advance route, a second
ceiling function, and a second term in the report. `0023` measured exactly that price on the
supplier side and recorded `obligations.party_kind = 'supplier'` left declared and unused
(`v4-settlement.md` L25). A salary and a utility bill are the same fact — money left, for a
concept, with no goods arriving — where a purchase and a sale are not.

**D5b — The column is NOT reserved now, and the reason is recorded policy.** Two measured
facts: the FK target does not exist (`grep -ci "create table.*\(users\|staff\|employee\)"` →
**0**), and `docs/en/architecture.md:104` states the principle against reserving — *"adding it
later is a nullable `ALTER TABLE` with no backfill — which is why no reserved column anticipates
it"*. Adding `staff_id` when staff exists costs one line and needs no backfill: earlier expenses
stay `NULL`, which is the truth rather than a placeholder.

**D5c — What D5a promises, and it is only promisable because it was measured.** Adding
`staff_id` later touches `payments` not at all, nor the advance route, nor the book, nor the
summary — advances keep settling the expense through `party_kind = 'expense'`. The employee is an
attribute of a document that already exists.

**D5d — Open for the staff slice, stated so it is not decided by accident.** "One salary per
employee per month" is a uniqueness rule that does not exist today, because
`expenses.occurred_at` is an instant and not a period. When `staff_id` arrives, that slice has to
choose between an explicit month column and `UNIQUE (staff_id, date_trunc('month', occurred_at))`.

**D6 — Two alternatives were evaluated and rejected, and they are recorded here rather than
left in a conversation.** The owner asked whether a table for expenses is right, or whether a
purchase should instead carry a field naming which expense it belongs to, with the rest written
as account movements carrying that same identifier. Both fail on measured facts.

**A1 — a field on `purchases` naming its expense: refused by the immutability guard.**
`0001_schema.sql:774-794` installs `reject_purchase_mutation()` as
`BEFORE UPDATE OR DELETE ON purchases`, **with no column list** — unlike
`payment_methods_account_guard`, which is `BEFORE UPDATE OF account_id`. A purchase has no mutable
column at all. The field leaves two roads, both worse: relaxing the guard opens `UPDATE` on a table
`stock_movements` and `payables` both hang off; leaving it strict forces the classification to be
decided **at the moment of buying**, which `PRODUCT.md` forbids (*"no report may justify asking
for an extra field at write time"*). Re-classification would then be impossible, and the salary
case has no document to hang the field on at all.

**A2 — expenses written as `money_entries` rows carrying their own identifier: refused by four
measurements.**

1. **A movement cannot hold a document's data.** `money_entries` has nine columns — `id`,
   `account_id`, `payment_method_id`, `document_kind`, `document_id`, `amount`, `occurred_at`,
   `created_at`, `created_by` — and none is a payee, a category, or a **declared** amount.
   `document_id` has no foreign key and no validation trigger; coherence comes from per-document
   constraint triggers (`payments_claim_their_entry`, `sales_payment_method_claimed`). An entry of
   kind `'expense'` pointing at nothing is what `0017` called *"an entry with no cause is a bug,
   and a bug should not be representable"*.
2. **An owed expense is not a movement, and this is the requirement that kills the shape.**
   `money_entry_names_a_real_account` refuses any row whose account is not `cash`/`digital`
   (`accounts_kind_known CHECK (kind IN ('cash','digital'))`), and `0021` established that a sale
   on account writes **zero** entries. October's salary recorded on the 1st and paid in three
   advances has no movement on the 1st — it is a debt, and debts are documents since v4.
3. **Advances would be indistinguishable from separate expenses.** There is no unique index on
   `money_entries (document_kind, document_id)` — a transfer writes two rows on purpose. "One
   salary of 300 paid 100+100+100" and "three expenses of 100" therefore produce byte-identical
   rows, and the declared amount becomes an aggregate that can never be compared with anything.
4. **The closed vocabulary would be re-cut a fourth time.** `money_entries_kind_known` was re-cut
   three times (`0018`, `0020`, `0022`) and `0020` lost `annulment` doing it, turning every
   annulment into a 500. Ten `INSERT INTO money_entries` sites across five files would start
   writing a new kind with its own pair of guards.

**D6a — The one point in A2's favour, and how it is handled.** A2 would have allowed classifying
goods purchases by rubro (cleaning supplies vs. resale stock), which S9's single derived group
cannot. The owner deferred it — *"pir ahora no, dejamos una versión chica y lo vemos mas
adelante"* — and the shape it must take when it returns is stated now so it is not rediscovered
as A1: a separate `expense_classifications (document_kind, document_id, category_id)` table with
its own guard, **never** a column on `purchases`.

## Specs

S1 **`expense_categories` is a table, seeded, and categories are retired rather than
deleted.** `expense_categories(id, name, is_active, created_at)` with `name UNIQUE`. The seed
carries the shop's own words: `Alquiler`, `Sueldos`, `Servicios`, `Mantenimiento`,
`Impuestos`, `Otros`. The names are **not translated**: a category is shop data, exactly as a
supplier's name is, and both languages render the same row. `is_active` exists for the reason
`payment_methods.is_active` does — an expense that named a category is history, so the row
cannot be deleted, and a picker must be able to stop offering it.

S2 **An expense names a payee in text and a category by reference.**
`expenses(id, category_id, payee, amount, occurred_at, payment_method_id?, created_at,
created_by)`:

- `category_id UUID NOT NULL REFERENCES expense_categories (id)` — the owner's *"Dato
  importante, quiero poder categorizar gastos"*, and the reason S1 is a table.
- `payee TEXT NOT NULL CHECK (btrim(payee) <> '')` — "Edenor", "Ferretería X", a person's
  name. No FK: the payees this document needs are mostly **not** suppliers, and forcing them
  into `suppliers` would fill a purchase table with rows that never sell anything and can
  never be deleted.
- `amount NUMERIC(14, 6) NOT NULL CHECK (amount > 0)` with `amount = round(amount, 2)` and
  `created_by <> ''` — the same three constraints `opening_balances`, `transfers` and
  `count_adjustments` carry.
- `occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()` — **the day the expense belongs to**, which
  is not necessarily the day it is paid. This is what makes D3 worth having: October's salary is
  recorded in October and settles in November, and a later report can say both. Absent in the body
  means now, exactly as `opening_balances.opened_at` does it. **It is an instant and not a
  period** — see D5d for the consequence the staff slice inherits.
- `payment_method_id UUID REFERENCES payment_methods (id)` — nullable, and the null is a
  claim held by a trigger (S4), exactly as `sales` and `purchases` do it since `0021`.

S3 **`POST /api/expenses` reuses the `Destination` grammar, and its three inputs keep their
meaning.** The body is `{category_id, payee, amount, occurred_at?, payment_method_id?}` and
`payment_method_id` follows `models/destination.rs` exactly:

| input | means | what the transaction writes |
|---|---|---|
| omitted | the seeded `cash` method | the expense, **one** `payments` row `out` for the full amount, and that payment's ledger entry |
| a uuid | money left through that method | the same two rows, naming that method |
| `"on_account"` | nothing left; the shop owes | the expense alone: no payment, no ledger entry |
| explicit `null`, a number, a list | not a destination | `400` |

**The route reads the field raw and judges it in the handler**, never through a custom serde
deserializer: `v4-payables.md` L4 measured that a deserializer's failure reaches the client
through axum's `Json` as **422** while the field's contract promises 400, and both existing
routes were repaired for it.

S4 **The two states of an expense, checked at commit.** A constraint trigger
`DEFERRABLE INITIALLY DEFERRED` on `expenses`, the mechanism `0021` established for `sales`:

```
payment_method_id IS NOT NULL  →  at least one `payments` row (out, party_kind='expense',
                                  party_id=this expense), summed within `amount`
payment_method_id IS NULL      →  no `payments` row for this expense yet
```

**The claim is judged at the moment the document is created, and that is why it is
deferred.** A later advance (S7) adds payments to an expense whose claim was the null one;
that is a second document making its own statement, not a contradiction of the first. The
deferral also carries `0021`'s recorded trap: the payment is written after the expense row
exists, and **no `ALTER TABLE` on this table may follow a row-touching statement in the same
transaction**.

S5 **The book learns no new kind, and that is the strongest available evidence that this
design is right.** Money moves through the `payments` row S3 writes, which already owns
exactly one `money_entries` row of kind `'payment'` — `payments_claim_their_entry`
(`0001_schema.sql`) refuses a payment that wrote none or more than one, and `0022` re-cut that
closed set to admit `'payment'` once already.

**An `'expense'` ledger entry would therefore count the same coin twice**, and the first
draft of this document had exactly that defect. It is recorded rather than quietly fixed
because it is the class this repository keeps paying for: two records of one movement. So the
expense document writes **no entry at all** — and because it writes none, this migration
touches `money_entries.document_kind` neither by `ALTER` nor by a hand-copied member list,
which is the operation that lost `annulment` in `0020`.

S6 **`payments` learns a third party, and the direction rule is extended rather than
loosened.** `payments.party_kind` gains `'expense'` and `payments_direction_matches_party`
gains the pair `(direction = 'out' AND party_kind = 'expense')`. **The money direction is
`out` and only `out`**: money never arrives at an expense, and a rule that allowed `in` would
make the direction a suggestion — the argument `0022` makes for `in`-from-a-supplier. Both
`CHECK`s are re-cut by the whole member list and asserted by a `DO` block that names every
member the chain already accepted, the discipline `0020`'s lost `annulment` bought.

S7 **A payment against an owed expense is an advance, and its ceiling is a sum.**
`POST /api/expenses/{id}/advances` with `{payment_method_id, amount}` — one transaction: one
`payments` row `out` naming `party_kind='expense'`, `party_id=<expense>`, and its ledger entry.
Refusals, and each is its own status because each is a different correction:

| case | status | why |
|---|---|---|
| `SUM(advances) + amount > expenses.amount` | `409` | the debt is not that large; asking first is what makes it a `409` and not a trigger's `500` |
| an amount that is not positive whole cents | `400` | the database's `CHECK`, asked before it |
| an unknown expense or an unknown method | `404` | the same shape check every other route makes |
| the expense was fully paid when it was created (S3 named a method) | `409` | it owes nothing, so there is nothing to advance against |

The expense row is locked `FOR UPDATE` before the ceiling is judged, so two concurrent
advances cannot both read the same committed sum and both find room. **The lock order is one
row, so there is no order to get wrong** — the defect class `aa95665` cost is not reachable
here, and that is worth stating rather than assuming.

**The ceiling is enforced below the route as well as in it**, by a trigger on `payments`
`BEFORE INSERT` when `party_kind = 'expense'`. The route's `409` exists because a `RAISE` at
commit reaches the caller as a `500`; the trigger exists because a ceiling enforced only above
the database is a suggestion to everyone who reaches below it — `v4-payables.md`
`R3-005` records that lesson on the supplier side.

S8 **The expense ledger is day-scoped with the contract every other ledger has.**
`GET /api/expenses?from=&to=&limit=50&offset=0`: newest first by `occurred_at`, the same
half-open `[from, to)` bounds, the same `limit` clamp (1..200) and `offset` clamp, each row
carrying category name, payee, method name (or null, meaning on account), `amount`, `paid`
(derived) and `outstanding` (derived). **`paid` and `outstanding` are never stored**: a stored
figure that is a subtraction of other rows can disagree with them, which is why `sales` lost
its lifecycle in `0015`.

S8a **The ledger answers a page, and the page says what it left out.** (`L17`.) The response is
`{rows, unsettled_older, cap}`, not a bare array. **`unsettled=true` narrows the page to the
documents that still owe something** — `amount − SUM(payments) > 0`, composed with every other
parameter rather than replacing them — and it is the only filter here that asks a question
instead of slicing an order. `unsettled_older` counts what falls outside the page, **capped
like the list itself** so the count stays affordable, and it is `0` when the flag was not sent;
`cap` names the largest page the endpoint will answer, so no client hard-codes the server's
limit.

**The filter exists because the alternative was measured to fail twice.** `R3-003` of
`review-0e3269a3a9623ddc` found the screen deciding which documents owed something in the
browser, over the newest 200 in the shop's whole history: past that, an owed salary rendered no
row and no advance form — the same defect the previous fix had closed, pushed one page size
further out, with the cap unsurfaced. **A larger page is not the fix**; it moves the horizon
and leaves the class behind it. The browser's only remaining arithmetic is combining two
answers that are already true: the day's own list and the older debts the server found (see
`frontend/src/lib/expenseLedger.ts`).

S9 **The summary answers "what did the shop spend", and `net` has two terms.**
`GET /api/expenses/summary?from=&to=` answers
`{expenses, purchases, supplier_returns, net, documents}` where
`net = expenses + purchases − supplier_returns`, scoped by each document's own `occurred_at`
for an expense and by `created_at` for a purchase. Two halves of D1 in one payload: the
recorded expenses and the purchases already in the system, **derived, not copied**, so the
answer to "what did I spend this month" does not depend on anybody remembering to record a
purchase twice.

**Payments out on expense parties appear in no term of `net`, and the asymmetry with
settlements is deliberate.** A settlement is excluded because the debt it answers is already
counted; here the payment is excluded for a stronger reason — the *money* it moved is the same
money `expenses` already carries at full amount, whether it was paid today or in three
advances. The payload therefore reports `expenses` (what the period cost) and, per row, the
`paid` and `outstanding` that S8 derives; it does **not** report a second period figure named
`advances`, because a number that looks like spending and is a settlement of spending is the
kind of figure a reader adds twice.

**Purchases enter under one derived group and gain no column.** A purchase records no category
and must not start: `PRODUCT.md`'s principle is explicit — *"No report may justify asking for
an extra field at write time"* — and every purchase in this system is a goods purchase, so one
group is the truth rather than a simplification.

S10 **`GET /api/expense-categories` and `POST /api/expense-categories`.** The getter returns
every category, **including inactive ones**, for the reason `GET /api/payment-methods` does:
a ledger rendering a historical expense needs the name of a category retired afterwards. The
poster takes `{name}` — `400` blank (trimmed first), `409` duplicate. Retiring is
`PATCH {is_active}`, and a category may not be deleted.

S11 **Every rule is proven by a test that bites, through the public interface.** RED first,
one test per requested rule, asserting every observable effect: the status code, the exact
response shape, **and that a refused request left no row behind** — no expense, no payment, no
ledger entry, no advance — because the book is append-only and a refused write that left a row
cannot be taken back. Each guard is then proven by neutering it and watching its test fail. A
documents count or a `len()` is **not** an assertion: `v5-customer-account.md` V2b records
that class of decoration passing for a different row three times.

S12 **Scope.** In: categories, the expense document, immediate payment, advances, the
ledger, the summary, the category routes. Out, with reasons recorded so they are not
rediscovered as surprises:

- **Salaries as a staff flow** (D5-D5d) — needs a `staff` table, deferred to the staff slice.
  The shape is already decided (D5a) and needs no work here.
- **One payment across several expenses** (D4) — the price of D3, stated.
- **Classifying purchases by category** — the owner's answer: *"pir ahora no, dejamos una
  versión chica y lo vemos mas adelante"*. Purchases enter the summary as **one derived
group** (S9), which is the small version. When the shop wants goods split by rubro, that is
  a separate slice and **not** a column on `purchases` (D6).
- **A recurring template** ("every month, rent 300.000") — a schedule is a different document,
  and a template that silently writes expenses is a machine that records facts nobody entered.
- **Reversing an expense.** An expense is immutable and append-only. Its correction document
  is a future slice, and it is the mirror of `annulment`: a `reason`, not an edit.
- **Attaching a receipt image.** No file storage exists in this system.
- **Changing `created_by`** — still `'counter'`, still deferred with `v4-settlement.md` L24.

## Tasks

| ID | Spec | Task | Route | Risk | Commit |
| --- | --- | --- | --- | --- | --- |
| E1-T1 | S1, S10 | `expense_categories` + seed + `GET`/`POST`/`PATCH`, with the closed-set assertion discipline | inline | — | — |
| E1-T2 | S2, S3, S4, S5, S6 | the expense document in one migration — the table, `payments.party_kind` extended to `'expense'` with its direction pair and its `DO` assertion, the two-state trigger, and `POST /api/expenses` over the shared `Destination` grammar | inline | item 1 | — |
| E1-T3 | S7 | advances: `POST /api/expenses/{id}/advances`, the ceiling, the lock, the four refusals | inline | item 1 | — |
| E1-T4 | S8, S9 | the ledger and the summary, with `paid`/`outstanding` derived and purchases as a term | inline | — | — |
| E1-T5 | S11 | the whole rule set proven to bite, plus `architecture.md`, `README.md`, `PRODUCT.md` and `docs/en/testing.md`'s counts | inline | — | — |
| E2-T1 | S1, S8 | `/expenses`: the draft sheet (payee, category picker, amount, method picker with "on account"), the day's ledger, the rail destination, the version chip, both dictionaries | inline | item 3 | — |
| E2-T2 | S7, S9 | the advance action inside an expanded expense, the spending foot with its two terms and the purchases term, and one e2e that records a paid expense and finds it by identity | inline | item 3 | — |
| E3-T1 | S9 | **deferred by the owner (L7)** — `GET /api/spending`: the period's spending grouped by category with purchases as one derived group | inline | — | — |
| E4-T1 | S8a | the ledger's page envelope and the server-side `unsettled` filter: `{rows, unsettled_older, cap}`, the shared predicate, the bounded count, and `frontend/src/lib/expenseLedger.ts` with the component asking the server instead of filtering a page | inline | item 3 | — |
| E4-T2 | S8a, S11 | RED-first proof in both levels: the integration test against a small page, the component's arithmetic unit-tested, the e2e whose older-day debt renders and takes an advance — each guard neutered once and watched to fail | inline | — | — |

**E3 is deferred, not dropped.** The owner's answer to classifying purchases by rubro was
*"pir ahora no, dejamos una versión chica y lo vemos mas adelante"* (L7). So the **small version
is E1-T4**: purchases appear in `GET /api/expenses/summary` as **one derived group**, which is
the owner's *"compras como una categoría"* at its cheapest. A per-rubro breakdown waits, and
when it returns it is `expense_classifications (document_kind, document_id, category_id)` with
its own guard — never a column on `purchases` (D6, A1).

Order is dependency, not preference: E1-T2 needs T1's table, T3 needs T2's document, T4 needs
T3's advances to report `paid`, and E2 needs E1 green. E1 and E2 are the whole of this
feature's committed work; E3 is the deferred remainder and can be dropped without leaving a
half-built document.

**Slice boundaries, so each candidate stays reviewable.** E1 is one migration and one route
family (~900 authored lines with tests) — above the 400-line guideline, so it carries a
`size:exception` with this paragraph as its reason: the table, the two `CHECK` re-cuts of
`payments`, the two-state trigger and the routes cannot be split without a commit that
compiles and does not work. E2 is the screen (~700). E3 is one read model (~250). The
precedent for refusing a candidate this size does not apply: `v4-payables.md` was refused at
**3556** lines, and the whole of E1 is a quarter of it.

## Log

L1 (owner, verbatim): "dale especifiquemos gastos"

L2 (owner, verbatim, answering who a gasto pays): "Por un lado quiero que registre las
compras como una categoria de gastos, asi que esos gastos se pueden derivar de compras que ya
tienen proveedor obligatorio. Por otro lado quiero registrar ordinarios que por ahora
podriamos dejar como texto. Y otro importante auqnue podriamos dejarlo para otro slice, es
registrar los salarios de empleados, pero todavia no tenemos user/staff creado, asi que
podemos dejar para mas adelante. Dato importante, quiero poder categorizar gastos"

L3 (owner, decision, answering how a gasto is classified): **`expense_categories` as a table
with a seed**, chosen over free text and over a fixed `CHECK` vocabulary. Reasons of record:
free text makes "Sueldos" and "sueldos" two categories in a report that an immutable document
can never correct; a `CHECK` is the closed-set anti-pattern this schema paid for three times
(D2).

L4 (owner, verbatim, answering whether a gasto can be owed): "un gasto puede quedar en deber.
Por ejemplo anoto el salario de un empleado del mes y este durante el mes puede ir retirando
adelantos. Es decir puede haber pagos parciales de gastos."

L5 (owner, verbatim, answering the slice shape): "API mas pantalla en 2 o mas slices, por ahi
es mejor divir en mas slice si se hace grande el feat"

L6 (owner, request, before accepting the design): "Conviene crear una tabla para gastos o es
preferible agregar un campo a compras para identificar a que gasto pertenece, a salarios
(cuando se cree a futuro) y los demas gastos anotarlos como movimientos de cuenta que tambien
lleve un identificador para que gasto corresponde?" — **both alternatives measured and rejected
as A1 and A2 in D6**, with the file and line of the guard that refuses each.

L7 (owner, verbatim, on classifying purchases by rubro): "pir ahora no, dejamos una versión
chica y lo vemos mas adelante" — deferred as D6a, with the shape it must take when it returns.

L8 (owner, verbatim, on salaries): "mi idea es que sea con empleado pero un documento con FK en
user" — satisfied by `expenses.staff_id` (D5a) rather than a `salaries` document, because the
latter duplicates the debt mechanism `0023` already paid for. The column is **not** reserved now
(D5b), and what D5a promises about not touching `payments` is D5c.

L9 (design, from L2's verb "derivar", and the reason it is a spec rather than an
implementation detail): a purchase is **not** copied into `expenses`. Two records of one fact
is the defect `0017` killed for balances, and `PRODUCT.md` forbids a report justifying a new
column at write time — so purchases enter the spending report as a derived term (S9) and
never as rows.

L10 (design, D3/D4): an owed expense is a **party to its own money** (`party_kind='expense'`,
`party_id=expenses.id`) rather than a third debt table with a third ceiling function. The
price — one payment answers one expense — is L4's mirror left unbuilt on purpose, and it is
written in D4 rather than left to be discovered.

L11 (evidence, E1-T1 delivered as `7593c16`): `expense_categories` with its six-row seed, the
delete and truncate guards, and `GET`/`POST`/`PATCH`. RED first — all four tests answered
`404` before the route existed. Both guards proven to bite by neutering: removing the trim
made the trim assertion fail, and replacing the delete trigger's `RAISE EXCEPTION` with
`RETURN NULL` let the `DELETE` succeed. 161 integration tests, 67 unit.

**The migration is `0002`, not a section appended to `0001`, and the reason is measured.**
`0001` is the squashed chain and is already applied: sqlx keys a migration by version *and*
checksum, so editing it either fails on every existing database or silently skips the new SQL
on one that recorded version 1. Confirmed with `select version, description from
_sqlx_migrations` → exactly `(1, schema)`.

L12 (review `review-32b53a14e35954c8`, approved, authority burned): tier medium, lenses `risk`
+ `reliability`, 2 reviewers, 4 non-blocking findings. **`R3-001` was a real hole and it was
reachable**: `0002`'s non-blank CHECK used `btrim(name)`, whose default trim set is a single
space, so `INSERT ... VALUES (E'\t')` succeeded while the route refused it (Rust's `str::trim`
removes the whole `char::is_whitespace` set). The two layers disagreed, which is the one thing
that file's own comment promised they would not. Measured both directions before fixing:
`btrim(E'\t\n') = ''` → false, `btrim(E'\t\n', E' \t\n\r\f\v') = ''` → true.
The other three were test gaps, passing before the fix and therefore confirming the diagnosis.
Fixed in `961a00d` with four RED-first tests; the whitespace one failed genuinely first.

L13 (review `review-63c8b89cb1e8f768`, approved after one bounded correction): this is the
review of the *fix*, and it found **`R3-001` CRITICAL, `deterministic`, `introduced`**: the
final assertion in `0003` read `count(*) = 6`, which is true only on a database that has never
had a category created — so any installation where the shop had added its own category could
never apply a security fix. Measured on disposable databases: with `0002` plus a `Fletes` row,
the committed `0003` answered `ERROR: expected the 6 seeded expense categories, found 7`,
while the corrected one applied cleanly. Fixed in `9da1939`; the assertion now counts **the
six seeded names**, which is the property it owed all along. Validator: `original_criteria`
passed, `correction_regression` passed.

L14 (debt carried out of `review-63c8b89cb1e8f768`, both excluded from its correction plan and
recorded here so they are not lost):

- **`R3-002` — the pre-check's diagnostic is unreachable.** `0003` adds the stricter `CHECK`
  *before* the `DO` block that promises a readable message for pre-existing whitespace-only
  names. PostgreSQL validates the new `CHECK` during `ALTER TABLE`, so on such a database the
  migration aborts there and the custom diagnostic never emits. The remedy message is a promise
  the file cannot keep. Correct shape: run the pre-check first, then the `ALTER`.
- **`R3-003` — the trim set is ASCII, Rust's is Unicode.** The `CHECK` trims
  `E' \t\n\r\f\v'`, while `str::trim` also removes U+00A0 and the rest of
  `char::is_whitespace`. Measured: `btrim(U&'\00A0', E' \t\n\r\f\v') = ''` → **false**, and
  `"\u{a0}".trim().is_empty()` → **true**. So a name of one non-breaking space still passes the
  database guard and is refused by the route — the same disagreement `R3-001` fixed for ASCII,
  left standing for Unicode. Both are informational and neither blocks delivery.

L15 (review `review-1e9c4c74ced6d94b`, CRITICALs found on the delivered UI): the first
review that saw the screen found three, and **all three were mine, on code the tests had
already declared green**.

- **`R3-002` — a date-only value was read in UTC.** `new Date('2026-03-15')` is
  `2026-03-15T00:00:00Z`, which in the shop's zone (UTC-3) is **the 14th**. Measured, and
  the first fix I attempted was itself wrong: sending the bare `YYYY-MM-DD` to the server is
  a 422, because the field is a `DateTime<Utc>`. The real fix resolves the date in
  `SHOP_TIMEZONE` on the **server**, through `day::first_instant_of` — the same helper
  `/api/day` already uses, so DST is answered once.
- **`R3-003` — an owed expense outside today could never be paid.** The ledger was scoped to
  the current shop day by `occurred_at`, so an owed salary from an earlier day rendered no
  row and no advance form. The feature's reason for existing was unreachable from the screen.
- **`R3-001` — editing an applied migration is silent, and that is worse than failing.**
  Measured: `sqlx` keys by version, so a database that had recorded version 4 kept the **old
  trigger body** with no checksum error, no re-run, and exit 0 — while that body refused
  every advance. `0006` re-creates the functions and triggers idempotently and asserts their
  **bodies**, because a trigger with the right name and the wrong body is what a stale
  database holds.

The correction measured **351 diff lines against a frozen budget of 200**, so the provider
classified it as a scope change rather than a bounded correction and offered `RECOVER` with
`scope_changed`. The owner authorized it; the predecessor lineage closed and
`review-0e3269a3a9623ddc` was opened over the same base, reviewed, **approved**, and its
authority burned.

L16 (debt: what that review left, and one it found that this slice does not fix):

- **`R3-004` of `review-1e9c4c74ced6d94b`** — the ledger requests the default page size and
  never paginates, so a day with more than 50 expenses silently omits the rest while the
  summary foot still covers the whole day. Not admitted to the correction plan.
- **`R3-003` of `review-0e3269a3a9623ddc`, and it is the sharper one** — the fix for
  `R3-003` above introduced an **unscoped** `listExpenses({ limit: 200 })` and filters in the
  client. Two queries for one screen, and the unscoped one grows with the shop's whole
  history: past 200 documents an owed expense older than that is invisible again — the same
  defect, merely pushed further out, and the cap is not surfaced. **The correct shape is a
  server-side filter for "owes something", not a bigger page size.**
- **`R3-001` of that review** — `day::first_instant_of` became `pub` for this slice and has
  no unit test of its own; its behaviour is pinned only transitively through the route tests.
  **Closed in `test/day-first-instant-unit-tests` (L22).**
- **`R3-002` of that review** — the advance route's `409` is advisory; the trigger is the real
  enforcement. Recorded because a future change that dropped the trigger would leave a `409`
  that no longer holds.

L17 (owner, verbatim, on `L16`'s sharpest item): "Lo que queda abierto ... El más accionable lo
encontró la review sobre mi propio fix: el arreglo de R3-003 usa una lista sin scopear topada en
200, así que un gasto adeudado más viejo vuelve a ser invisible — el mismo defecto que ese fix
cerraba, empujado más lejos. La forma correcta es un filtro del servidor por 'debe algo', no un
page size más grande. abri un issue y trabajemos sobre esto" — tracked as issue #115, and E4 is
its work.

L18 (evidence, E4-T1 and E4-T2 delivered together). **Measured before claiming anything**: the
report on #115 was reproduced first, in `tempox_test` inside a rolled-back transaction, by
inserting two recent paid expenses and one owed expense dated `now() - interval '400 days'` and
running `list_expenses`' own statement with `LIMIT 2` standing in for `LIMIT 200`. The owed row
was absent from the page — the same shape the review described, and the reason the report quotes
that output rather than a number.

- **The endpoint.** `GET /api/expenses` answers `{rows, unsettled_older, cap}` and gained
  `unsettled=true`, which composes with `from`/`to`/`limit`/`offset` instead of replacing them.
  The predicate is `e.amount - COALESCE(p.paid, 0) > 0`, and it is deliberately **not**
  `payment_method_id IS NULL`: an owed document with one advance still owes, and the older
  filter would have printed it as settled and hidden its form. The page and its remainder are
  read under one `REPEATABLE READ READ ONLY` snapshot, so the pair cannot describe two
  different ledgers.
- **`unsettled_older` is bounded, and that is the design rather than an optimisation.** It
  counts the remainder up to `MAX_LIST_LIMIT`; an exact count over the shop's whole history
  grows with it, and a figure nobody can act on is not worth an unbounded query. It is `0` when
  the flag was not sent — absent, not a lie about a count nobody asked for.
- **RED observed in both levels.** The integration test failed first with `invalid type: map,
  expected a sequence` while the endpoint still answered a bare array. The frontend's arithmetic
  failed first with the module missing. `frontend/src/lib/expenseLedger.ts` is new because that
  arithmetic lived inside a `$effect` and was the wrong arithmetic.
- **Both guards neutered once and watched to fail.** Replacing the `unsettled` predicate with
  `TRUE` returned the paid documents (`left: 5, right: 1`); disabling the remainder count
  returned `left: 0, right: 1`. The e2e was proven the same way: with the older-debts ask
  stubbed to an empty page — the pre-fix decision, day-scoped only — the row vanished
  (`toHaveCount` resolved to 0 elements fourteen times) and passed again when restored.
- **The header's count was wrong too, and it is a separate assertion.** `oncount` received
  `expenses.length` of the merged list — the day plus every older debt rendered under it — while
  the foot beside it reported the day's figures. A day with two expenses could display seven
  documents over a foot covering two. It now receives the day's own count,
  `expensesInDay(todays.rows, bounds).length`, and `Props.oncount` became `number | null` to
  match the purchases ledger. The e2e asserts the figure is unchanged by recording an older-day
  debt.
- **What the envelope costs, stated.** One extra bound query per `unsettled` request, and the
  day's screen now makes three reads instead of two (day, older debts, foot) — up from two, and
  every one of them is bounded rather than "everything recent and hope". The three compose no
  snapshot, which is why the merge deduplicates by id: a document created between the two
  answers can legitimately appear in both, and rendering it twice would put two advance forms
  over one debt.
- **A second test exists because the first would pass with the shortcut predicate**, and that
  is the trap the review named: `payment_method_id IS NULL` is *almost* "owes something", and
  diverges exactly where an owed document has taken an advance (the advance names its own rail,
  the document keeps its null). `v4_expenses_a_partially_advanced_debt_still_answers_unsettled`
  pins both edges: a debt with one advance still answers, a fully advanced debt does not. Proven
  by swapping the predicate for the shortcut — the settled document reappeared as owed and the
  assertion failed on `left: [.., ..], right: [..]`.
- **Counts**: 182 backend integration (was 180), 67 unit, 6 limits, 274 frontend unit (was 266),
  41 e2e (was 40). `docs/en/architecture.md` gained the envelope's route row and the invariant
  "a ledger is a page, and a page that is not the whole says so".
- **Still open from L16, unchanged by this**: `R3-001` (`day::first_instant_of` pinned only
  transitively), `R3-002` (the advance `409` is advisory; the trigger enforces), and `R3-004`'s
  older half — the day's own list is still a page of 50 with no next-page control. That last one
  is now *sayable* rather than silent only for the older-debts list; the day's list still
  truncates without a notice, and that is the next candidate.

L19 (review `review-16c84931d9274aaf`, **retired without approval, and it was my ordering
error**, not the provider's). I applied the `R2-001` fix and **committed it before submitting
the correction plan**, which moved the candidate out from under the reviewed target
(`ec75b27…` → `2c0ecda…`). Every correction-plan binding I then submitted was correctly
refused as stale, and STATUS reported `authority_target_identity: ec75b27…` beside a newer
`target_identity` — the provider saying plainly that the authority and the target had parted.
The order is: plan against the reviewed target, then fix, then let the provider validate. The
fix itself was right (`2766172`) and it survived into the next candidate.

L20 (review `review-045709e4682ea82c`, **approved**, authority burned; `R3-001` corrected and
validated). A fresh lineage over the corrected tree, because the previous one's target no
longer existed. It found one more CRITICAL, and it is the sharper lesson of this slice.

- **`R3-001` — the owed ask was bounded by a day, and a day is not where debts live.** The
  older-debts request sent `to: day.from`, reasoning that an owed document belonging to the day
  is already in the day's own list. That reasoning covers **today only**: `expenses.occurred_at`
  is the day the expense belongs to, and a salary entered before it is due is precisely what an
  owed expense is for. So a future-dated owed expense was fetched by neither list — the day's
  list stops at `day.to`, this one stopped at `day.from` — and it rendered before the change and
  vanished after it. Corroborated by the refuter; fixed in `7324a1b` by removing the bound
  entirely, since `mergeOlderDebts` already deduplicates by id.
- **Why two rounds of this class**: both findings are "a page replaced a whole, and the hole
  moved". `R2-001` narrowed the day's page; `R3-001` bounded the debt question by a day. The
  third test is the one that should have existed first: **the union of the two asks must cover
  every owed document, wherever its date falls** — long before the day, inside it, and after it.
  Two unit tests and an e2e all stayed green while the document disappeared because none of
  them asked the question in those terms.
- The correction plan was 120 diff lines against a frozen budget of 200, accepted; the validator
  confirmed the fix and the review closed `approved`.

L21 (advisory findings of `review-045709e4682ea82c`, all non-blocking, recorded so they are not
rediscovered as surprises): `R2-001` `expenseLedger.ts:101-102`; `R2-002` `en.ts:314-317`;
`R2-003` `expenseLedger.ts:65-71`; `R2-004` `expenseLedger.ts:83-86`; `R2-005`
`v4-expenses.md:571-575`; `R3-002` `expenseLedger.ts:102`; `R3-003`
`backend/src/routes/expenses.rs:761`. None opened a correction and none reopens the review.

L22 (evidence, `R3-001` of `review-1e9c4c74ced6d94b` closed — `day::first_instant_of` now has
unit tests of its own). Debt carried since `L16`; no behaviour changed, only coverage, so there
was no RED to observe and the proof is the inverse: break each branch and watch its test fail.

- **What the indirection was hiding.** Every existing test reached the function *through*
  `day_range`, which pins the endpoint's behaviour well and says nothing about the function's
  branches. Enumerating them directly showed `Single` and `Ambiguous` covered only in the zones
  and dates `day_range`'s fixtures happen to use, `None` reached by accident, and the bounded
  walk's fallback covered by nothing at all.
- **One hypothesis was wrong and the measurement is the record.** The code comment said a gap
  is "at most a couple of hours" and the fallback "unreachable for any real zone". Samoa deleted
  30 December 2011 entirely, so I predicted the 1440-minute walk would fall through to the
  fallback. It does not: the walk lands on 31 Dec 00:00 +14, which is 30 Dec 10:00Z — a real
  instant, ten hours from the nominal midnight. Verified independently with tzdata
  (`TZ=Pacific/Apia date -d @1325239200` → `Sat Dec 31 00:00:00 +14 2011`). Both comments were
  corrected rather than the test; asserting the predicted fallback would have encoded a claim
  the code never made true.
- **Six tests**, all calling the function directly: a normal day, UTC as a zone like any other,
  a gapped midnight (Santiago 2027), an ambiguous one taking the **earlier** reading (Havana
  2027), the skipped calendar day, and a property sweep over five zones × twelve months
  asserting that whenever local midnight exists the answer is that midnight on that date.
- **Neutered three ways and each failed.** `first.min(second)` → `first.max(second)`: the
  ambiguous tests failed, the others stayed green. The walk stepping backwards: four tests
  failed. `Single` forced to a fixed -3 offset: eight failed. Restored, 73 pass.
- **Counts**: backend unit 73 (was 67). No other suite touches `day.rs`, so integration, e2e and
  the frontend are unaffected — and that is worth stating because it is exactly why the gap
  survived this long.

L23 (the other half of `R3-004`, delivered). The day's stack was a page with a notice and
nowhere to go: #116 made it *say* when part of the day was off screen, which was honest and left
201 documents a wall. This adds the pager, and the four things measuring it taught:

- **The arithmetic lives in `expenseLedger.ts`** (`dayOffset`, `dayPageCount`, `dayPageNumber`,
  `canStepPage`) for the reason the rest of that module does: a `$effect` is not a place
  anything can be asserted. The clamp is the rule that matters — `total` comes from the
  summary's exact `documents`, so the last page is a known address and the pager cannot ask for
  one that holds nothing.
- **`dayIsTruncated` changed meaning, and the old semantics were the bug.**
  `rowsHeld >= cap` asked "is this page full", which is not the question once a pager exists:
  the *second* page of a 300-document day also compares false. `rowsHeld < total` answers "is
  part of the day off screen", which is true on every page that is not the whole. The two
  disagree exactly where it matters, and three tests that had been green under the old
  semantics were rewritten rather than kept.
- **The pager's labels were inverted, and the e2e found it.** The stack is newest-first, so
  offset 0 is the *newest* page: "Older" is the forward offset and "Newer" steps back towards
  zero. The first markup paired `stepPage(-1)` with "Older", read correctly, and did the
  opposite. The code comment records it so the next reader does not "fix" it back.
- **Three defects were in the test, and each one was a decoration test in disguise.** The e2e
  first asserted only that pages were non-empty, which passed with the request's offset
  hard-wired to `0` — the pager rendered, the label advanced, and every page showed the same
  rows. It now asserts each page leads with a different document, and that assertion is what
  fails when the offset is broken (measured). The second and third were a duplicated backward
  click and a stale `isDisabled` read, each of which left the walk a page short.
- **The shared, append-only e2e database is why the count is never asserted.** An earlier draft
  asserted "de 2" and failed with "Página 1 de 3"; a later one asserted the oldest document was
  absent from page one and failed once the day held a thousand documents from previous runs. The
  test now claims what is true regardless of history: the pager names more than one page, every
  page it reaches leads with a different document, and walking forward returns to page one.
- **Counts**: frontend unit 292 (was 280), e2e 42 (was 41), backend unchanged at 73/182/6.
  `make lint` clean.

L24 (the class closed where it actually lived). `R3-004` was recorded as an expenses defect; it was
a **repo-wide** one wearing three costumes, and this is the generalisation:

- **Ventas** said its table was a page and gave the reader nowhere to go — the same "notice with
  no destination" the expenses fix had just removed one screen over.
- **Compras** asked for 200 rows and let the rest of the day go **in silence**, with a count line
  that read as the period's.

So the arithmetic moved to `frontend/src/lib/ledgerPaging.ts` (`clampOffset`, `pageCount`,
`pageNumber`, `canStepPage`, `pageIsPartial`), owned once for the three ledgers, exactly as
`methodLabel.ts` owns the empty-rail rule for the same three. `expenseLedger.ts` keeps its
`day*` names as thin wrappers so that module still reads as being about a day.

- **Three defects found while wiring it, all real**: the sales pager moved the label and not the
  table, because `pageOffset` was not a dependency of the effect that reads the day; the count
  line on purchases reported the page as the period; and the pager labels are inverted by the
  newest-first ordering, which the expenses slice had already paid to learn.
- **The e2e bit back four times, and every failure was a decoration test in disguise** — the
  pattern `v5-customer-account.md` V2b names. It branched on whatever the shared database held
  ("the period fits", so removing the pager still passed); it read `.foot .note` for a class that
  is `.footings .note`; it guessed `<tr>` rows where every ledger uses `data-doc-row`; and it
  compared `innerText` of a slip, which Playwright normalises differently between reads. Each was
  fixed by asserting the identity instead of a derived rendering.
- **Neutered four ways and each failed**: purchases ignoring the offset; purchases hiding the
  partial notice; the sales pager not rendered; and `pageOffset` dropped from the effect's
  dependencies.
- **Counts**: frontend unit 307 (was 292), e2e 44 (was 42). Backend untouched.

L25 (three review rounds, and the honest account of why). Lineage
`review-58ab3200d6f59c90` found 2 CRITICAL, `review-811cd5f5c6e59feb` found 3 more in the
correction, and its validator then rejected the third-round correction while **passing**
`R3-001` and `R3-002`. Every finding was in the same family, in the same two files:
**pagination state that outlived the context it was captured in.**

- `R3-001` (round 1): purchases passed `() => false` as its staleness predicate, so no response
  was ever stale.
- `R3-002` (round 1): the post-refund reload assigned rows without asking whether the reader had
  paged.
- `R3-001` (round 2): the clamp invalidated **its own** re-read, because it changed the offset
  and then judged staleness by that same offset. A logical deadlock: the clamped page was never
  fetched. **Corrected and accepted.**
- `R3-002` (round 2): the period was not part of the comparison, so a period change at the same
  offset was invisible. **Corrected and accepted.**
- `R3-003` (round 2), still open: `reload` captures `requested = pageOffset` **after** `stepPage`
  already moved it, so `offsetAfterFailedLoad(requested)` "rolls back" to the new offset. The
  helper and its tests are honest about their own contract and useless for the defect, because
  the previous offset was lost before `reload` was entered.

**The pattern, stated so the next session does not rediscover it.** The offset lives in the
component, and every asynchronous path has to remember to capture, compare and roll back — so
each fix created the next finding. The rule that would close the class is one owner: a single
function that runs a read and decides whether its result still applies, given the context
(day, period, page) captured **where the read is issued**, not where it is consumed. Rolling
back needs the previous value passed in, not re-read from the state it was moved from.

**Recommendation carried to the maintainer**: refactor before merge, not a fourth round of
patches. The three corrections that landed are individually sound and tested (315 unit, 44 e2e,
lint clean); what is unsound is the shape they are patching.

L26 (the single owner, and what it did and did not prove). `refactor/ledger-paging-owner`, branched
from `f0fae7a` so it starts on the three fixes that passed two validations rather than on `main`.

`frontend/src/lib/ledgerPage.ts` owns the offset in one closure. A component reads `offset` and
`total` through getters and calls `read`/`step`; there is no setter, so the five findings' cause —
"the offset lives in the component and every async path must remember to capture, compare and roll
back" — is not expressible rather than merely fixed. `read` captures `{offset, span, generation}`
at issue time and discards its own answer when any of them moved; the clamp re-read is issued
inside the owner so it cannot judge itself stale; `step` captures the previous offset before moving
it, so the rollback cannot restore the value it was meant to undo.

**What is proven, and what is not.** Seven tests, and each was verified by deleting the code it
covers:

- deleting the rollback's offset restore fails 2 tests (`R3-003`);
- deleting the clamp re-read fails 1 (`R3-001`, the deadlock);
- **deleting the span guard fails none**, and deleting the offset guard fails none — because a
  newer read also bumps the generation, and that guard fires first. Those two are therefore
  **untested claims**: kept because they are the only thing that can catch a step or a period
  change inside a read that does not bump the generation, and justified by reasoning, not by a
  test that fails without them. An attempt to construct that scenario hung on its own gate and
  was deleted rather than kept as a decoration.

Also corrected in this pass: the first version of the two staleness tests **passed with the guard
deleted**, because they released the stale answer before the newer one and the newer one's apply
landed last. A test whose conclusion is "the last write wins" cannot see a stale write that
happens to be first. Releasing the stale answer *last* is what makes the defect visible; the same
mistake, in the test this time, as the round before.

Counts: frontend unit 322 (was 315), e2e 44 unchanged, backend untouched, `make lint` clean.

L27 (the owner is a class with runes, and `PurchaseLedger` is migrated). The first version of
`ledgerPage.ts` was a plain closure and **could not be wired**: `let offset` inside a function is
not Svelte state, so reading `page.offset` from a template created no reactive dependency — the
pager would have rendered once and never updated. Svelte 5 tracks runes only, and runes only work
in `.svelte.ts`. It is `ledgerPage.svelte.ts` now, a class whose `offset` and `total` are `$state`
and whose generation counter deliberately is not (a counter that is rendered would re-run every
effect on every read).

`PurchaseLedger` uses it. What was deleted: `pageOffset`, `loadSeq`, the four-branch `loadDay`
with its own clamp and its own predicate, `reload` with its captured `requested`, and the three
comparisons written by hand. What replaced them: a fetcher that answers one request, an `apply`,
and `page.read`/`page.step`. `R3-003` is now unexpressible there — the owner captures the previous
offset before moving it, so the rollback cannot restore the value it means to undo.

`R3-001` (the clamp deadlock) and `R3-002` (page and period) are the owner's, in one place, for the
two ledgers still to migrate. `Ledger` (sales) still calls `isSamePageResponse` itself; a test
asserts both states so the remaining migration is visible rather than forgotten.

Counts: frontend unit 321, e2e 44, backend untouched, `make lint` clean. `svelte-check` 0 errors.

L28 (browser mode, and the two bugs it found in ten minutes). Adopted
`@vitest/browser` + `@vitest/browser-playwright` after six failed attempts at migrating sales by
reasoning alone. `vite.config.ts` now has two projects: `unit` (jsdom, 321 tests, unchanged speed)
and `component` (Chromium, for anything that needs to mount Svelte).

**The first thing it settled was that I had been blaming the wrong thing.** A probe that mounts a
component whose effect reads a class rune and then writes it from outside proved the effect *does*
re-run — including with an `await` in the body. So `await_reactivity_loss`, the hypothesis I had
been working from, **did not apply**. The docs name it for state read *after* an await; the ledger
reads synchronously. Six attempts were spent on a cause that was not there.

**Two real bugs, both found by measuring rather than arguing:**

1. **`expanded.clear()` ran on every read.** Correct while the effect only fired on a scope or day
   change; after the owner was introduced a recorded credit note refreshed the page, the effect ran
   again, and the open row collapsed — and the credit-note block lives inside that row, so the note
   vanished from the screen while existing on the server. Pinned by
   `tests/component/expanded-survives-refresh.test.ts`, which shows the row surviving after the fix
   and collapsing with an unconditional clear.
2. **`const pageTotal = periodDocuments;` captured once and froze.** Not a `$derived`, so it held
   the initial 0 forever. The symptom was precise and misleading: the summary said 74 documents,
   the table showed 50, and the note still claimed "the whole period". Found by a probe e2e that
   printed both numbers, after three wrong guesses.

Sales now has **one reader** — its effect — and a write moves `page.revision` instead of reading.
`stepTo` moves the offset and the revision together for the same reason: one trigger, no second
reader to race.

Counts: unit 321 / component 2 / e2e 44, backend untouched, `make lint` clean.

L29 (the two independent CRITICALs, closed in one PR; and the one scope decision the owner stated).
`#122` and `#123` are the batch the issue comments predicted would be independent — different files,
no shared state — so they share one branch and one PR (`fix/ledger-superseded-load-and-refund-order`)
and each keeps its own commit, so each can be read alone.

**The owner's constraint, verbatim:** "este PR no toca ledgerPage.svelte.ts". It is honoured, and it
turned out to be load-bearing rather than a preference. Both fixes are component-side:

- `#122` — `PurchaseLedger`'s effect lost the `isStale` predicate when the screen moved onto the page
  owner, so a superseded load published its error and cleared `loading`: `readDay(at).catch((e) =>
  (error = describe(e))).finally(() => (loading = false))`. `LedgerPage` decides whether an *answer*
  is applied; a rejected read never reaches it, so it structurally cannot decide whether a *failure*
  belongs on screen. The guard that was deleted is restored in the component, where the question
  actually lives.
- `#123` — `getCash()` was made unconditional to close `R3-CORR-001` and placed **before** the
  refresh, so a failing balance read threw past `page.refresh()`: the note existed on the server and
  the row never showed it. The refresh is now issued first and not awaited (it only moves
  `revision`), the cash figure refreshes after it, and the two failures are separate again.

**What the constraint deferred, stated rather than silently dropped.** The `stepPage` path has the
same shape of question as `#122` — a page read that fails has no way to ask the owner whether a newer
read superseded it — and it is **not** patched here. Answering it needs the owner to say whether a
*failed* read was superseded, which is `#120`/`#121`'s design question (`L25`'s "one owner" answer,
taken one level further: the owner owns the failure as well as the answer). Patching it from the
component would be the fourth hand-written generation comparison in a file `L25` records as having
failed five times that way.

**Three tests, and each was proven to bite** by deleting the code it covers, measured:

| test | deleted | failed |
| --- | --- | --- |
| `purchase-ledger-superseded-load.test.ts` › does not publish its error | `if (!cancelled) error = …` → `error = …` | 2 tests |
| … › does not clear the loading state | `if (!cancelled) loading = false` → `loading = false` | 1 test |
| `refund-cash-failure.test.ts` › still reloads the rows | `page.refresh()` moved back below `getCash()` | 1 test |
| … | `refundStale.mark(…)` → `refundStale.retireAll()` | 1 test |

**Both are against the real components, not a harness.** The first draft of the refund test defined
its own copy of the sequence in a harness and asserted that — which is exactly `R3-CORR-002` of an
earlier round, the validator's sharpest follow-up ("the unit test defined its own local copy and so
exercised nothing the components implement"). Extracting the sequence into a module was rejected for
the same reason it was there: the shared thing is two calls in an order, and a module that owned the
order would exist only to be testable while the component kept doing whatever it liked. Both tests
now mount the component against a mocked client and drive the real interaction.

The first harness also hung, and the reason is worth keeping: its fetcher wrote `$state` from inside
the reader's effect, which re-ran the effect, which read again — an infinite flush that surfaced as
Svelte's DEV warning `[updated at]` repeating forever, not as a timeout. The narration is plain now.

Counts: unit 327 / component 6 / e2e 44; backend untouched, `make lint` clean, `svelte-check`
0 errors.

L30 (the sixth round, and the honest account of why my correction did not close it). `L29` claimed the
constraint "no tocar `ledgerPage.svelte.ts`" was load-bearing and correct. The owner lifted it, the
correction landed (`77da969`, 193 lines), and the re-review found five CRITICAL findings that I had
introduced. Two were mine in the worst way: **the fix changed the defect instead of closing it.**

**What the facade could not show, and the native inspection could.** The second lineage,
`review-b263118903947f58`, closed `correction_required` and then every subsequent call answered
`corrupted_or_unverifiable_authority`. That diagnosis was wrong. `gentle-ai review status` on the
CLI reports the authority as **`status: active`, `problems: []`, `diagnostics: []`, no locks**, and
`review inspect-authority` counts 37 entries with 0 invalid edges and **0 sanctioned exits**. What is
broken is the START handshake — reusing a lineage already in `correction_required` makes the
negotiated repository context disagree with the active one (`candidate_context_unavailable`). The
findings were on disk the whole time, in
`.git/gentle-ai/review-transactions/v2/<lineage>/review-state.json` under
`state.admitted_role_results[].value.result.findings`, and the refuter's verdicts beside them.
**Recorded so the next session reads the file instead of concluding corruption.**

**The five findings, each measured rather than believed.** A probe mounted the real component:

| finding | measured |
| --- | --- |
| `R3-002` | **one** pager click produces **two** reads of offset 200. The effect re-runs because `readDay` reads `page.offset` reactively, and it claims *after* the step, so the step is always superseded. **My gate therefore makes the rollback unreachable** — the exact inverse of what it claimed to fix. |
| `R4-001`/`R3-003` | a failed step leaves `Página 2 de 3` with **0 rows** and an error. The offset advanced and nothing restored it. `stepTo` (sales) has no rollback path at all; `step` (purchases) has one that can no longer run. |
| `R3-001` | the loading test clicks the header's day button, not the footer pager's — the same mistake my first probe made, which is why it went unnoticed. |
| `R3-004` | `refundStale.retireAll()` runs before the `try`, retiring notices before any read has returned. The `#123` fix kept the ordering bug and I did not see it. |

**The cause is a second reader, and it is structural.** Purchases has `page.step` (imperative) *and*
an effect that re-runs on `page.offset`; sales has `stepTo` (which only moves the offset) and an
effect. Two readers of one page cannot be gated against each other by a token in either one. Sales
already has the right shape — **one reader** — and purchases was never migrated to it. That is the
work, and it is `L25`'s rule applied one level deeper than `L26` took it: not "one owner of the
offset", but "one reader of the page".

**What is not in doubt.** The unit suite (322), component (6), e2e (44), `make lint` and
`svelte-check` are green, and each new rule bites when its code is deleted. Green tests and a
correct design are different claims; this log records the second one failing.

**State: the candidate is not approved.** The re-review is blocked on a START handshake that cannot
be replayed (`retry_safe: false`), and the correction it demanded is known-insufficient. Resolving
it needs either a fresh candidate with one reader per ledger, or the owner's decision to ship
#122/#123 alone on the `de56`… base where these findings do not apply.

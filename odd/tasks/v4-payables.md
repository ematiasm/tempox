# v4-payables — what we owe, and the money that pays it

Slice **S6b / T7** of `odd/tasks/v4-settlement.md`, and the mirror of
`odd/tasks/v4-receivables.md` (S6a). **State: closed. T7.1-T7.5 implemented and green (146
backend tests), delivered as PR #87 (merged 2026-10-25), and deliberately not reviewed
natively — see L9.** S1, S3, S4, S5, S6a and S7 are merged to `main`; S2 is deferred to v5.
`0023` and the three routes **never received an outside view**, and that is the debt this
header states rather than an oversight it leaves for a later reader to discover.

The state that must hold when this slice closes, and it is one sentence:

```
a purchase on account writes a payable and no money,
and a return of a purchase nobody paid lowers the payable instead of taking money back.
```

## Specs

S1 **A payable is not a ledger row, and neither is the purchase that creates it.**
   A purchase on account writes `payables` and **zero** `money_entries` rows. Money
   appears in the book only when a supplier payment settles the payable.

   `v4-settlement.md` S5, read on the purchase side. The symmetry is the point: `credit`
   is not a wallet, and until this slice a purchase **could not** go on account at all —
   `purchases.rs` refuses the debt method (`S9b` of `v3-accounts.md`: *"there is no
   purchase-side counterpart to a credit sale until the payable document exists"*). That
   counterpart is this slice, and it is a document, not an account.

   **Its own table, not `obligations`** (`v4-settlement.md` L25, the owner's decision, and
   `L2`-`L4` below). The three new tables are `payables`, `payables_adjustments` and
   `payable_settlements`; `0021` and `0022` are **not touched**, which is the whole point of
   the choice — the two operations this feature has already got wrong twice are an `ALTER`
   over a closed set and a `CREATE OR REPLACE` of a function body.

S2 **`purchases.payment_method_id` becomes nullable, and the null is a claim, held by a
   constraint trigger.** The two states are enforced together, exactly as `0021` does for
   a sale:

   ```
   payment_method_id IS NULL      →  exactly one payable names this purchase
   payment_method_id IS NOT NULL  →  zero payables, and at least one ledger entry
   ```

   `DEFERRABLE INITIALLY DEFERRED`, because the purchase is inserted before the payable
   that names it — the same reason, and the same trap, `0021` records: no `ALTER TABLE` on
   this table may follow a row-touching statement in the same transaction.

   **Existing purchases keep working unchanged**: `0017`'s backfill wrote one entry per
   purchase and no purchase ever named the debt method, so every row already in a database
   satisfies the non-null half. The migration proves that rather than assuming it.

S3 **A purchase accepts the same three destinations a sale does, and the sentinel is
   defined once.**

   | input | meaning |
   |---|---|
   | omitted | the seeded `cash` method — what every purchase meant before v3 |
   | a uuid | money left through that method |
   | `"on_account"` | **money did not move**; the purchase owes, and `payables` records it |

   An explicit `null` is `BAD_REQUEST`, and the reason is `0021`'s: `Option<Uuid>` cannot
   tell `null` from an absent key, so null would have to mean either "the till" — silently
   turning a debt into a cash purchase — or "on account" — silently turning every older
   client's omitted field into a debt.

   **`"on_account"` on the one-off supplier is refused `400`**, mirroring `walk-in`:
   anonymous debt is not debt. The check asks the *supplier*, not `is_active`, for the
   reason `sales.rs` records — inactive methods are a picker concern, and gating on one
   would make the refusal fire for the wrong cause.

   **One definition, not two.** `Destination` and its deserializer move out of `sales.rs`
   into a shared module, because a second hand-written copy of a serde decision this subtle
   is how two spellings of one contract diverge.

S4 **A payable belongs to the purchase's supplier, derived and never trusted.**
   `payables(id, source_document_kind, source_document_id (UNIQUE), party_kind, party_id,
   amount >= 0, occurred_at, created_at, created_by)` with `source_document_kind IN
   ('purchase')` and `party_kind IN ('supplier')`, and a constraint trigger that reads the
   purchase and refuses a party that disagrees with it. `v4-settlement.md` S5: *"the party is
   derived from the document, never trusted from the caller."*

   `amount >= 0` and not `> 0` for `0021`'s measured reason: a zero-total document is
   legitimate (`0005`, `0009`), and refusing it turns a valid document into a 500.

S5 **Lowering what we owe is its own document: `payables_adjustments`.**
   `payables_adjustments(id, payable_id, amount > 0, document_kind ∈ {supplier_return,
   cancellation}, document_id, created_at, created_by)`, one row per document. Not a mutable
   `payables.amount`: `0019`'s writer-flag pattern cost a CRITICAL finding
   (`v4-settlement.md` V7), and the honest shape exists here.

   Outstanding is `amount - SUM(payable_settlements) - SUM(payables_adjustments)`, derived
   and never stored.

S6 **A supplier payment settles a payable: `payable_settlements`.**

   `payable_settlements(id, payable_id, payment_id, amount > 0, UNIQUE (payable_id,
   payment_id), created_at, created_by)` with both ceilings as sums over rows:

   - `SUM(settlements.amount) + SUM(adjustments.amount) ≤ payables.amount` — a debt may not
     be resolved twice, once by a document and once by money;
   - `SUM(settlements.amount) ≤ payments.total` — a supplier payment may cover several
     payables, and a payable may be paid in parts.

   A settlement names **money that went out**, to the **same supplier** the payable belongs
   to. Both invariants live in the trigger and in the handler, for `0022`'s reason: only the
   handler's is reachable over HTTP, only the trigger's over `psql`.

   **No ledger entry**, exactly as a receivable settlement writes none: the money moved when
   the payment was recorded; the settlement only says which payable it answered.

   **This ceiling is a second copy of `settlement_within_both_ceilings`, and that is a
   recorded cost** (`v4-settlement.md` L25): the rule is the same, the table it is written on
   is not. If the rule ever changes, both functions move together.

S7 **A payable settlement may not spend money we never sent.** The mirror of
   `lock_party_balance`, in one helper:

   ```
   + SUM(payments out)              money we sent the supplier
   - SUM(payable_settlements)       of which is already applied
   ```

   There is no third term because a supplier cannot pay *us*: `payments_direction_matches_party`
   (`0022`) makes `in` from a supplier inexpressible. Said here rather than left to be
   re-derived, and it is the one place the mirror is **not** symmetric — the customer's
   balance has three terms, the supplier's has two.

   The lock is on the party's payment rows, taken **before** the payable and before the
   payment row, which is the order `aa95665` fixed for the receivable side. **One order,
   everywhere** (`v4-receivables.md` L31: a fix that inverted it created the deadlock it was
   closing).

   **Corrected after measuring it (L8): this ceiling is a restatement, not an independent
   guard.** On the customer side the middle term (`settlements`) is not recoverable from the
   per-payment remainders, because a refund takes money back out without touching a settlement —
   that is what made `R1-001`/`R3-001` real. Here `Σ(payments out) − Σ(payable_settlements)` is
   **identically** `Σ(payment.total − applied)`, so the per-payment ceiling implies it term for
   term. Measured on a live database: both formulas, one fixture, `30.000000` and `30.000000`.
   Neutering the helper's comparison leaves every test green, and that is arithmetic rather than
   a weak test. The helper stays because S7 is stated in one place and because a future third
   term (an inbound supplier payment, if the schema ever allows one) would make it a real guard.
   What it is **not** is coverage, and the test says so where it lives.

   **Paying a supplier more than we owe stays allowed**, and it is the mirror of `L20` of
   `v4-receivables.md`: the excess is a derived credit balance (`sent - applied`), never a
   stored one. Recorded so a later session does not read its absence as an oversight; the
   route to *spend* it is not this slice.

S8 **`supplier_returns.payment_method_id` becomes nullable, and the null lowers the
   payable.** A return that inherits a NULL purchase method writes **no** ledger entry and
   exactly one `payables_adjustments` row. A return may be NULL only when its purchase is
   NULL — a return on a paid purchase that moved no money would be money from nowhere, the
   same rule `0021` states for a credit note.

S9 **`supplier_returns` gains the override, and its guard asks where the money actually
   left.** `v4-settlement.md` L19: *"`supplier_returns` inherits the purchase's method with
   no override … and that inheritance is only true for a purchase that was paid. Returning
   goods on an unpaid purchase credits the debt; it does not hand money back."*

   An explicit method means money comes back through a rail, so **that rail must have been
   where the money left**: a return naming method `m` on purchase `p` requires

   ```
   what p paid out through m  −  what p's earlier explicit returns already took back through m
       ≥  this return's total
   ```

   where "what `p` paid out through `m`" is the purchase's own entry when it was paid, or the
   settlements of `payable_settlements` whose payment went out on `m` when it was owed.

   The inherited path stays free: it lowers the payable and moves nothing, so it has no
   precondition to check. That asymmetry is the whole fix, and it is the mirror of
   `v4-receivables.md` S7.

S10 **The refusals keep their exception channel.** The debt account is `400`; a ceiling is
    `409`; a second payable on the same purchase is `409` (the `UNIQUE` is the rule, the
    pre-check names it); an unknown purchase, line, method, supplier or party is `404`; an
    amount that is not whole cents is `400`.

S11 **Scope ends at the API.** No payables screen, no supplier statement, and `credit` stays
    `is_active = false`, following `v4-receivables.md` S11 and the owner's recorded decision
    (`v4-receivables.md` L4: *"Solo backend, como S3 y S4"*).

    **Fixed since, in PR #89** — it was recorded here first, which is the point of recording
    things. A document with no method rendered an empty `payment_method_name` cell in both
    ledgers, and the TypeScript type said `string` while the server answered `null`. Both are
    corrected by one shared `methodLabel`, because the two ledgers render the same column from
    the same shape and a rule written twice drifts.

S12 **One migration for the three tables, because a re-cut is what went wrong.**
    `0022` re-cut `obligation_adjustments_within_amount` to add the settlement half and
    silently lost the `FOR UPDATE` `0021` had added (`R3-004`, CRITICAL). `payables`,
    `payables_adjustments` and `payable_settlements` therefore ship in **one** file, written
    once, with both terms present from the first line. `CREATE OR REPLACE` is not additive;
    not needing to replace is cheaper than remembering.

S13 **`0021` and `0022` are not modified at all.** No `ALTER` on `obligations` or
    `settlements`, no `CREATE OR REPLACE` of either ceiling function, and
    `money_entries_kind_known` is untouched — a payable writes no entry, so there is no new
    kind to accept. This is what the decision in `v4-settlement.md` L25 buys, and it is
    testable: the migration adds no statement whose target is an object `0021` or `0022`
    created.

## Tasks

| ID | Spec | Slice | Route | Risk | Commit |
| --- | --- | --- | --- | --- | --- |
| T7.1 | S1, S2, S4, S5, S6, S12, S13 | S6b — the document and the schema | `0023_payables.sql`: the three tables, their ceilings and append-only guards, nullable `purchases.payment_method_id` and `supplier_returns.payment_method_id`, the claim triggers | item 1, item 3 | — |
| T7.2 | S1, S2, S3 | S6b — a purchase that owes | `purchases.rs`: the `"on_account"` destination, the payable write, the `LEFT JOIN` read paths, the refusal of the one-off supplier on account; `Destination` moves out of `sales.rs` into a shared module | item 1 | — |
| T7.3 | S6, S7, S10 | S6b — paying a supplier | `POST /api/payable-settlements`, the three-way check (exists, went out, same supplier), the party-sent ceiling, the two ceilings pre-checked | item 1, item 4 | — |
| T7.4 | S8, S9, S10 | S6b — the return that owes | the override in `create_supplier_return`, the inherited NULL path writing a `payables_adjustment`, the rail budget, the combined-ceiling pre-check | item 1 | — |
| T7.5 | all | S6b — close | `docs/en/architecture.md` route table and invariants, the slice's entry in `v4-settlement.md`, full `make test` and `make lint` | — | — |

Order is dependency: T7.1 before everything (nothing writes a payable the schema cannot hold),
T7.2 before T7.3 and T7.4 (a payable has to exist before anything settles or lowers it),
T7.4 last of the code (its guard needs `payable_settlements` to have something to sum).

Each of T7.1, T7.2, T7.3 and T7.4 carries **item 1**, so each is its own commit.

## Log

L1 (owner, verbatim): "sigamos con S6b"

L2 (owner, decision): **the supplier side gets `payables`, `payables_adjustments` and
`payable_settlements` of its own** — not `obligations`/`settlements` extended. Asked which,
because `v4-settlement.md` S5/S6 say "extend" and the T7 row says "payables", the owner chose
the second. Recorded in `v4-settlement.md` as L25, where the two spec paragraphs are
**annotated rather than rewritten**, per this repository's append-only rule for ODD logs.

L3 **Why the choice is worth its cost, in the terms the last two reviews punished.** Reaching
the supplier side through `obligations` takes four edits to shipped surgery: two closed-set
`ALTER`s (`obligations_source_known`, `obligations_adjustments_kind_known`) and two whole
`CREATE OR REPLACE` re-cuts (`settlement_within_both_ceilings`,
`obligation_adjustments_within_amount`). Both operations have already failed here — `0022`'s
re-cut silently dropped a `FOR UPDATE` and its `CHECK` rewrite silently dropped `annulment` —
and `v4-receivables.md` L29 names the pattern: *"a guard keyed on the wrong axis"*. A new
table cannot lose a clause it never had.

L4 **What the decision costs, named so it is not read as free.** The ceiling is written twice:
`settlement_within_both_ceilings` for the customer side and a payable mirror. Same rule, two
functions, so they must move together. `obligations.party_kind = 'supplier'` stays declared and
unused — removing it would be the rewrite being avoided — and `obligations_source_known` still
refuses `'purchase'`, so a payable cannot become an obligation by accident. Both are recorded
as follow-ups rather than as tidiness for a later session to perform.

L5 **The supplier-side balance has two terms, not three.** `lock_party_balance` sums
`in − settlements − out`; the payable mirror sums `out − payable_settlements`. The third term
is structurally zero because `0022`'s `payments_direction_matches_party` makes `in` from a
supplier inexpressible. Recorded because a reader who sees two terms beside a three-term
original will otherwise assume one was forgotten.

L6 **The UI is not touched, and the consequence is written down instead of shipped silently**
(S11). Both ledgers rendered `payment_method_name` for a document that has none, and
`frontend/src/lib/api.ts` typed it `string`. That was one decision for two screens (sale on
account from S6a, purchase on account from here) and it is now made, in **PR #89**: an empty
cell became "A cuenta" / "On account", and the type became `string | null`, which TypeScript
accepted with no call-site change — the proof that nothing was reading it as a string.

L7 **Measured baseline before any code**: `make test-backend` **127 passed** on `main` at
`2e6f7be`, the development database at migration `0022`, worktree clean. Every number this
document records afterwards is against that.

L8 **S7 claimed a guard that arithmetic makes redundant, and the neutering found it.** Two
handler guards were proven to bite (`party`, `whole cents`, the two ceilings) and two were not.
One of the two — the handler's `direction != "out"` — is **unreachable for any schema-legal
payment**: `payments_direction_matches_party` makes every `in` payment a customer's, and a
customer's `party_kind` never equals a supplier payable's, so the party check answers first. It
is kept as defence in depth (the schema is the rule and the handler agrees with it, exactly as
`create_settlement` keeps its own) and it is **not** claimed as tested — the direction rule is
proven below the route, in the trigger test, where direction is checked before party.

The other is S7's party-sent ceiling, and there the finding is about the **design**, not the
test: the mirror must be `Σ(payments out) − Σ(payable_settlements)`, which is identically the sum
of the per-payment remainders. So on the supplier side the per-payment ceiling already implies
it, and the helper cannot refuse anything the ceiling above it would accept. Measured twice —
neutered (all green) and then computed both ways on one fixture (`30.000000`, `30.000000`) — and
recorded in S7 and in the test rather than left as a guard that reads as coverage and is not.

**Why the customer side is different, in one line**, because the symmetry is the trap: there
`Σ(settlements)` is *not* recoverable from the per-payment remainders, because a refund takes
money out without touching a settlement. That asymmetry is the whole reason
`lock_party_balance` needed three terms and this one needs two — and why "the mirror" was the
wrong intuition for the ceiling even though it was the right one for the lock order.

## Verification log

V1 **T7.1, `0023_payables.sql`, measured against a fresh database before any route existed.**
The migration applies clean on an empty database (23/23). Every guard was then measured by
hand with `psql`, because **a trigger no route reaches yet is only proven by reaching it**:

| # | attempt | answer |
| --- | --- | --- |
| 1 | purchase on account + its payable, one transaction | **accepted** — the pair is the rule, not a violation |
| 2 | purchase on account with **no** payable | refused: *"purchase … says it moved no money, but 0 payable(s) name it; a purchase on account has exactly one"* |
| 3 | purchase that names a method **with** a payable | refused: *"names payment method …; money moved or debt arose, never both"* |
| 4 | payable naming a **cash** purchase | refused: *"names payment method …: money moved, so it owes nothing"* |
| 5 | payable naming a purchase that does not exist | refused: *"payable names purchase … which does not exist"* |
| 6 | payable with `source_document_kind = 'sale'` | refused by `payables_source_known` |
| 7 | payable attributed to the **wrong supplier**, one transaction | refused: *"must belong to that purchase's supplier …, not supplier …"* |
| 8 | apply 70 of a payment of 60 | refused: *"is for 60.000000, with 0.000000 already applied; 0.000000 + 70.000000 would apply more than was paid"* |
| 9 | apply exactly 60 | **accepted** |
| 10 | settle a supplier payable with a customer `in` payment | refused: *"is not money going out; only money we sent can pay what we owe"* |
| 11 | lower a payable past zero | refused: *"is for 100.000000, with 40.000000 already settled; 40.000000 + 10.000000 would lower it past zero"* |
| 12 | lower it by exactly the room left | **accepted** |
| 13 | `UPDATE payables`, `TRUNCATE payable_settlements` | refused: *"are append-only: corrections are compensating rows, not edits"* |
| 14 | supplier return claiming it moved no money on a **paid** purchase | refused: *"says it moved no money, but its purchase … names payment method …"* |
| 15 | supplier return claiming it moved no money with **no** adjustment | refused: *"moves no money, so it must lower exactly one payable; 0 found"* |
| 16 | the correct inherited path — no method, one adjustment | **accepted**, and the payable dropped by it |

V1b **A probe artifact that looked like a defect, and was not.** Running the acceptance case as
two standalone `psql` statements answered *"…says it moved no money, but 0 payable(s) name
it"* on the second one. That is **autocommit**, not the trigger: the purchase committed alone,
which is precisely a purchase on account with no payable. Inside one explicit transaction the
pair is accepted, and the wrong-party case in the same shape is refused with the party named.
Recorded because the first reading of it was a false alarm, and the next session would have
the same one.

V1c **What `0023` deliberately does *not* enforce, and why the split is the same one `0021`
chose.** A supplier return naming a real method on an **unpaid** purchase is accepted by the
schema when it writes its entry: the trigger's job is the two-state rule (`S8`), and "did this
rail actually receive the money" is `S9` — a **budget**, kept in the handler exactly as
`v4-receivables.md` S7 keeps the `L107` guard in `create_credit_note` rather than in `0021`.
The consequence is stated so it is not read as an omission: **until T7.4 lands, an explicit
supplier return on an unpaid purchase can take money out of a rail that never paid it.** That
window is the reason T7.4 is a task of its own and not a detail of this one.

V1d **`S13` is asserted inside the migration itself**, not only believed: the `DO` block reads
`prosrc` of `settlement_within_both_ceilings` and `obligation_adjustments_within_amount` and
refuses to continue if either body no longer carries the clause those migrations wrote. It is
a weak assertion — it greps for a message string — and it is the honest one that can be made
from a migration: it catches the specific regression that has happened twice here, which is a
`CREATE OR REPLACE` that quietly became the only copy.

V2 **T7.2, the purchase that owes.** `make test-backend` **135** (was 127; +8),
`make test-unit` 52, `make test-e2e` 23, `cargo fmt --check` and `clippy -D warnings` clean,
`svelte-check` 0 errors. The eight new tests are `v4_s6b_*`.

V2b **A defect in `sales.rs` that S6a shipped and this slice found: the documented 400 was a
422.** `NewSale::payment_method_id` was deserialized with a custom function that returned a
serde error for an explicit `null` or an unparseable string, and **serde's failure reaches the
client through axum's `Json` extractor as `422`**, not the `400` the field's own doc comment
promised and `docs/en/architecture.md`'s decoding note implies. Nothing failed, because no test
had ever sent an explicit null to `/api/sales`. The shared definition reads the field raw
(`destination::keep_present`) and judges it in the handler, which is what makes the refusal the
status its contract states — and it is a **behaviour change on an existing route**, so
`v4_s6b_a_sale_refuses_an_explicit_null_or_a_garbage_destination_as_a_bad_request` pins both
the refusal and that the two refused sales wrote nothing.

V2c **The return path is refused rather than half-written, deliberately.**
`create_supplier_return` now answers `409` on a purchase with no method, before any write,
because the inherited `NULL` path it needs is T7.4's. Left unrefused it would have inserted the
header and then been refused at COMMIT by `0023`'s deferred trigger — a 500 for an ordinary
operation, which is `R1-004` of `review-336e2618582310f9` exactly.
`v4_s6b_a_return_of_an_on_account_purchase_is_refused_until_it_can_lower_the_debt` pins the
refusal, that nothing was written, and that the debt still stands. **This closes the window V1c
opened** and it is the reason T7.4 is now purely additive.

V3 **T7.3, paying a supplier.** `make test-backend` **142** (was 135; +7), `make test-unit` 52,
`make test-e2e` 23, `make check` on the frontend 232, `make lint` clean and `make sqlx-check`
in sync. The seven new tests are `v4_s6b_*` on the settlement route.

The route's own guards, each proven by neutering it and watching its test go red:

| guard neutered | the test that failed | what it said |
| --- | --- | --- |
| the party check | `v4_s6b_only_money_we_sent_to_that_supplier_can_retire_what_we_owe` | refused `201` instead of `400` |
| whole cents | `v4_s6b_the_new_endpoint_refuses_an_amount_that_is_not_whole_cents` | `1.005` reached the CHECK and came back `500` |
| the payable ceiling | `v4_s6b_both_ceilings_refuse_and_each_names_its_own` | the over-settlement was accepted |
| the payment ceiling | the same test, **only after a second unapplied payment was added** | `500`, not `409` — see V3b |

V3b **Two guards did not bite, and each taught something different.** Neutering the handler's
`direction != "out"` left its test green, and so did neutering `body.amount > sent_available`.
Both are recorded above as `L8` and neither is claimed as coverage. The first is unreachable for
any schema-legal payment (the party check fires first, because `in` is always a customer's); the
second is arithmetically implied by the per-payment ceiling. The payment ceiling's own test was
also weak and is fixed: without a second, fully unapplied payment the **party** ceiling answered
the same `409`, so neutering the payment ceiling stayed green. The test now sends 500 more that
is unapplied, which is what separates the two ceilings from each other.

V3c A fixture bug of mine, found by the suite and not by reading it: my `purchase_on_account`
helper created a **new supplier per call**, so a test that meant to build two payables for one
supplier built one for each and the party check correctly answered `400`. The helper now has a
`_for` variant that takes the supplier, and the two call sites that need one supplier use it.

V2d **What the writer changed and why it is right.** `Destination` and its grammar moved to
`backend/src/models/destination.rs` with two unit tests of its own; both routes call it. The
`LEFT JOIN` keeps a purchase on account in the create response and the list, and `payment_method_id`
/`payment_method_name` are `Option` on the wire — the same shape `models/sale.rs` already
carried, so a client reads one contract for both ledgers.

V4 **T7.4, the return that owes, and T7.5, the close.** `make test` green: 52 unit + 232
frontend + **146** backend (was 142) + 23 e2e. `make lint` clean — and it was **not**, once: the
first `make lint` run failed on `cargo fmt --check`, which is the exact step whose absence cost
this repository a CI round in S6a (`v4-receivables.md`, "`make lint` was not the lint CI runs").
`make sqlx-check` in sync.

V4b **The sign bug this task found, and it is the one `0014` warns about.** The rail budget's
first version summed the purchase's ledger entry as it stands. A purchase's entry is `-total`
(money went out), so the sum was `-200`, `arrived - left` was negative, and **the rail that had
actually paid was refused with 409**. The fix is `-COALESCE(SUM(...), 0)`, and the comment beside
it says why the sale side does not need the negation: a sale's entry is `+total`, which is the
whole reason a single sign convention has to be declared once instead of re-derived per call site.

V4c **A design mistake of mine, caught by nine pre-existing tests.** I first read "omitted
`payment_method_id`" as "this return moves no money", and refused it on any paid purchase — so
every v2.5 and v3 supplier-return test turned `400`. The correct reading, and the one `0021`
established for credit notes, is that **omitted means inherit**: the return takes the purchase's
method, whatever it is. On a paid purchase that is a real rail and money moves back, exactly as
before this slice; on an unpaid one it is `NULL` and the debt drops. That makes `S8`'s *"a return
may be NULL only when its purchase is NULL"* a property of the result rather than a refusal to
write — and a rule the route **cannot violate**, which is why its test now lives below the route
(`v4_s6b_the_database_refuses_a_null_return_on_a_paid_purchase`) instead of asserting a status the
handler could only get wrong.

V4d **Three tests of mine passed for the wrong reason, found by neutering, and all three fixed.**
This is the fourth time in this feature and the pattern is now explicit:

| test | what it appeared to measure | what it actually measured |
| --- | --- | --- |
| the combined-ceiling over-return | the handler's pre-check | a single-unit purchase made the whole debt the return's total, so `0023`'s trigger answered. Now the fixture buys **two** units and returns one |
| the paid-purchase NULL | the handler's refusal | the handler no longer refuses it — moved below the route, where the trigger is the only guard |
| the over-return, after the fix | the pre-check alone | the `P0001` → `409` mapping. **Both** are needed: neutering the pre-check alone stays green, neutering both gives `500`. Recorded as deliberate redundancy, for `v4-settlement.md` V9c's reason |

V4e **The guards that do bite, each by neutering:** the rail budget (`409` → accepted, driving a
rail negative), the debt-account refusal (`400` → `409`), the party check on the settlement route,
whole cents, and both settlement ceilings. That is five distinct behaviours, each shown red with
its guard removed and green with it restored.

V4f **What is deliberately not done, restated from `S11`:** no payables screen, no supplier
statement, and `credit` stays inactive. `SupplierReturn` gains `payment_method_id` on the wire
(because a client must be able to tell a return that moved money from one that lowered a debt) but
**no resolved `payment_method_name`**, unlike `Sale` and `Purchase`: a return is nested inside the
purchase that already carries the rail, so a second copy would be the same fact twice.

L9 (owner, verbatim): "Pr sin review" — **the slice is delivered without a native review**, and
the reason is measured rather than convenient. The candidate is `origin/main..HEAD`: 27 paths,
3556 authored lines. Two lanes over it are refused at START with `lens_context_budget_exceeded`,
`mutation_outcome: not_started`, `lineage_created: false` — **no authority was created, so nothing
was abandoned and nothing has to be repaired.** The provider's own message says retrying that
exact candidate cannot succeed and to split it into smaller commits.

Splitting it after the fact requires rewriting history, and the safety policy blocks that. The
owner chose delivery over the rewrite. **What that costs is stated so a later session does not
read it as an oversight: this slice got no outside view.** In S6a the outside view found four
defects that the slice's own tests could not see (`v4-receivables.md` L28), and three of the four
defects found in this one were caught by *neutering guards*, not by review — so the discipline
substituted for review here, and the human reviewer carries the rest.

L10 **The method lesson, and it cost a whole session.** I proposed six commits by task, then
produced one, then spent four attempts slicing it afterwards — and each attempt made the file
larger: `purchases.rs` came back duplicated twice, one stash pop left conflict markers, and two
files were truncated to zero bytes by a redirect to a path that did not exist. Nothing was lost
(the work was recoverable from the commit and from `git fsck --lost-found`), but the split never
happened.

Three things were wrong, and they are worth more than this slice:

1. **A migration that makes a column nullable cannot be its own commit.** `sqlx::query_as!`
   infers nullability from the live schema, so `0023` alone does not compile until the models
   change. T7.1 and T7.2 are one commit by construction, not by preference — my six-commit plan was
   unbuildable from the first row.
2. **A review candidate is a range of commits.** It cannot be carved out of one commit without
   rewriting history, so "I will split it later" is not a plan; it is a wish. The division has to
   be written in from the first commit, which means deciding it *before* the first line of code,
   against the measured line counts of each file.
3. **Measure the candidate before committing, not after.** The one measurement that would have
   saved the session — `git diff --stat` of each intended commit against the provider's budget —
   takes a second and I did it only after the fact, four times.

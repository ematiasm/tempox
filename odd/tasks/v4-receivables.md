# v4-receivables — what we are owed, and the money that settles it

Slice **S6a / T6** of `odd/tasks/v4-settlement.md`, split into its own document because it
is the slice where an obligation stops being a fake account. **State: T6.1-T6.4 implemented,
green (119 backend tests), in six commits on `feat/v4-annulment`. `0021` was reviewed and
approved (`review-14d05c7f7f92e0bf`) after its BLOCKER was fixed; the correction `b0735f4` was
approved earlier (`review-1becd57b99cb3a1f`); the slice as a whole is not, because it exceeds
the reviewer context budget and had to be split to be reviewed at all.** S1, S3, S4 and S5 are
merged to `main` (`e081077`); S2 is deferred to v5; S6b and S7 are still ahead.

The state that must hold when this slice closes, and it is one sentence:

```
a credit sale writes an obligation and no money,
and a refund of a sale nobody paid is refused.
```

## Specs

S1 **An obligation is not a ledger row, and neither is the sale that creates it.**
   A sale on account writes `obligations` and **zero** `money_entries` rows. Money appears
   in the book only when a collection settles the obligation.

   This is `v4-settlement.md` S5 restated as the reason this slice exists: the defect is
   that `credit` does three jobs at once. Two records of one debt — a ledger row naming a
   fake account *and* an obligation row — is the same defect with an extra table.

S2 **`sales.payment_method_id` becomes nullable, and the null is a claim, not a free
   pass.** The two states are enforced together by a constraint trigger, exactly as
   `v4-settlement.md` S4 declares:

   ```
   payment_method_id IS NULL      →  exactly one obligation names this sale
   payment_method_id IS NOT NULL  →  zero obligations, and at least one ledger entry
   ```

   **Moved here from T8, and the reason is measured.** `0017` put a plain FK from
   `money_entries.payment_method_id` to `payment_methods.id`, `0012` refuses `DELETE` on
   both tables, and `0013`'s `payment_methods_account_guard` fires on any real change of
   `account_id`. If a credit sale wrote `credit +total` into the book, S7 would have to
   delete rows from an append-only table to keep its promise. It does not have to today:
   nothing writes a `credit` entry, because the method is seeded inactive and no document
   names it. **S7 stays a clean delete only while no ledger row ever names `credit`.**

S3 **Nullability arrives with the document kind that needs it: `sales` and `credit_notes`
   in this slice, `purchases` and `supplier_returns` in T7.** `v4-settlement.md` S4 names
   `sales` and `purchases`; `L7` of that document established that all four carry the column
   and that it means two different things across them.

   **They are still split, and the reason is S4 of that same document:** a null is a claim,
   and a claim needs a rule or it is an arbitrary null. The rule this slice can enforce
   exists only on the sale→obligation side, because `obligations` is the document that
   makes "this moved no money" checkable. A nullable `purchases` opened here would be a
   hole with no rule behind it until `payables` exists in T7 — so the column stays `NOT
   NULL` until the slice that can guard it.

   **The credit note's null is what makes `0014`'s `-total` impossible to write against a
   debt nobody paid.** A credit note on an unpaid sale inherits the sale's method, which is
   now NULL, so it writes no entry — and the debt drops by the note's total. That is the
   honest version of what the current code fakes by routing the note through `credit`.

   **A note may be NULL only when the sale it corrects is NULL.** A note on a paid sale
   that moved no money would be a credit balance with no payment behind it — money from
   nowhere. The refund-a-balance case is S6's, and it is a `payments` row, not a note.

S4 **A collection is one payment with a direction, and it is its own document.**
   `payments(id, party_kind, party_id, payment_method_id NOT NULL, total > 0, direction ∈
   {in, out}, occurred_at, created_at, created_by)`. `in` is a customer paying us. A
   payment writes **one** entry — positive for `in` — and never names a fake account.

   `v4-settlement.md` S7. There is **no mixed payment**: a `payments` row carries one
   method, and the existing limitation (`v3-accounts.md` S22) stays declared rather than
   silently half-fixed.

S5 **Settlement is a table, and both of its guards are sums over rows.**

   `settlements(obligation_id, payment_id, amount > 0, created_at, created_by)`, with

   - `SUM(settlements.amount) ≤ obligations.amount`, per obligation — an obligation may be
     paid in parts;
   - `SUM(settlements.amount) ≤ payments.total`, per payment — a collection may cover more
     than one debt, and the excess is the customer's credit balance.

   `v4-settlement.md` S6 and L18 (owner, verbatim): *"un cobro puede pagar varias
   obligaciones. como tambien una venta/compra puede quedar parcialmente pagada."*

   **Partial is not a state.** It is `SUM(settlements) < amount`, and no column says
   `partial` or `unpaid`.

S6 **An unapplied payment is the customer's credit balance, and there is no table for it.**
   It is a `payments` row whose `SUM(settlements) < total`. Consuming it is a `settlements`
   row naming the same payment and a newer obligation — **manual**, never automatic
   (`v4-settlement.md` S8, `L22`: *"si, vamos por manual"*). The balance is derived, never
   stored, for the same reason a balance is a sum over `money_movements`: a subtraction of
   stored rows cannot drift.

S7 **The `L107` hole closes on the explicit-method path, and the inherited path is where
the debt drops.**

   A credit note that names a method other than the sale's, on a sale that was owed,
   requires `SUM(settlements) ≥` the note's total. Without enough settlement it is refused.

   The inherited path stays free, because it now does something different: it lowers the
   obligation and moves no money. The two paths are no longer the same statement with a
   different account, and that asymmetry is the whole fix.

S8 **Lowering what is owed is its own document: `obligations_adjustments`.**

   `obligations_adjustments(id, obligation_id, amount > 0, document_kind ∈ {credit_note,
   cancellation}, document_id, created_at, created_by)`. A credit note that inherits the
   sale's (`NULL`) method writes one; so does a cancellation.

   **Not a mutable `obligations.amount`.** The writer-flag pattern exists
   (`sale_items.reversed_quantity`, `0019`) and it cost a CRITICAL finding (`R3-002`,
   `v4-settlement.md` V7). Here the honest shape is available, so the flag is not needed:
   the debt drops through a row.

   The outstanding figure is therefore `obligations.amount - SUM(settlements) -
   SUM(obligations_adjustments)`, and a partial drop is the normal case rather than a
   special one.

S9 **The invoice's numbers arrive in the edit shape `v1.5-credit-note.md` established.**
   A sale's outstanding figure and a payment's unapplied figure are derived in SQL and
   serialized through `crate::money::serialize_amount`, so the API says `"100.00"` and not
   `100.000000`. The unapplied figure is S6's; the outstanding one lands with its first
   consumer in T6.4.

S10 **The three refusals keep their existing exception channel.**
    `payments.payment_method_id` may not resolve to the debt account (`BAD_REQUEST`),
    a `settlements` row may not exceed either ceiling (`CONFLICT`), and a second obligation
    on the same sale is `CONFLICT` (the `UNIQUE` enforces it; the pre-check names it). The
    handler pre-checks give the caller a status it can act on; the triggers are the rule.
    Both layers exist for the reason `v4-settlement.md` V9c records — only the handler's is
    reachable over HTTP, only the trigger's over `psql`.

S11 **Scope ends at the API.** No collection screen, no balance-applying button, and
    `credit` stays `is_active = false` so the counter cannot sell on account. This follows
    S3 and S4, which shipped tables plus routes and no UI, and it is the owner's decision.

## Tasks

| ID | Spec | Slice | Route | Risk | Commit |
| --- | --- | --- | --- | --- | --- |
| T6.1 | S1, S2, S3, S7, S8 | S6a — the four documents stop faking a wallet | `0021`: `obligations`, `obligations_adjustments`, nullable `payment_method_id` on `sales` and `credit_notes`, the two-state constraint trigger, sale on account in `sales.rs`, a credit note that moves no money | item 1, item 3 | `d520b11` |
| T6.2 | S4, S5 | S6a — a collection is a document | `0022`: `payments`, `settlements`, their ceilings and append-only guards, `POST /api/payments` and `POST /api/settlements` | item 1 | this commit |
| T6.3 | S6 | S6a — a balance nobody assigned | the derived unapplied figure, settlement against a newer obligation from a payment with free amount, the manual rule stated in one place | item 1 | this commit |
| T6.4 | S7, S9 | S6a — the `L107` hole | the explicit-method guard on `create_credit_note`, the `v3-accounts.md` L107 scenario as a test | item 1 | this commit |

Order is dependency: T6.1 before everything (nothing settles an obligation that cannot
exist), T6.2 before T6.3 (a balance is a payment with free amount), T6.4 last (it needs
`SUM(settlements)` to have something to sum).

Each of T6.1 and T6.2 is its own commit because both carry **item 1**.

## Log

L1 (owner, verbatim): "arranca con S6a"

L2 **Two decisions taken before any code, because the document did not settle them.**

L3 **Question 1, and the measured answer.** `v4-settlement.md` S5 says an obligation writes
no ledger entry and neither does the sale that creates it — but `0014` writes `+total` for
every sale and `0017` made that a row. Without S4's nullability, a credit sale would write
`credit +100.00` **and** an obligation of 100.00, two records of one debt.

The owner's answer, verbatim: *"credit desaparece en el slice 7, checkea por las dudas"*.
Checked, and it is **true with a condition that this slice must protect**:

- `0012` refuses `DELETE` and `TRUNCATE` on `accounts` and `payment_methods`.
- `0017` put `payment_method_id UUID NOT NULL REFERENCES payment_methods (id)` — no
  `ON DELETE`, so the FK alone would block a delete once a row names the method.
- `0013`'s `payment_methods_account_guard` fires on any real change of `account_id` when
  any of the four document tables names the method.
- `money_entries` holds **zero** rows naming `…0006` today: the method is seeded
  `is_active = false` (`0012`) and `sales.rs` only reaches it when a caller names it
  explicitly.

So S7 is a clean delete **now**, and stops being one the moment the book records a credit
row. S4 therefore moves into this slice: it is what keeps S7 cheap.

L4 **Question 2, the owner's answer.** *"Solo backend, como S3 y S4"*. No collection
screen, and `credit` stays inactive, so a sale on account is reachable by API only. Recorded
as S10 rather than left implicit, because a later session reading "sale on account" would
otherwise expect the counter to offer it.

L5 **One correction to the plan the question exposed.** `v4-settlement.md` S4 scopes the
nullable column to `sales` and `purchases`. `L7` of that document already established that
all four document tables carry `payment_method_id` and that the column means two different
things across them.

L6 **S3 first said all four documents, and that was wrong — corrected before any code.**
The first version widened the nullable column to `purchases` and `supplier_returns` on the
strength of `L7`'s asymmetry argument. It does not survive S4's own rule: **a null is a
claim, and a claim needs a rule.** This slice can enforce the rule on the sale side only,
because `obligations` is what makes "this moved no money" checkable. A nullable
`purchases.payment_method_id` written here would be an unguarded hole until `payables`
arrives in T7.

So nullability now arrives with the document kind that needs it: `sales` and `credit_notes`
in T6.1, `purchases` and `supplier_returns` in T7, each with its guard in the same commit.
The asymmetry `L7` names is not recreated by this, because the asymmetry was about the
**meaning** of the column, and that is now enforced where each meaning lives.

L7 **A fourth table, and why not a mutable column.** Lowering what a credit note owes could
be done by mutating `obligations.amount` behind a `tpox.*_writer` flag — the pattern
`sale_items.reversed_quantity` established in `0019`. It is refused here: that flag was the
subject of `R3-002`, a CRITICAL finding in the review of `4ffb95e` (`v4-settlement.md` V7),
and a second instance of it is a sharp edge bought for nothing. The debt drops through an
`obligation_adjustments` row instead: append-only, no flag, and the balance stays
`amount - SUM(settlements) - SUM(adjustments)`, which is a subtraction of stored rows and
cannot drift.

## Verification log

V1 **T6.1, the schema and the write path.** `make test-backend` 114 (was 107; +7). The two
guards proven to bite by neutering each in turn:

| guard neutered | the test that failed | what it said |
| --- | --- | --- |
| `sale_payment_method_is_claimed` (`0021`) | `v4_s6a_a_null_method_is_never_an_arbitrary_null` | *"a sale claiming to have moved no money with no obligation must be refused"* |
| `money_entry_never_names_the_debt_account` (`0021`) | `v4_s6a_the_book_refuses_to_name_the_debt_account_again` | *"a ledger row naming the debt account must be refused"* |

V1b **Seven existing tests failed, and all seven were right.** `0021` made "a sale that
names a method wrote an entry" a rule, so every raw fixture that inserted a sale header
alone was refused. Two of the seven were not fixture drift but a **changed mechanism**, and
they needed rewriting rather than repair:

| test | why it failed | outcome |
| --- | --- | --- |
| `v3_t4_a_credit_sale_must_name_a_customer` | sold on the `credit` method | now sells on `on_account`; asserts the obligation exists once, and none for the refused sale |
| `v3_t5_a_credit_note_inherits_the_sales_method_by_default` | asserted the `credit` account went 90 to 0 | now asserts outstanding 90 to 0, cash untouched, and the note's method NULL |
| the other five | inserted a header with no entry | migrated to `insert_paid_sale`, a helper that writes the pair in one transaction |

V1c **The sentinel exists because serde cannot tell omission from null.** `Option<Uuid>`
collapses `"payment_method_id": null` and an absent key, so null would have to mean either
"the till" — silently turning a debt into a cash sale — or "on account" — silently turning
every older client's omitted field into a debt. `"on_account"` is the only spelling where
all three inputs stay distinguishable, and an explicit null is a `BAD_REQUEST` that says so.

V1d **Two `LEFT JOIN`s, and they are the difference between a sale appearing and vanishing.**
`sales.rs` read the persisted sale and the day's list through `INNER JOIN payment_methods`.
With a nullable method, an inner join drops a sale on account from both — a missing document
nobody is told about. sqlx then inferred the joined name as non-nullable, so the query names
the nullability explicitly (`CASE WHEN s.payment_method_id IS NULL THEN NULL`) instead of a
`!` override that would have compiled and lied.

V2 **T6.2 and T6.3, the collection side.** `0022` applies on a database with data; `make
test-backend` 114. **Both ceilings were measured before any route existed**, against a
migrated database by hand, and each raised its own message:

| attempt | answer |
| --- | --- |
| apply 70 of a payment of 60 | *"payment ... is for 60.000000, with 0.000000 already applied; 0.000000 + 70.000000 would apply more than was paid"* |
| apply exactly 60 | accepted |
| apply 50 to a debt with 40 left | *"obligation ... is for 100.000000 ... 60.000000 + 50.000000 would settle more than is owed"* |
| apply 40 | accepted; 60 left unapplied |
| adjust a fully settled debt by 10 | *"obligation ... is for 100.000000, with 100.000000 already settled ... would lower it past zero"* |

V2b **A test of mine was wrong and the schema corrected it.**
`v4_s6a_a_payment_may_cover_several_debts_and_a_debt_may_be_paid_in_parts` first wrote 40
and then 60 against the *same* `(obligation, payment)` pair, and `0022`'s
`UNIQUE (obligation_id, payment_id)` refused the second row. It was right to: two rows from
one payment to one debt sum to exactly what one row of 100 says, so allowing them stores one
fact twice. "Paid in parts" means across **payments**, which is what the test now expresses —
and the leftover balance it then measured (140 unapplied on the first payment) is exactly
`S6`'s credit balance, so the corrected test demonstrates the spec instead of contradicting
it.

V3 **T6.4, `L107` closed.** The test was written first and observed failing against the
implemented routes: a refund naming the till on a sale nobody had paid answered **201** and
took the note's total out of an account that never received it. With the guard, it answers
**409**, the till stays at 0.00 and the debt stays at 100. Neutering the guard (`&& false`)
turns it red again with the original message. The legitimate half is asserted in the same
test: after 100 has really been collected and settled, the same refund is accepted and the
till goes 100 to 0.

V3b **The guard asks "did this much arrive", not "was it settled", and the difference is a
real case.** A sale paid outright has no obligation at all, so its `SUM(settlements)` is zero
— but its money did arrive, at the sale. Reading only settlements would have refused every
refund of a cash sale, which the suite would have caught and which the code now states
explicitly.

V4 **One regression I introduced and did not ship.** The first version of `0022` re-wrote
`money_entries_kind_known` with the `0017` set plus `payment` and **silently dropped
`annulment`**, which `0020` had added. Every annulment answered 500. Two `v4_an_annulment_*`
tests caught it on a full `make test`, not on the focused runs. The list is now complete and
the `DO` block asserts every kind the earlier migrations introduced is still accepted, so the
third copy of that `CHECK` cannot fail the same way. **This is `0020`'s own lesson repeating:
copying a closed set's members by hand is how the set loses one.**

V4b **A flake, reported rather than dismissed.** The first full `make test` run failed one
e2e purchase spec on a product-picker timeout, then passed 23/23 on three subsequent runs
including two full `make test` runs. Nothing in this slice touches the frontend or the
picker, and the failure is a 5 s timeout on a listbox, but it is recorded because an
intermittent green is not the same as a green.

## The review of the first candidate, and the five severe findings it measured

L8 **Review `review-336e2618582310f9`, tier high, four lenses, 23 paths, 3540 lines.** Five
CRITICAL findings, all `causal_disposition: introduced`, all corroborated by the refuter.
**The worst of them was this slice's own defect, in this slice's own new route.**

L9 **`R1-001` / `R3-001` — a refund from nowhere.** `create_payment` accepted any positive
`direction = "out"` total. Measured against a live server before the fix: a customer with no
collections at all, `POST /api/payments {"total": "500.00", "direction": "out"}`, answered
**201**, and the till read **−500.00**. This is `v3-accounts.md` L107 with a different verb:
the shop pays out money nobody ever handed it. The fix is `lock_party_balance`, which computes
`payments in − settlements − payments out` with the party's payment rows locked, and the
handler answers 409 above it.

L10 **`R1-002` / `R3-002` — money that left the till settling a debt.** `create_settlement`
read `payments.total` and never `payments.direction` or its party. Measured: a 100.00
on-account sale, a 100.00 outbound "refund", then settling the debt with that same refund
answered **201** and left the debt at **0** while the cash had only gone out. The customer's
money was resolved twice — once as a debt, once as cash in their hand. Three checks now: the
payment exists (404), it is money that came **in** (400), and it belongs to the **same party**
the debt belongs to (400).

L11 **`R1-003` / `R3-003` — the L107 guard only looked at what arrived, never at what already
left.** The gate compared each note against `SUM(settlements)` and did not subtract the
explicit notes already issued, so several notes each passed alone. Measured: a 200.00
on-account sale settled for 100.00 admitted **two** 100.00 cash notes, and the till ended at
**−800.00** on a day 100.00 came in. The guard now spends a budget: `arrived − SUM(explicit
credit_note entries)`.

L12 **`R1-004` / `R4-001` — a reachable ordinary operation answering 500.** The inherited
no-method path inserted the whole note total as an adjustment; when the obligation had no room
left, `0022`'s combined ceiling raised at COMMIT and the caller got a **500**. Returning goods
on an already-settled on-account sale did exactly that. The handler now pre-checks the same
combined ceiling the settlement route pre-checks, and answers 409.

L13 **`R2-001` — a comment that overstated, and the CHECK it hid.** "Shape first" claimed every
database CHECK was pre-checked while whole-cents was not: three decimals reached
`payments_whole_cents` and came back as a **500**. Both new endpoints now validate
`is_whole_cents` and answer 400. A comment that overstates is worse than none, because it is
the reason nobody looked.

L14 **Two findings I record and do not fix here, both deliberately.** `R1-005` is the repository-
wide unauthenticated posture: every write route is open, sales and credit notes included, and
this slice gives that posture higher stakes. Closing it is an authentication design, not a
correction, and pretending otherwise in a bounded correction would be worse than naming it.
`R2-002` and `R2-003` are diagnostic-wording suggestions (a message that prints two of three
summands; count variables named as if they were amounts); they change no behaviour and are
carried to the next slice.

L15 **Two of my own tests had the finding baked in.** `v4_s6a_a_payment_may_cover_several_debts`
and `v4_s6a_both_ceilings_refuse_and_say_which_one` each gave its two sales a *different*
customer and then settled both from one payment — which the old total-only check allowed and
the new party check refuses. Both now build their second debt for the paying customer. A test
that encodes the defect cannot detect it, which is why they passed for a day.

## Verification of the correction

V5 **Five REDs observed before the fix, each with the measured number:**

| test | before | after |
| --- | --- | --- |
| `v4_s6a_a_refund_may_not_exceed_what_the_customer_actually_has` | 201, till −500.00 | 409, till 0.00 |
| `v4_s6a_only_money_that_came_in_can_settle_a_debt` | 201, debt 0 | 400, debt 100 |
| `v4_s6a_a_refund_through_a_rail_counts_what_left_not_just_what_arrived` | 201 twice, till −800.00 | 201 then 409, till 0.00 |
| `v4_s6a_returning_goods_on_a_settled_debt_is_a_conflict_not_a_500` | 500 | 409 |
| `v4_s6a_the_new_endpoints_refuse_an_amount_that_is_not_whole_cents` | 500 | 400 |

V5b **Every new guard proven to bite by neutering it**, in turn, each `&& false` restored
afterwards: the refund balance guard, the direction check, the earlier-refunds budget, and the
inherited-path room pre-check. Each turned its own test red and nothing else.

V5c **Re-measured against a live server, the three end-to-end sequences.** The ghost refund
answers 409 with the till at 0. The outbound refund cannot be settled (400, debt stays 100).
Two explicit notes against 100.00 collected answer 201 then 409, and the till lands at 0.00
instead of −800.00.

V5d `make test` green after the correction: 50 unit + 232 frontend + **119** backend + 23 e2e.

## Second review: the correction, approved

L16 **Review `review-1becd57b99cb3a1f`, tier high, lenses `risk` and `reliability`, 4 files,
711 lines.** Scope: `b0cfe16..b0735f4` — the correction of the five CRITICAL findings above,
not the whole slice. Result: **approved**, authority burned
(`gentle-ai.review-acknowledged/v1`). Four findings, all `informational`, none opening a
correction.

L17 **Why the scope is the correction and not the slice, measured twice.** The whole tree
(`e7c8856f…`, 4221 lines, four lenses) is refused at START with
`lens_context_budget_exceeded` — *"the candidate's complete reviewer evidence exceeds the
native context budget, so no review authority was created"*. Retrying that candidate cannot
succeed and nothing had to be abandoned. `review status --next-transition --base-ref b0cfe16`
then produced a 711-line candidate with two lenses, which fits. **The provider's own stop
message named this route**, and it is the second time in this feature that reading the stop
instead of guessing saved the authority.

L18 **`R1-001` and `R3-001` are the same residual, found by both lenses, and it is real.**
The correction closed *settling with the outbound refund itself* and left the sequential
variant open:

```
collect 100.00  ->  available = 100.00
refund  100.00  ->  passes the new ceiling with equality; the till is now 0, the balance -100.00
settle the debt with the ORIGINAL inbound payment
                ->  direction ok, party ok, per-payment remainder ok, obligation room ok
                ->  debt 0, and the 100.00 has already left
```

Every new guard passes and the money is resolved twice. **It is a narrowed residual and not a
new regression** — the base could do this too, and it could also do the same-payment variant
that this correction closed. Recorded for the next slice: the settlement's remaining-room
question should be asked against `lock_party_balance`, not against the payment alone.

L19 **`R1-003` — a narrow race in the inherited-path pre-check.** The pre-check reads the
obligation and its sums without a lock the settlement writer respects, so a settlement
committing between that read and the note's COMMIT still reaches the deferred ceiling and
answers 500 in that window. The database enforces the ceiling, so no over-lowering survives;
only the status code is affected. A lock-ordering fix, not a data fix.

L20 **`R1-002` — the unauthenticated posture, confirmed by an independent lens.** Every write
route this slice touches accepts anonymous requests, and the new ceilings bound what an
anonymous caller can move. That is a strict improvement over the base, which had the same
surface uncapped. Closing it is an authentication design and is not this slice's.

L21 **A tooling rule adopted mid-review, because it cost two rejections.** The provider's
bindings are opaque and must never be retyped: a hand-copied `subject-hash` was rejected once
(`collect-group-rejected`) and a second hand-copy would have been captured wrong silently. The
rule now is **serialize the provider payload to a file, rebuild the binding from it, and assert
that every argument token and the subject hash match the provider's own bytes before
submitting**. Both captures after that rule passed first try, including the retry after a
`pi-host-relay-transport-failure`, which was only retried because fresh STATUS reoffered the
**identical** revision and subject hashes.

## The two residual findings of the review, closed

L22 **`R1-001` / `R3-001`, found independently by `risk` and `reliability`.** The correction
closed the variant that settles *with the outbound refund itself*; the sequential one survived:

```
collect 100   available = 100
refund  100   passes: 100 <= 100. The till is now 0 and the balance -100.
settle the 100 debt with the ORIGINAL inbound payment
              direction ok, party ok, per-payment remainder ok, obligation room ok
              -> debt 0, and the cash has already left
```

The per-payment ceiling counted one payment's own settlements and could not see that the party
had taken that money back out. **The fix is the ceiling the helper already names:** a settlement
may not spend more than the party still has, asked through `lock_party_balance` — the same
function the refund path uses, which is why it exists as one definition rather than three.

Test first: `v4_s6a_a_settlement_cannot_spend_money_that_was_already_refunded`, observed
failing at 201 with the debt cleared, then green at 409 with the debt standing. Neutering the
guard (`&& false`) turns it red again. Measured against a live server: the sequence answers
409, the debt stays 100, the till stays 0 — and the legitimate path in the same test (100 more
arrives, then it settles) still works, so the refusal is not a blanket one.

L23 **`R1-003`, a narrow race rather than an amount.** The inherited credit-note pre-check read
the obligation and its sums without a lock the settlement writer respects, so a settlement
committing between that read and the note's COMMIT reached `0022`'s deferred ceiling and the
caller got the 500 the pre-check exists to remove. The database refused correctly either way —
no over-lowering survived — so only the status code lied. The pre-check now takes `FOR UPDATE`
on the obligation, in the same order `create_settlement` takes it, so two writers cannot
deadlock. **This one has no dedicated test, and that is stated rather than hidden:** a race
between two transactions is not observable from an in-process test without a second connection
and a held lock, which is a harness this suite does not have. The lock is the fix; the proof is
the reasoning about lock order, not an assertion.

L24 **No migration was written for either.** The owner said migrations are not needed yet, and
neither fix requires one: both are route-level ceilings and locks over tables that already
exist.

V6 **Verification of the two residuals.** `make test-backend` 121 (was 120; +1). All 19
`v4_s6a` tests pass. The new guard proven to bite by neutering. Both end-to-end sequences
re-measured against a live server, legitimate and illegitimate side by side.

L25 **The two remaining `informational` findings, closed.** Neither changes behaviour, which is
why they were carried rather than fixed in the correction.

`R2-003`: three tests derived "outstanding" with their **own** SQL instead of `outstanding_of`.
One of the three was already the weaker definition — it subtracted adjustments only, so it
would have disagreed with the helper the day a settlement lowered the same debt. All three go
through the helper now. **Proven by neutering the helper's adjustment term and watching both
migrated tests fail**, which is the only way to know a test reads through a shared definition
rather than beside it.

`R2-004`: `insert_paid_sale` stamped `money_entries.occurred_at` with `NOW()` while production
uses the sale's own `created_at` — one transaction, one instant. The helper claimed to mirror
the route it did not mirror. It now returns `created_at` from the insert and binds it, so a test
that compares the book's ordering against the document's asks the same question the route
answers.

L26 **`R1-refund-guard-rail-blind`, found by the `risk` lens of `review-2e03230018397328` and
not closed by any earlier correction.** The refund guard measured what arrived **in aggregate**
and never on the rail the note names. `SUM(settlements)` carries no rail information, so a sale
collected in the till could be refunded by transfer — the harm the guard exists to close, through
a side door.

Reproduced before the fix: a 100.00 sale on account collected in cash, refunded by transfer,
answered **201** and drove the bank to **-100.00**.

The distinction the guard was missing: **money is not fungible across rails for the purposes of
a refund.** A sale's arrival on a rail is a sum over that sale's own documents — the sale's entry
when it was paid outright, the settlements whose payment used that rail when it was owed — minus
what earlier explicit notes already paid out through the same account. Measured after: **409**,
bank at 0.00, till keeps its 100.00, and the rail that *did* receive the money still refunds it
(the note through cash answers 201 and the till goes to 0).

L27 **A note on how this was found, because it changes what this review was worth.** The three
lenses of `review-2e03230018397328` also raised two CRITICAL findings — a refund without a
balance ceiling, and a settlement without a direction or party guard — and both were **already
closed** by `b0735f4` and `590de3f`. The candidate under review was `b0cfe16`, the commit
*before* those corrections, so the review rediscovered defects that no longer existed.

That is not a wasted review: `R1-refund-guard-rail-blind` is real, was open in HEAD, and no
correction had touched it. But it does say something about reviewing a chain: **a candidate that
is not the state you intend to ship finds the defects of a state you already left.**

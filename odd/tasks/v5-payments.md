# v5-payments — collecting and paying against several debts in one document

Slice 2 of "cobros y pagos", continuing `v5-customer-account.md`. Branch
`feat/v5-payment-allocation`. Test runner: `make test` (see `docs/en/testing.md`).

**State: complete, reviewed natively and APPROVED** — lineage `review-9cff43c50976184c`,
lenses `risk` + `resilience` + `reliability`, tier high, 3.332 lines, correction budget 200
with 199 used. Authority burned (`gentle-ai.review-acknowledged/v1`). `make test` green
(52 unit + 261 frontend + 154 integration + 31 e2e), `make lint` clean, `make sqlx-check` in
sync.

**Delivered as a chain of three PRs**, cut by surface rather than by commit (the review's
findings crossed backend and client, so a commit-order split would have produced slices that
do not build). Each slice was verified green on its own — backend, frontend and e2e — before
being pushed, and **the tip of the chain is byte-for-byte the tree the review approved**
(`git diff` against `ba4972b` empty):

| PR | branch | base | changed lines |
| --- | --- | --- | --- |
| #94 | `pr1-collection-endpoint` | `main` | 2 121 |
| #95 | `pr2-collection-form` | `pr1-collection-endpoint` | 1 227 |
| #96 | `pr3-closing-e2e` | `pr2-collection-form` | 144 |

`feat/v5-payment-allocation` keeps the work-unit history and is not pushed as a PR of its own;
the chain supersedes it. The two slices over the 400-line guideline carry the reason and a
`size:exception` recommendation in their bodies: the endpoint cannot be split from its tests
without leaving a slice that does not compile, and the form cannot be split from the
dictionaries it renders.

Slice 1 taught the shop to **read** a party's account. Slice 2 lets it **write**: the
button the owner described in `L2` of `v5-customer-account.md` arrives, together with the
endpoint `L3` there deferred. The state that must hold when this slice closes:

```
a cashier opens a client's panel, types how much money arrived and which debts it is
for, and the panel tells them which documents that money closed and which it did not.
```

## Specs

S1 **One endpoint per side, with the party in the path, and one implementation.**
`POST /api/customers/{id}/collections` and `POST /api/suppliers/{id}/payments`. Two thin
handlers over one shared function parameterised by the closed `Party` enum that
`routes/parties.rs` already defines — the same shape as `customer_account` /
`supplier_account` over `account_of`, and for the same reason: the two branches read
different tables, and a typo in a string would become a write against the wrong table
instead of a compile error.

The party is in the **path**, not the body, so a request cannot name one party and
allocate to another's debts. The two route words are the shop's own: a client's money
arriving is a **cobro**, a supplier's money leaving is a **pago**.

S2 **One transaction: one payment, one ledger entry, N settlements.** The payment is the
money moving (one `money_entries` row, signed by direction, exactly as
`POST /api/payments` writes it); each settlement says which debt that money answered and
writes **no** ledger entry. A settlement that moved money would count the same coin twice
— the sentence `routes/payments.rs` exists to keep.

Any refusal writes nothing: no payment, no ledger entry, no settlement, no till movement.

S3 **The allocation rule is the owner's hybrid, and it is the reason this endpoint
exists.** `L2`: the cashier allocates **freely and manually** to whichever debts they
name, and **the rest goes to the oldest debt**, oldest first.

Concretely, given `total` and the `manual` lines:

1. each manual line settles exactly the debt it names, for exactly its amount;
2. `left = total − SUM(manual)`;
3. `left` is applied to the **remaining** debts — every unpaid debt the cashier did not
   name — in **ascending document date** (the instant the sale or purchase was recorded,
   `obligations.occurred_at` / `payables.occurred_at`), tie-broken by id so the order is
   total;
4. when the money runs out or the debts do, it stops. **Money left over after every debt is
   covered is not an error**: it stays as the party's credit balance (`in_favour` for a
   client, `in_our_favour` for a supplier) — money that arrived and is not yet spoken for,
   exactly what `S6` of `v4-receivables.md` already describes.

**A debt the cashier named is excluded from step 3** (`L8`). Not a detail: the schema has
`UNIQUE (obligation_id, payment_id)`, so one payment reaches a debt through exactly one
row, and a row has exactly one `source`. If the walk could revisit a named debt, that debt
would need a row with two origins and the response could no longer say whether the cashier
typed it or the rule added it. The consequence is stated rather than hidden: a cashier who
names the oldest debt and leaves an amount unallocated sends the remainder to the next
oldest, not to the named one.

`L2` also fixes what this is **not**: it is not FIFO-only (the cashier's line wins over the
date), and it is not "the cashier picks everything" (the remainder must not be typed by
hand, which is the mistake the rule exists to prevent).

S4 **Both ceilings, and each is a number the caller can act on.** `manual` must be
non-empty-or-empty without being a lie, so:

| refusal | status | why not the other |
| --- | --- | --- |
| a manual line naming a debt that is not in **this party's unpaid list** | `400` | the request names something this party's account does not contain: a client error in the request's shape |
| two manual lines naming the same debt | `400` | one row per `(debt, payment)` pair is the schema's rule; refusing by hand gives a status instead of a `500` |
| an amount that is not whole cents, or not positive | `400` | the database's CHECK; asking first is what makes it a `400` |
| `SUM(manual) > total` | `409` | it is not a malformed request, it is money that does not exist |
| a manual line greater than that debt's **outstanding** | `409` | the debt is not that large *now*; the caller asked to settle more than is owed |
| a party with no debts at all | `201` | money arriving from a client who owes nothing is a credit balance (`S3.4`), which is a real state both panels already render |
| an unknown payment method, an unknown party, or a `total` that is not positive | `404` / `400` | the same shape checks `POST /api/payments` makes, and for the same reason |

`outstanding` is never re-derived by the client: `amount − SUM(adjustments) −
SUM(settlements)`, the same subtraction `S4` of `v5-customer-account.md` states, in the
same place (`account_of`).

S5 **"Unpaid" is derived, and this slice stores nothing new.** No `paid` column, no
`status`, no allocation table: `settlements` and `payable_settlements` already are the
records, and the third box of the panel is a `HAVING outstanding > 0` that was already
written. A fully settled debt leaves the list, and its absence is the state.

S6 **The response carries the party's account, read by the same function the GET uses, in
the same transaction.** The panel must not re-read the account afterwards: a second
request is a second instant, and this feature has already paid once for a panel that
contradicts itself (`R3-001` of `review-b1f1ea20a0e4419b`). `account` in the response is
byte-for-byte the shape `GET /api/customers/{id}/account` returns.

**The write transaction runs at the default isolation level, and that is a decision about
money rather than about presentation** (`V8`). Its ceiling is judged twice — in the plan and
in `0022`'s `settlement_within_both_ceilings` — and both read `SUM(settlements)`, so both
must see the latest committed state. A `REPEATABLE READ` snapshot fixes the state before
the row locks are taken, which makes a loser resume from a commit it cannot see: measured, a
debt of 100.00 was settled **twice, for 200**, and the loser answered `201`. Read
consistency comes from the locks (`S8`), not from the snapshot, and the cosmetic risk a
concurrent *sale* could open in the response's four queries is the price the ceiling is
worth.

Each applied row carries its **`source`**: `manual` for what the cashier typed, `oldest`
for what the rule added. A cashier who settles one document and finds two closed needs to
see which one the rule decided, and no test could assert the rule without it.

S7 **`POST /api/payments` and `POST /api/settlements` are untouched.** This endpoint is
additive: the two-step path stays exactly as it is, and the mirror
(`POST /api/payable-settlements`) too. Anything that reads their shape is unaffected.

S8 **One lock order, the one `0022` established: the party's payment rows, then the
debts in a deterministic order.** The debts are locked `FOR UPDATE` in ascending
`(occurred_at, id)` before the first ceiling is judged, and the party's payment rows are
taken first so every writer of a party's money agrees on the order. A deadlock here would
be a deadlock the base cannot have — `aa95665` records the cost of getting this wrong once.

**What that lock does *not* buy, stated because the first version of this spec claimed it
did** (`V6`): it is not what stops two racing collections from over-settling one debt. That
is `0022`'s `settlement_within_both_ceilings`, which locks the payment and then the debt, so
the *trigger* serialises the pair and refuses the second. Measured: the race test passed
with this route's `lock_debts` deleted outright, and again with its order inverted. The
route's lock is an **ordering discipline** — a read consistent with the writes that follow,
and one lock order for every writer — and the test that exists asserts the ceiling that
actually holds.

S9 **The panel gains the collection form in the balance box, where the owner put it.**
`L2` of `v5-customer-account.md`: *"el segundo el saldo y ahi el boton para hacentar el
pago"*. A `Cobrar` / `Pagar` button in box 2 opens, in place, a form with: the method
(active methods only, the same picker the sale draft offers), the amount received, and one
optional amount per unpaid document in box 3. Submitting calls the endpoint and replaces
the panel's account with the one the response carries.

**The form never simulates the allocation rule.** It shows the remainder as one figure
(`total − manual`) and the sentence that names its destination — *"el resto va a las
deudas más viejas"* — and it does **not** claim which documents. The server's `applied`
with its `source` is the only statement of what happened. A second implementation of one
rule in a client is how the two disagree, and this repository has paid for that class
three times (`V7b` of `v5-customer-account.md`).

S10 **`DESIGN.md`'s two rules, applied to a form.** "The Balance Owns the Accent Rule": the
submit is the primary action — solid ink bar at the foot of the form, never accent-filled,
and the accent stays on the running balance figure. "Document State Is Readable": a
settled document is text plus a 2px tick, never colour alone. Both shipped languages
(`es-AR` default, `en`) get every string.

S11 **Scope ends at one collection.** Not in this slice: applying a credit balance to a
future debt automatically (that is `S6`'s manual decision and stays manual), an
idempotency key (the ledger is append-only and no route here has one — a double submit is
two payments, recorded as such), reversing or editing a collection (compensating rows, and
no route for it yet), a till-floor guard for supplier payments (pre-existing: the ceiling
in `create_payment` guards a **customer's** balance, not the till), and any change to the
three boxes' content.

S12 **Every rule is proven by a test that bites.** RED first: each spec's test is written
and observed failing before the code exists. Each guard is then proven by neutering it and
watching the test it covers fail — the fixtures are chosen so neutering **changes the
answer** (`V1b` of `v5-customer-account.md`).

## Tasks

| ID | Spec | What | Route | Risk | Commit |
| --- | --- | --- | --- | --- | --- |
| T1 | S1-S8, S12 | the endpoint: one payment, N settlements, the hybrid rule, the account in the response, both ceilings, one lock order | inline | item 1 | done — 151 integration, V1 |
| T2 | S9, S10, S12 | the form in the balance box, the pure validation module, both dictionaries | inline | item 1 | done — 257 frontend unit, V2 |
| T3 | S3, S9 | one e2e: a collection typed in the panel closes two documents, by identity | inline | — | done — 31 e2e, V3 |
| T4 | all | `architecture.md`, the full `make test`, close | inline | — | done — V4 |

Order is dependency: T2 needs T1's contract, T3 needs T2's form, T4 needs all three.

## Log

L1 (owner, verbatim): "arranca con el slice 2. Anda con el hibrido; pago manual libre, con
resto a la mas vieja"

L2 (owner, decision, recorded as the option chosen): **manual free allocation with the
remainder to the oldest debt.** Offered four rules — oldest first only, smallest first
only, the cashier chooses everything, and this hybrid — the owner chose the hybrid. It is
`S3`, and the two rejected shapes are why `S3` says what it is *not*.

L3 (decision, mine, and stated because it is the shape of the whole task): **the party
goes in the path and the endpoint is nested under it**, mirroring `GET
/api/customers/{id}/account`. The alternative — one `POST /api/collections` with
`party_kind` in the body, mirroring `POST /api/payments` — was rejected because it lets a
request name one party and allocate to another party's debt, and the schema's own guard
(`settlement_within_both_ceilings`) would answer that with a trigger exception rather than
a status the caller can act on.

L4 (decision): **the debt is named `debt_id` on the wire**, and it is the `id` of an
unpaid document in the account payload — the obligation for a client, the payable for a
supplier. Not `obligation_id`: one request body serves both mirrors, and `S3`'s own words
are that a debt is a document. The mapping is stated once, where the struct is declared.

L5 (decision): **the response returns the account, and the panel does not re-read.** One
transaction, one snapshot, no second instant. The cost is a heavier payload; the
alternative is the half-loaded panel `S6` of slice 1 exists to prevent.

L6 (decision): **the form does not simulate the rule** (`S9`). The preview would be a
second implementation of `S3` in a client, and the class of defect this repository has
already paid for three times is two things that must agree, allowed to differ.

L7 (measured before planning): `POST /api/payments` exists and writes the money and the
ledger entry; `POST /api/settlements` and `POST /api/payable-settlements` exist and write
one settlement each. Nothing allocates more than one, and nothing computes a remainder.
`routes/parties.rs::account_of` already returns every unpaid document with its
`outstanding`, sorted newest first — this slice sorts it, it does not re-derive it.

L8 (decision, forced by the schema and found while writing the first test): **a debt named
in `manual` is excluded from the remainder walk.** `UNIQUE (obligation_id, payment_id)`
means one payment reaches a debt through one row, and each row in the response has one
`source`. Without the exclusion, a named debt that also sat in the walk would need a row
with two origins, and `S6`'s `source` would be a lie in exactly the case the rule exists to
make visible. The visible consequence is recorded in `S3` rather than left for a reader to
discover.

## Verification log

V1 **T1, the collection endpoint.** `cargo test --all-features`: **52** unit + **151**
integration (was 144; +7). `make lint` pending until T4, `sqlx-check` untouched (every new
query is `query_as`/`query_scalar`, none is `query!`).

**RED first**: all seven answered `404` before the route existed — the endpoint was absent,
not broken.

**Every guard proven to bite by neutering**, and two of the attempts changed the fixtures:

| guard neutered | what failed | what it said |
| --- | --- | --- |
| the walk revisits a debt the cashier named | `v5_s3_a_named_debt_is_not_walked_again…` | `500`, because the schema's `UNIQUE (…, payment_id)` refused the second row |
| the debt ceiling (`line.amount > outstanding`) | `v5_s4_each_refusal…` | `500` where the test demands `409` — the exact defect the pre-check exists to prevent |
| an empty plan is refused (no credit balance) | `v5_s3_money_from_a_party_who_owes_nothing…` | `400` where the test demands `201` |
| a settlement writes a ledger entry | all seven | `500` on the ledger trigger, and the ones that survived it would have double-counted |

V1b **The reverse-walk neutering passed the first time, and the fixtures were the problem.**
Reversing the walk (`enumerate().rev()`) left every test green, because the first fixtures had
**three debts of the same size** — inverting the order changed no figure. Rewritten as 100/60/40
with a total that runs out mid-walk, the same neutering fails with `50.00` where the rule says
`60.00`. This is `V1b` of `v5-customer-account.md` for the third time in two slices, and the
first time it was caught before shipping rather than after: **a test that passes after
neutering means the fixture cannot see the difference, not that the guard is redundant.**

V1c **A case with no test at all, found by neutering.** Adding `if planned.is_empty() { 400 }`
broke nothing, which is the report that "money from a party who owes nothing" was uncovered.
The new test (`v5_s3_money_from_a_party_who_owes_nothing_is_a_credit_balance`) fails with `400`
against that neutering and asserts the whole advance path: the money is in the till, it is
`in_favour`, and a later sale is paid from a second collection.

V1d **A refusal must not leave a payment behind, and that is asserted rather than assumed.**
`v5_s4` checks seven refusals and then, in the same test: the three debts still owe what they
owed, no `payments` row exists for the client, and the till has not moved by a cent. A
refusal that wrote the payment would be money the operator believes was rejected, in a ledger
that cannot be taken back row by row.

V1e **`HAVING` was the wrong clause for a per-row predicate.** The first version wrote
`HAVING o.amount - … >= 0` with no `GROUP BY`, which makes the whole query one group — it
would have been rejected for a non-grouped column, so no test could have caught it in the
wrong state. `routes::parties` uses it in a `WHERE` for exactly this predicate, and the
difference is not stylistic: `HAVING` filters groups, `WHERE` filters rows.

V1f **Two refusals for two different facts, and the distinction is the point.** A debt this
party does not have, or a debt already resolved, were both `400` in the first version. They are
now `400` (not yours) and `409` (nothing left on it) — the second is what a cashier sees when a
colleague collected the same document a moment earlier, and it is a conflict rather than a bad
request. The `debts` query therefore returns resolved debts with `outstanding = 0` instead of
filtering them out; without them, the two answers would collapse into one.

V1g **A test of mine asserted an arithmetic the rule does not produce.** The supplier fixture
used 100 against two payables of 80 with 50 named, which leaves 50 — the same figure as the
named amount, so both debts took 50 and the dates decided nothing. Worse, I had written
"the rule took the older one for its full 80" under it. The test was wrong about its own
fixture and the code was right; corrected to 120 (older takes 70, named keeps 30) so the two
figures differ and the order is observable.

V2 **T2, the collection form.** `npm run test:unit`: **257** (was 246; +11 across two files).
`npx eslint src tests` clean, `svelte-check` 0 errors, `prettier --check` clean.

**RED first**: `collection-form.test.ts` failed to even load — the module did not exist.

**Guardias probadas por neutering:**

| guard neutered | what failed | what it said |
| --- | --- | --- |
| the remainder clamped at zero instead of going negative | `says the plan exceeds the money rather than silently clamping it`, and the gate's ordering | `expected '0.00' to be '-20.00'` |
| a blank line folded into zero (`cents(amount) ?? 0n`) | `counts a blank line as unallocated, and a typed zero as a refusal` | `expected 2 to be 1` — the abandoned line counted as an allocation |
| the duplicate-document check removed | `refuses the same document named twice` | `expected { ok: true }` |
| the preview adds a `documents` field (the temptation) | `exposes no field that names which documents the remainder closes` | the key list gained `documents` |

V2b **A hole found by writing the test rather than by the neutering.** A line with a typed
`0.00` passed the whole gate: `lineCents` answered `0n`, no ceiling complained, and the server
would have refused it with `400` (`S4` checks every manual amount). Fixed with a
`lineNotPositive` blocker. The distinction it protects is the one the neutering table's second
row measures: **blank is "not allocated", `0` is an allocation the server refuses**, and the
easy implementation collapses them.

V2c **The blank-vs-zero distinction needed a second assertion to be observable.** Folding blank
into zero passed every test before `allocatedLines` was asserted: with a blank line and a `20.00`
line the manual total is 20 either way. The count is what makes the difference visible, which is
the slice-1 lesson again in a smaller key — a test that passes after neutering means the fixture
cannot see the difference.

V2d **The submit is an ink bar, not an accent fill** (`S10`, `DESIGN.md`'s "The Balance Owns the
Accent Rule"). The form follows `Catalog.svelte`'s `.do` precedent verbatim — solid `--ink`, white
text, `text-transform: uppercase` in the figure face — because the panel's loudest element is the
balance figure above it and a form's own bar must not compete. Proven by reading it against the
rule, not by a test: no test can see a colour, which is why the rule is quoted where it is
applied.

V2e **The remainder is stated in words, never in documents** (`S9`). The form renders
`El resto ($ 210,00) va a las deudas más viejas.` and nothing that names a document. The response's
`applied`, with `source` per row, is what the panel shows afterwards.

V3 **T3, the end-to-end path.** `make test-e2e` **31** (was 30; +1). `make test` green
throughout: 52 unit + **257** frontend + **151** integration + **31** e2e.

**The e2e found two mistakes of mine before it would pass, and neither was in the code:**

1. **The first hint assertion named the wrong field.** An empty form is missing the method
   *and* the amount, and the gate answers the first one — so the spec demanded the amount's
   sentence and saw the method's. Both are now asserted, in the order the fields appear,
   which is the behaviour the gate promises.
2. **The first total was 100 against 125 of debt.** 80 + 45 is 125; the panel closed the newer
   sale for its full 45 and left the older one with 25 — correct behaviour, wrong expectation.
   Corrected to 125, so the spec's claim ("both documents closed") is the claim it makes.

**Two guardias probadas por neutering:**

| guard neutered | what failed | what it said |
| --- | --- | --- |
| the `Cobrar` button rendered outside the balance box | this spec | `waiting for … section.box … button "Cobrar"` — `L2`'s placement is the placement |
| the remainder paragraph names a document | this spec | `Received string: "El resto ($ 80,00) va a las deudas más viejas. #a65f01ce"` |

V3b **A weak assertion, found by the second neutering rather than by review.** The first version
asserted `not.toContainText(olderSale.id)` — one document. A form that predicted the allocation
would name whichever document it chose first, so the assertion passed while the rule was being
re-implemented in the client. It now asserts that the paragraph contains **no** `#` at all, which
is the property `S9` actually states. This is the same lesson as `R3-002` of the T1 review in
slice 1, one level up: an assertion that names one forbidden value misses the other one.

V3c **What the panel proves and what the database proves, kept apart.** The spec asserts the DOM
(both documents left the third box, `Nada pendiente`, the figure `$ 0,00`) and then the API (no
unpaid sales, `owed` and `in_favour` both zero). The `applied` rows with their `source` are
**not** asserted here, because the panel does not read the response's `applied` — it renders the
account. That claim belongs to the integration tests (`v5_s2`, `v5_s3`), which are where a
`source` can be compared. A spec asserting a field the UI never touches would be testing the
API twice and the screen once.

V4 **T4, the close.** `make test` green: **52** unit + **257** frontend + **151** integration
+ **31** e2e. `make lint` clean (clippy `-D warnings`, eslint, svelte-check 0, prettier).
`make sqlx-check` **in sync** — every new query is `query_as`/`query_scalar`, so no `.sqlx`
entry was needed and the offline build stays honest.

V4b **`clippy -D warnings` found a dead field, and deleting it was the right fix.** `Debt` carried
`occurred_at` "to document what oldest means", and nothing read it: the ordering lives in the
query's `ORDER BY`. `#[allow(dead_code)]` would have kept a field that invites a caller to
re-sort a list the rule already ordered — a second ordering of the same rows. Deleted, with the
reason written where the struct is.

V4c **Deleting it broke every test for one round, and that is recorded rather than hidden.** The
first attempt removed the column from the `SELECT` **and** left `ORDER BY o.occurred_at` in a
query whose result tuple had already changed shape; the endpoint answered `500` on every call.
Seven tests failed, the cause was obvious from the first `body:` being empty, and the fix was to
keep the `ORDER BY` (legal without selecting the column) and change only the tuple. **A refactor
that compiles is not a refactor that runs**, and the suite is what said so.

V4d **What `architecture.md` gained.** Two route rows with their full contract — the body, the
`400`/`409`/`404` split, the response including `account` read in the same transaction, and the
`source` on every applied row — plus the mirror and the one difference that is structural rather
than an omission (a customer's overpayment is refundable, a supplier's is not, because
`payments_direction_matches_party` makes `in` from a supplier inexpressible). Two invariants:
one document moves the money and the settlements write no ledger entry, and the one lock order
for a party's money. The table's own heading now says v5 collects.

V5 **The independent verification, run in parallel with the native review, and it agreed with
it.** A separate verifier read the whole feature document and the code against `S1`-`S12` and
returned the **same four defects** the native review found — the amount parser dropping a
hand-allocated line, the `applied` rows discarded, `READ COMMITTED` instead of `REPEATABLE
READ`, and the parser's dot-only separator — plus three advisories the review had not made.
Two independent readers, the same answer, and one of them alone would have been enough.

**What it found that the review did not, and what I did about each:**

1. **The `409` for a resolved debt had no test.** The branch existed and nothing produced it,
   which is the most common refusal in practice — a colleague collected the document a moment
   earlier. `v5_s4_a_debt_already_resolved_is_a_conflict_and_writes_nothing` now drives it and
   proves the refusal records no money.
2. **`S2` was measured only through balances.** The balance assertions prove the effect; they
   cannot see whether the shape is one entry or three.
   `v5_s2_one_payment_writes_one_ledger_entry_and_n_settlements` counts: **one** `money_entries`
   row for the payment, **two** settlements, which is the claim `S2` makes and the thing that
   would break if someone added the missing per-settlement entry "helpfully".
3. **`v5_s8` did not bite the lock.** This was the valuable one, and it is `V6` below.

V6 **A test that claimed more than the code guarantees, and the spec that repeated the claim.**
`v5_s8` drove two collections over the same single debt and asserted one wins. The verifier
reported it green **with `lock_debts` deleted outright** and again **with its order inverted**.
I reproduced both myself: green, three runs out of three, in both neuterings.

The reason is structural. `0022`'s `settlement_within_both_ceilings` takes `FOR UPDATE` on the
payment and then on the debt, so the **trigger** serialises the pair and refuses the second
settlement. The route's `lock_debts` was never what prevented that race.

So both the test and the spec were corrected rather than propped up. The test is now
`v5_s8_two_racing_collections_cannot_settle_one_debt_twice`, and it asserts the ceiling that
actually holds (never two `201`s, a losing collection is a `409`, the settlements sum to the
debt). `S8` says in the open that the lock buys **ordering** — a read consistent with the writes,
one lock order for every writer — and that the ceiling belongs to the trigger; the code's
doc-comment on `lock_debts` says the same.

**The lesson is the one this repository keeps paying for, in a new place:** an untested
neutering hides in a test that looks like coverage. `v5_s8` would have shipped as proof of a
property no code path provided, and the only thing that found it was an outside reader deleting
the line and watching the test stay green.

V7 **The correction cycle could not close, and the reason is recorded rather than worked
around.** The provider's first lineage (`review-b6bebae2ab91c89f`) found the four findings,
corroborated them with a refuter, and asked for a bounded correction with a frozen budget of
**200 diff lines**. The corrections measure **501** (413 insertions, 88 deletions). Declaring
501 exceeds the maximum; declaring 200 would be false. **The four findings were all real, all
fixed, and all proven to bite** — the cycle is open on accounting, not on substance.

`V7b`: the corrected state is a **different candidate** (`196ba88`), so it is judgeable on its
own, which is what closed the equivalent impasse in slice 1 (`review-b1f1ea20a0e4419b` could not
close; a fresh review of the corrected state approved). A second lineage
(`review-5e9da0f9a1d1fad5`) was started over the corrected tree for that reason.

V7c **`make sqlx-check` was reported in `V4` and the independent verifier could not reproduce
it** — it needs `cargo sqlx` and a live database, and it ran in a scratch copy without either.
It was run here, before the correction, and again after: **in sync** both times. Every query this
slice adds is `query_as`/`query_scalar`, so no `.sqlx` entry was needed; the claim stands, and
here is the reason it was the one claim an outside reader could not confirm.

V8 **`REPEATABLE READ` was a regression, and the fix for the fix was measured.** The first
correction cycle's `R3-003` said the write transaction was `READ COMMITTED` and the response's
account could therefore pair a stale figure with a fresh list. I changed it to
`REPEATABLE READ` and wrote a structural test pinning that. **That was worse.**

The second review (`review-9cff43c50976184c`) reported it as `R3-001` / `R4-1`: the ceiling of
this endpoint is judged in the plan *and* in `0022`'s trigger, both read `SUM(settlements)`,
and a snapshot fixed before the row locks makes a loser evaluate both against a state that no
longer exists. The refuter corroborated it.

**I did not take it on the reviewer's word.** I forced the interleaving: a blocker holds the
debt row, the request enters and blocks in `lock_debts` **after taking its snapshot**, the
blocker settles the debt in full and commits, then releases. Result on the unfixed code:

```
PROBE racer status: 201 Created
PROBE total settled on a 100.00 debt: 200
```

A debt of 100 settled for 200, with a `201` and `debt_outstanding: 0.00`. After reverting to
the default level, the same probe answers `409` and the settlements sum to **100**.

That probe is now `v5_s4_a_collection_racing_a_committed_settlement_reads_the_current_state`,
a permanent test, and reverting the isolation level makes it fail with `left: 201, right: 409`.
The structural test that pinned `REPEATABLE READ` was **replaced** rather than deleted, with
the story in its comment: two tests asserting opposite things is how the next session inherits
the wrong one.

**The lesson is the one this repository has now paid for twice in two slices**: a correction
reviewed only for what it fixes can introduce a defect in what it touches, and the only thing
that caught this was a reviewer reading the *interaction* between my fix and the schema's
trigger. "The fix made the reported symptom go away" is not evidence that the fix is right.

V9 **The second lineage approved, and it took three findings to get there.** `review-9cff43c50976184c`
(`risk` + `resilience` + `reliability`, tier high): two CRITICAL findings, both about the
isolation level I had changed in the *previous* correction, both corroborated by the refuter,
both fixed and **measured** (`V8`). Then a targeted validation against the frozen trees —
`original_criteria.passed`, `correction_regression.passed`, one follow-up — and the authority
burned on the exact acknowledgement.

The review lists five more findings as **informational** and non-blocking, each recorded here
rather than fixed in this slice:

- `R1-preexisting-till-floor` — the supplier payment route has no till floor. Pre-existing:
  the ceiling in `create_payment` guards a *customer's* balance, never the till. Named in `S11`
  as out of scope and now recorded by the review as well.
- `R3-002` — the panel's `collected` receipt state is not cleared when the selection changes,
  so a receipt from one party can be visible while another party's panel is open. Real, small,
  and the next thing I would fix; it is a presentation defect, not money.
- The remaining three are suggestions about wording and structure.

**What this slice cost, in review cycles: two lineages, one abandoned on a 200-line budget it
could not fit, 501 lines of correction in the first and 199 in the second.** The value is not
in the count: the first lineage's four findings were all real, and the second lineage's two
were defects *introduced by fixing the first*. That is the sentence worth carrying forward.

V10 **The delivery check, which only the image can give.** `docs/en/testing.md` names exactly
one proof that no other level provides — a stale image fails at boot with
`migration N was previously applied but is missing in the resolved migrations`, which
`cargo run` cannot reproduce because it compiles fresh and embeds the current migrations — and
**CI never builds the backend image**, so a green CI says nothing about it.

| what | result |
| --- | --- |
| `docker compose -f docker-compose.prod.yml build backend` | built: `3cb63af5…`, created 2026-10-25 (replacing the 2026-10-21 image) |
| migrations **inside** the image | exactly the repository's: one `0001_schema.sql`, 237 569 bytes |
| the container boots | `backend listening on http://0.0.0.0:3001` |
| `GET /health` | `200` — `{"status":"ok"}` |
| `GET /api/day` | `200`, and `zone` is the `SHOP_TIMEZONE` it was given |
| `GET /api/customers` | `200` |

Run against `tempox_test` (disposable) in **one** throwaway container on a free port, then
removed. No new stack, no new volume, the development volume untouched — the rule
`docs/en/testing.md` exists to enforce.

**Three of my four attempts were wrong and the record says so**, because a check you muddle is
not a check: `127.0.0.1` inside a container is the container, not the host; `--network
container:` does not combine with `-p`; a `timeout` on a foreground process kills the server
before it can be observed, and the `Command aborted` that produced was mine, not the server's;
and the clean attempt probed **`/api/health`, which does not exist** — the route is `/health`
at the root router, so a healthy server was reported dead by a wrong path. The last one is the
one worth keeping: the measurement was wrong, not the thing measured.

V11 **Merged.** `main` is `1c3558e`, three merge commits in order (#94 → #95 → #96), CI on
`main` **4/4 success** (`Backend`, `End to end`, `Frontend`, `Frontend image`), and `make test`
on `main` green: 52 unit + 261 frontend + 154 integration + 31 e2e.

**The chain's code is byte-for-byte the reviewed tree.** `git diff ba4972b origin/main` shows
one file — this document, gaining `V10` — and `git diff … -- backend/** frontend/src/**
frontend/tests/** docs/**` is empty. The review approved a tree; `main` carries that tree plus
its own delivery record.

`GitHub did not retarget #95 and #96 automatically`, because `--delete-branch=false` left the
base branches alive. Both were retargeted by hand, and each was checked to carry only its own
files before merging (11 for #95, 2 for #96) rather than a polluted diff built on a stale base.

The three chain branches and the work-unit branch are deleted, locally and remotely; the
repository holds `main` and `origin/main`. The work-unit history survives in the merge commits'
ancestry.

# v5-customer-account — a client's account, and a supplier's, read from one panel

Slice 1 of three for "cobros y pagos". **State: T1 and T2 reviewed natively and approved; T3-T4 ahead.** The product
shape was settled by the owner in conversation, and this file is the specification a later
session reads.

The state that must hold when this slice closes, and it is one sentence:

```
a client has a page, and clicking a row opens a panel that says who they are,
what they owe and what is owed to them, and which of their sales are still unpaid.
```

Nothing in this slice **writes**. Registering a payment is slice 2, and it needs the new
endpoint the owner asked for (`L3`); this slice only reads.

## Why it is three slices, measured

| what | lines today | note |
| --- | --- | --- |
| `Ledger.svelte` | 1696 | one table, sales |
| `PurchaseLedger.svelte` | 1052 | the same table, purchases, written separately |
| `PurchaseDraft.svelte` | ~360 | the existing side sheet precedent (`.sheet`) |

Two mirrored pages plus a three-box panel plus three new endpoints plus a side sheet is
1500–2000 authored lines. That is not one reviewable candidate, and this feature has already
paid once for shipping a candidate over the provider's review budget (`v4-payables.md` L9).

The owner's words: *"Si por ahi es mucho codigo separamos en un slice inicial y despues
seguimos con el A y B"*.

## Specs

S1 **Two pages, mirror images: `/clientes` and `/proveedores`.** Separate routes, not tabs,
because they are separate jobs in the shop's day and the rail can mark one active. They share
**one** panel component and **one** row component — the failure mode to avoid is the one this
repository already has, where the sales and purchase tables are two copies of the same idea
2700 lines long.

S2 **The panel is a side sheet, opened from a row, and deep-linkable.** Clicking a client row
opens a panel on the right with three boxes; the client's id goes in the URL as `?c=<uuid>` so
that a refresh, a bookmark or a link pasted to a colleague reopens the same panel. It uses
`replaceState`, not `pushState`: the browser's Back button must leave the page, not close the
panel.

**An id that does not exist is a state, not a crash.** `GET /api/customers/{id}/account`
answers `404` and the panel says so, with a way back to the list.

**Only the id lives in the URL.** The box's scroll, the open tab and the payment form stay in
component state. Three parameters synchronised by hand is how this becomes unmaintainable.

S3 **Three boxes, in this order and for this reason:**

| # | what | why it is first/second/third |
| --- | --- | --- |
| 1 | **who they are** — name, and the sales/purchases count | identity answers "did I click the right row?" before any number |
| 2 | **the balance** — what is owed, and what is in their favour, as **two separate figures** | `L4`: they can both be non-zero at once |
| 3 | **their unpaid documents** — the sales (or purchases) with outstanding > 0 | the detail behind the number above it |

**The two figures are never netted.** A client who owes 1000 on one sale and has 500 in their
favour from an overpayment is two facts, and netting them to 500 hides both. This is the
owner's decision (`L4`) and it is also what the schema already says: `outstanding` and
`unapplied` are two subtractions over two different tables.

S4 **"Unpaid" means `outstanding > 0`, and the figure is derived, never stored.** For a sale:

```
outstanding = obligations.amount
            - SUM(obligations_adjustments)   -- a credit note that lowered it
            - SUM(settlements)               -- money that arrived
```

and for a purchase the mirror over `payables` / `payables_adjustments` / `payable_settlements`.
A fully settled sale is **not** in the third box, and its absence is the state: there is no
`paid` column to read, and this slice must not add one.

S5 **The client's favour figure is derived from payments, not from a table.** It is
`SUM(payments in on this party) − SUM(settlements of them)`, the same subtraction
`lock_party_balance` computes. **There is no balance column and this slice adds none** — a
stored balance is a number that can disagree with the rows that produced it.

S6 **Three new read endpoints, and no writes.** Each is a read over tables that already exist:

- `GET /api/customers/{id}/account`
- `GET /api/suppliers/{id}/account`
- and the list pages need no new endpoint: `GET /api/customers` and `GET /api/suppliers` already
  exist and `api.ts` already has `listCustomers()` and `listSuppliers()`.

The account payload carries the three boxes in one response — identity, the two balance figures,
and the unpaid documents with their outstanding — because three round trips to paint one panel
is three chances to show a half-loaded panel that contradicts itself.

**`404` for an unknown id.** The panel's "not found" state depends on it (S2).

S7 **Both pages join the rail.** `/clientes` and `/proveedores` are two new items in
`+layout.svelte`'s rail, next to the four that exist, with labels from the i18n dictionary in
both shipped languages (`es-AR` default, `en`).

S8 **The panel obeys `DESIGN.md`, and the two rules it names are the ones at risk here.**
"The Balance Owns the Accent Rule": the blue accent does **not** become the payment panel's
section colour or a filled action colour — it stays on the running-balance figure. "The State
Is Readable Rule": a document that is settled versus owed is text plus a 2px tick, never colour
alone. A side sheet follows `PurchaseDraft.svelte`'s `.sheet` precedent rather than inventing a
fourth panel idiom.

S9 **Scope ends at reading.** No payment button in this slice (`L5`: it is slice 2, so that the
button arrives together with the endpoint that can apply one payment to several debts). No
filtering, no sorting, no export, no editing a client's name.

## Tasks

| ID | Spec | Slice | Route | Risk | Commit |
| --- | --- | --- | --- | --- | --- |
| T1 | S4, S5, S6 | the read model | `GET /api/customers/{id}/account` and the supplier mirror: the two balance figures and the unpaid documents, derived in SQL, `404` for unknown | item 3 | — |
| T2 | S1, S7 | the two pages and the rail | `/clientes` and `/proveedores` listing through the endpoints that already exist, plus the two rail items and their i18n keys | — | — |
| T3 | S2, S3, S8 | the panel | one side sheet with three boxes, opened from a row, `?c=<uuid>` with `replaceState`, the not-found state | — | — |
| T4 | all | close | the e2e that opens a panel by deep link and finds an unpaid sale, docs, full `make test` | — | — |

Order is dependency: T1 before T3 (the panel cannot show what the API does not return), T2
before T3 (a panel needs rows to open from). T3 is the only one that touches `DESIGN.md`'s
rules, and it is where the review should look hardest.

## Log

L1 (owner, verbatim): "vamos con la pantalla de cobros y pagos"

L2 (owner, verbatim): "Prefiero que creemos dos paginas pero una de CLientes que hoy no
existe, a la que se pueda hacer click al cliente y abra un side a la derecha que muestre 3
cuadros, el principal los datos del cliente, el segundo el saldo y ahi el boton para hacentar
el pago y el tercer cuado que muestre las ventas impagas. La segunda pagina va a ser de
proveedores espejo de la de clientes. Si por ahi es mucho codigo separamos en un slice inicial
y despues seguimos con el A y B"

L3 (owner, decision): **applying one payment to several debts is a new endpoint**, not a loop
over the existing `POST /api/settlements`. That endpoint requires an `obligation_id`, so today
a collection is always against one named sale. The new one is slice 2, and this slice does not
anticipate its shape.

L4 (owner, decision): **both balance figures are shown, never netted.** Asked whether to show
one figure or two, the owner chose two. The reason is in S3 and it is not tidiness: 1000 owed
and 500 in favour are two facts, and one nett figure loses both.

L5 (owner, decision, and mine to make explicit): the **payment button is not in this slice.**
The owner's description of the panel includes it, and it is deliberately deferred one slice so
that the button lands with the endpoint that can act on it. A button that can only collect
against a single sale would need rework the moment L3's endpoint exists.

L6 **What already exists, measured before planning**, because it changed the size of the slice:
`GET /api/customers` and `GET /api/suppliers` exist, and `frontend/src/lib/api.ts` already has
`listCustomers()` and `listSuppliers()`. What does **not** exist is `/customers` or
`/suppliers` in the frontend (no page at all), any `GET` for `obligations`, `payables`,
`payments` or their settlements, and any side-sheet component other than
`PurchaseDraft.svelte`'s `.sheet`. The rail has four items (`+layout.svelte:30-33`).

L7 **The deep link's costs, stated before choosing it**, because the owner asked whether it
had any. Two: the panel must fetch on mount and own a loading/error state (which it needs
anyway, because the payment button will refresh the balance after collecting), and an id that
no longer exists needs a visible state. In exchange, the panel is bookmarkable and a cashier
can send the link. Only the id goes in the URL; solapa, scroll and form stay in component
state, so this does not become URL-driven UI by the back door.

## Verification log

(empty — nothing is implemented yet)

## Verification log

V1 **T1, the two account endpoints.** `make test-backend` **143** (was 139; +4), `make test-unit`
52, `make test-e2e` 24, `make test-frontend` 238, `make lint` clean, `make sqlx-check` in sync.

**RED first**: all four tests answered `404` before the route existed. That is the right red —
the endpoint was absent, not broken.

**Both behaviours proven to bite by neutering:**

| guard neutered | the test that failed | what it said |
| --- | --- | --- |
| the two figures netted (`owed - in_favour`, favour zeroed) | `v5_a_fully_settled_sale_leaves_the_unpaid_box_and_a_surplus_lands_in_their_favour` | `owed` read `-50.00` where it must read `0.00`, and the 50 would have vanished |
| the `outstanding > 0` filter dropped | the same test | `unpaid_sales` listed a fully settled sale with `outstanding: "0.00"` |

V1b **A neutering attempt that did not bite, and what it taught.** The first netting test was run
against `v5_a_clients_account_...`, where `in_favour` is `0.00` — so netting changed nothing and
the test passed. Moving the same neutering to the **overpayment** fixture is what made it fail.
The lesson is the one this repository keeps relearning: a guard can only be proven by a fixture
where it changes the answer, and "the test passed" after neutering means the fixture, not the
guard, is the problem.

V1c **`inbound()` was written and removed.** The party's money direction (`in` for a customer,
`out` for a supplier) is real and asymmetric, but it ended up inline in the two favour queries and
the helper was dead code — caught by `clippy -D warnings`, not by review. Deleted rather than
`#[allow(dead_code)]`: an unused method that documents a rule belongs in a comment, which is where
the same statement already lives.

V1d **One module for both mirrors**, `routes/parties.rs`, parameterised by a closed `Party` enum
rather than a string: the two branches read different tables, and a typo in a string would become a
query against the wrong table instead of a compile error. The structs stay separate
(`CustomerAccount` has `owed`, `SupplierAccount` has `owed_to_them`) because a single shared field
name would let a screen read "what we owe" and label it "what they owe".

V2 **T1 reviewed natively — approved**, lineage `review-437f7e5652c641fd`, two lenses (`risk`,
`reliability`), 894 lines, tier medium. Authority burned (`gentle-ai.review-acknowledged/v1`).

**The first lineage of this task (`review-b1f1ea20a0e4419b`) found a CRITICAL and could not close.**
Worth recording, because both halves matter:

- **The finding was real and the refuter corroborated it.** `account_of` read `owed`,
  `in_favour` and the unpaid list in three separate pool statements. A collection landing between
  them could return `owed: "0.00"` above a sale with 40 outstanding — the panel contradicting
  itself, which is the one thing a one-response endpoint exists to prevent. Fixed in `d0a6d05`:
  every read runs in one `REPEATABLE READ READ ONLY` transaction, because the default
  `READ COMMITTED` gives each statement its own snapshot, which is exactly the defect.
- **The cycle could not be closed.** The provider demanded a targeted validation, and
  `capture-validation` rejected the submitted evidence on every attempt — with `--input=<path>` and
  with `--input=-` — as *"admitted targeted-validator evidence does not match its request and
  digests"*, using the exact tokens its own transition printed. Two attempts, no hypothesis left
  that was worth a third. **The correction was verified by reading the immutable candidate tree**
  (`inspect-candidate --operation patch`), not by the provider.

**What closed it instead was a fresh review of the corrected state**, which is a different
candidate (`4eab8565`) and therefore judgeable on its own: approved, four findings, all
`informational`. Two were fixed in `0c1ee1f`; two were left with reasons.

V2b **The finding worth keeping is `R3-002`**, and it is the third time this repository pays for the
same class: `assert_eq!(sales.len(), 2)` over a shared table. A length assertion also passes when a
*different* sale is present — this file plants a `legacy-fixture` customer — so it says nothing about
the two sales the test built. The id lookups beneath it were always the real claim; the count was
decoration that could go red for reasons unrelated to the code.

V2c **A guard can only be proven by a fixture where it changes the answer.** The neutering of the
"never net the two figures" decision passed on the first try, because that fixture's `in_favour` is
`0.00` and netting changed nothing. Moving the same neutering to the **overpayment** fixture made it
fail with `owed: "-50.00"`. "The test passed after neutering" means the fixture is wrong — recorded
as V1b, and it is the same lesson `v4-settlement.md` V6c and `v4-payables.md` V4d carry.

V3 **T2, the two pages and the rail.** `make test-frontend` **242** (was 238; +4),
`make test-e2e` **26** (was 24; +2), backend 144, `make lint` clean.

RED first: the nav unit test failed on a missing module, and the e2e failed on a missing
destination in the rail.

**The e2e was proven to bite** by removing the two rail entries: it fails on
`toBeVisible()` for the `Clientes` link, and the 25 other specs stay green — so it measures
the rail rather than the page existing.

**One component, two pages.** `PartyList.svelte` is shared by `/clientes` and `/proveedores`.
The failure mode it avoids has a number in this repository: `Ledger.svelte` (1696) and
`PurchaseLedger.svelte` (1052) are the same table written twice. The two pages differ in the
endpoint they call and the words they render, and in nothing else.

V3b **A test of mine invented a label, and the app corrected it.** The first e2e asked for the
rail by `name: 'Navegación'`; the rail's `aria-label` is `'Principal'` (`nav_main`). The
assertion failed for the right reason — it was measuring a name that does not exist — and the
fix was to read the dictionary, not to loosen the selector. Recorded because a selector that
"works after I changed it to something vaguer" is how an e2e stops asserting anything.

V3c **Nothing in T2 counts rows.** Both seeds (walk-in, one-off-supplier) are always present and
the database is shared, so a length assertion would say nothing about the fixture — the shape
the T1 review recorded as `R3-002` and this slice refused to add a second time. The e2e finds a
customer by the name it created, and asserts a supplier does **not** appear on the customers
page, which a "the row is there" assertion could never catch.

V4 **T2 reviewed natively — approved**, lineage `review-1817973d81dd8fda`, lenses `readability` +
`reliability`, 526 lines. Authority burned. Seven findings, all `informational`; four were real and
are fixed in `b82a40a`.

R2-001 partiesNav.ts:20-28 · R2-002 clientes/+page.svelte:19-43 (fixed) · R2-003 PartyList.svelte:41
R3-001 parties.spec.ts:38-41 (fixed) · R3-002 parties.spec.ts:51-53 · R3-003 parties-nav.test.ts:25-27
R3-004 clientes/+page.svelte:26 (fixed)

V4b **The substantive finding was `R2-002`/`R3-004`, and it is about a comment, not code.** The page
wrote `?c=<id>` and **nothing read it**; the comment claimed the click was "wired" when all it did
was rewrite the query string. **A comment that overstates is worse than none**, because it is the
reason nobody looks. The page now consumes the parameter and states the pending condition in words.

**And my first fix repeated the same lie**: it rendered `Leyendo…` — "reading" — over a page reading
nothing. Replaced with "el panel de la cuenta llega en la próxima etapa". The lesson is not about
this page: it is that the temptation to fill a gap with a plausible word is exactly what produces
the state the review found.

V4c **`R3-001`, and a rewrite of mine the review's own standard caught.** The e2e pressed
`page.goBack()` to prove `replaceState` — which proves nothing, because after a `replaceState` there
is no entry to go back to. The fix reads `history.length` before and after the click. **My first
rewrite compared that length against itself**, which measures nothing; the corrected version reads it
across the click, and switching `replaceState` to `pushState` now fails the spec.

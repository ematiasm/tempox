# v3-seed-identities — the seeded `credit` account and method keep their identity

Defect found by the owner while using v3. The `credit` account (`…0005`) and its
method (`…0006`) are seeded with fixed ids that the server resolves without a
lookup (`CREDIT_ACCOUNT_ID`, `CREDIT_PAYMENT_METHOD_ID`), but nothing enforced that
the rows holding those ids stayed recognisable: the method could be moved to another
account, and the account could be promoted to the shop's till. Deletion was already
refused and is not part of this work.

Raised as Risk item 1 and reviewed as its own candidate, because it changes what a
migration does to rows the ledger's meaning depends on.

## Specs

The measurement that opened this work, reproduced against a real server on the v3
tree (`feat/v3-frontend-close`), is quoted verbatim in `L2`.

S1 **The seeded credit method never changes account.** `payment_methods …0006`
   keeps `account_id = …0005` for the life of the database. Moving it away leaves
   the credit account with zero methods and puts a method called `credit` on
   another account, while `CREDIT_PAYMENT_METHOD_ID` still names that row — so the
   server's no-lookup resolution and the row's meaning stop agreeing.

S2 **No method may be moved onto the seeded credit account.** `…0005` holds what
   customers owe, and nothing may be attached to it later either: a method on that
   account answers "a customer's debt was settled by this means", which v3 has no
   document for.

S3 **The seeded credit account is never the shop's till.** `…0005` may not hold
   `is_default = true`. `is_default` answers "which account does the shop mean by
   the till", and `getCash()` reads exactly that flag, so promoting `…0005` makes
   the counter show the debt account's balance while a sale with an omitted method
   still posts to `…0003`.

S4 **`S1`–`S3` hold in the database, not only in the API.** A `psql` session
   bypasses `accounts.rs` entirely; the invariant lives in the schema as it already
   does for deletion and for the default-account-needs-an-active-method rule.

S5 **The API refuses with the right status, and writes nothing.** Both refusals are
   `409 Conflict`, because the request is understood-and-refused rather than
   malformed, and a refused write changes no row. Measured before the fix, promoting
   `…0005` while its method was inactive returned **`500`**: `conflict_or_500`
   (`backend/src/db.rs`) maps only `23505`, so every `RAISE EXCEPTION` from a guard
   trigger surfaced as an internal error.

   No response body is promised. The only client, `/accounts`, renders a failed write
   as `accounts_write_failed (409)` — it never reads the body — so a message nobody
   consumes would be decoration, and no handler in this crate returns one today.
   Naming the rule in the UI is recorded as debt in `L5`, not invented here.

S6 **Everything else about both rows stays editable.** `name` on either row, and
   `is_active` on the method, keep their current behaviour: renaming corrects a typo
   because documents store the id, and `is_active` is how the picker is told what to
   offer. This work does not decide whether `credit` should be offered for sale —
   that is `v3-accounts.md`'s seeded-inactive decision and the `customer_collections`
   slice's problem, and it stays open.

## Tasks

| ID | Spec | Route | Commit |
| --- | --- | --- | --- |
| T1 | S1-S4 | RED: integration tests proving the guarded moves, and a new account to keep the existing "unreferenced method may move" case alive | `94702f7` |
| T2 | S1-S4 | GREEN: migration `0016` adding the three guards, with the trigger-present proof block | `94702f7` |
| T3 | S5 | GREEN: `accounts.rs` pre-checks returning `409` for both refusals | `94702f7` |
| T4 | S6 | Existing rename and `is_active` tests still pass unchanged; the schema test also proves both stay editable | `94702f7` |
| T5 | S3 | Screen: `/accounts` stops offering "make till" for the credit account | `b6e1a91` |

T1-T4 landed together in `94702f7` because they are one behaviour: the schema guard,
its RED tests and the status code that reports it. Splitting them would have produced
a commit whose tests fail. The delivered work unit is 185 non-test/non-doc lines
(`0016` plus `accounts.rs`); the remaining lines are the two new tests and the three
migrated ones, which is where the evidence lives.

T5 landed separately in `b6e1a91` on `feat/v3-frontend-close` (PR #81), because it is
a screen change in a different branch and because the backend fix had to reach `main`
first: with #81 merged before #80, a screen that withheld the button would ship
without the server refusal behind it. That ordering is the chained-PR rule the
feature already follows, not a new one.

**T5 has a prerequisite #81 does not carry.** The screen withholds the button on its
own, which is correct for the screen; but the `409` it relies on lives in #80. Between
the two merges, and only then, a client calling the API directly still gets a `500`
for the promotion while its method is inactive. That is the state `main` would be in
for one PR, and it is the reason the merge order in `HANDOFF.md` matters.

## Log

L1 (owner, verbatim): "el primero quiero que te fijes si la cuenta credit esta
sembrada, siempre esta activa y nunca se puede borrar."

L1b (owner, verbatim): "lo mismo para el metodo de pago credit"

L2 The measurement, run against `cargo run` on `:3001` over the development
database. Every probe was reverted and the state restored; the rows below are the
ones that mattered.

```
PATCH /api/payment-methods/…0006 {"account_id":"…0003"}   -> 200   # credit method onto the till's account
PATCH /api/accounts/…0005 {"is_default":true}             -> 200   # after activating the credit method
PATCH /api/accounts/…0005 {"is_default":true}             -> 500   # while the credit method is inactive
DELETE /api/accounts/…0005                                -> 405   # no route: deletion was already closed
psql: DELETE FROM accounts WHERE id = '…0005'             -> ERROR accounts cannot be deleted: …
```

So: seeded, yes. Deletion, already refused in the schema and absent from the router.
"Always active", no — and the owner chose not to settle that now. What was actually
open was **movement and promotion**.

L3 (owner, verbatim): "No sé; primero cerrá sólo el agujero de borrado/movimiento"

L4 The existing test `v3_t0_a_referenced_method_cannot_move_but_an_unreferenced_one_can`
used `…0005` as the *destination* of a legal move, which is exactly what S2 now
forbids. That is the one pre-existing test this work has to change, and changing
its destination to a freshly created account is what keeps the case it was written
for — "an unreferenced method may move" — still covered.

L5 **S5 was wrong when written, and the measurement is why.** It asked for a `409`
whose body names the rule. Reading the only client: `/accounts` reports a failed
write as

```ts
error = e instanceof ApiError ? `${lang.t.accounts_write_failed} (${e.status})` : String(e);
```

— the status code and nothing else. No handler in `backend/src` returns a message
body either, so the body would have had no consumer and the test would have pinned
a shape nobody reads. S5 now asks for the status code and "writes nothing".

What that leaves: a person who gets `(409)` on the accounts screen is not told
which rule refused them. That is real, it is older than this work (the duplicate-name
`409` has the same shape), and it is a screen decision — named debt for the T5-side
work, not something to settle by inventing a wire format here.

L6 **The differential check, measured.** `AGENTS.md` asks for it, and it paid: a
fresh copy of `65bde58` in a scratch directory, both binaries booting their own
fresh database with migrations applied on boot, same five requests.

```
                                         BEFORE   AFTER
PATCH credit method -> cash account      200      409
PATCH credit account is_default          500      409
POST a method onto the credit account    201      409
PATCH rename the credit account          200      200   (unchanged, required)
PATCH deactivate the cash method         500      500   (unchanged, unrelated)
```

The first run of this check reported `500` for the credit-method move in the
`AFTER` column, and that was a **stale binary**, not a defect: `accounts.rs` had
been restored from a backup after the last build. Rebuilding reproduced the table
above. Worth recording because the wrong conclusion was one glance away — "the
trigger answers before the pre-check", which is exactly what the source says cannot
happen.

L7 **The three tests this work had to migrate, and why deleting them was not an
option.** Each used the seeded `credit` account as a *destination*, which S2 and S3
forbid, and each covers a rule that still holds:

| test | what it covered | new destination |
| --- | --- | --- |
| `v3_a_referenced_method_cannot_move_between_accounts` | an unreferenced method is a typo and may move | a fresh account |
| `v3_the_default_account_always_keeps_an_active_method` | the default moves as one transaction ending with exactly one `true`; a second default hits the partial unique index | a fresh account with an active method |
| `v3_t0_moving_the_till_is_a_single_operation_not_a_deadlock` | the move is one legal API call | a fresh account with an active method |

`v3_the_default_account_always_keeps_an_active_method` is the one where the choice
mattered: keeping `credit` as the candidate would have made its "second default"
assertion pass on the *seed* guard instead of the unique index, which is the exact
masking that test's own comment warns about. The candidate moved, and the comment
now says why.

L8 **Proof the new tests bite, both layers.** Neutering each guard in turn and
re-running:

- remove the `RAISE` from `guard_credit_method_identity` →
  `v3_the_credit_seed_keeps_its_identity_in_the_schema` fails on
  "the seeded credit method must never change account";
- remove the two API pre-checks → `v3_the_credit_seed_answers_with_a_cause_not_a_server_fault`
  fails with `left: 500, right: 409`.

So the schema test belongs to the schema guard and the HTTP test belongs to the API
check; neither passes because the other is present.

L9 **Verification.** All three levels, run through `make`, each number read from
the run rather than recalled:

```
make test-unit      -> backend 50 passed; frontend 14 files / 227 cases passed
make test-backend   -> 93 passed; 0 failed   (was 91 before this work; +2 new tests)
make test-e2e       -> 21 passed             (5 workers, 9.3s)
```

The migration is `0016`, applied on boot like every other one; nothing here touches
`0015`, the `DROP`, or the `money_movements` view — so this is not the candidate that
needed its own transaction for that reason, only the one that had to be a candidate
of its own because of what a migration does to seeded rows.

L10 **Slice boundary, recorded because the tool asked.** `feat/v3-backend` is 17
commits and ~6.2k authored lines from `main` — but that is the branch, and it is
already published as PR #80 with its own review ledger; the delivery strategy for
this feature is the two chained PRs, not one commit series. This work unit is
`94702f7`, 185 non-test lines of behaviour plus its evidence. Nothing new is
proposed here: if #80 ever needs to be reviewed as slices rather than as a chain,
that is a decision about the branch, and it was made before this commit existed.

L11 **A mistake made while starting T5, recorded because it could have cost the
review ledger.** Staging T5 with `git rebase feat/v3-backend` on
`feat/v3-frontend-close` rewrote all 19 published commits of PR #81 for a screen
change that needs none of that branch's work. Nothing had been pushed, so
`git reset --keep origin/feat/v3-frontend-close` restored the branch to `6b616bd`
with an empty diff against origin; `git diff` also confirmed the rebase had changed
no content, only hashes. T5 was then done on the published tip.

The rule this repeats, already in `HANDOFF.md` §5, is about correcting a commit once
a review opened authority — but the cost here was different and worth naming: a
rewritten tip is a new target for the provider, and the seals that cover #81's trees
would no longer cover it. Stacking a branch that cannot use the parent's work is not
a rebase; it is two independent fixes.

L12 **T5's evidence.** The spec
`the seeded debt account is never offered as the till` failed first on the button
being present (`Received: 1`) and passes now; reverting the `isDebtAccount` branch
made it fail again with the same message, so it belongs to that guard and not to the
render. `npx playwright test` -> 23 passed (was 21). `eslint` and `svelte-check` clean
(0 errors, 0 warnings).

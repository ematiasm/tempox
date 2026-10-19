# f3-oneshot — tasks

F3 of the six-phase direction in `docs/en/testing.md`, on top of F1 (#56) and F2
(#57). It removes the second process from the HTTP integration tests: instead of
`reqwest` against a backend the harness started on `:3099`, the router runs inside
the test.

Engram topic key: `tempox/odd/f3-oneshot/tasks`.

## Why this is the phase that matters

The 29 tests that speak HTTP are the ones that can desynchronize: the harness starts
a **separate process** with its own `DATABASE_URL`, and the test opens its own pool.
Nothing at runtime keeps those two pointed at the same database; today they agree
because the harness sets both from `TEST_DATABASE_URL`. That is a guard, not a
property, and `docs/en/testing.md` says so.

With `oneshot` the test owns the router, so there is no second process and nothing to
desynchronize.

## Specs

S1. `backend/tests/common/mod.rs` exposes a harness that builds the app from the
   same `AppState` main builds, driven by `#[sqlx::test]`'s pool.

S2. A converted test exercises the **same surface** as before: routing, extractors,
   status codes, and the shape of the JSON body. It does not call handlers directly.

S3. **No conversion is accepted without proving it still bites.** For each converted
   test, the behaviour it covers is broken and the test is observed failing. This is
   the rule `docs/en/testing.md` states, and it matters more here than in F2:
   `oneshot` proves the handler, not the transaction the handler opens.

S4. The slice is cut by domain — `v1`, `v2`, `v1_5`, `v2_5` — one PR per domain.
   This branch does **v1 only** (7 tests), because the whole set is the largest
   phase of the six and this repository already paid for oversized slices.

S5. The remaining HTTP tests keep working unchanged. `scripts/test-stack.sh` still
   starts the harness backend for them, so the two styles coexist until F4.

S6. `--test-threads=1` stays until the tests that read global cash totals are all
   converted. A converted test must not depend on cash that another test moves.

## Tasks

| ID | Task | Specs | Route | Commit |
| --- | --- | --- | --- | --- |
| T1 | `tower` gains `util`; `tests/common/mod.rs` with the harness | S1 | inline | — |
| T2 | Convert the 7 `v1` HTTP tests | S1–S2 | inline | — |
| T3 | Prove each converted test still bites | S3 | inline | — |
| T4 | Verify the whole flow | S4–S6 | inline | — |

## Log

L1 (user, verbatim): "ya hice el merge, podes arrancar con F3"

L2 (evidence, measured): of the 43 integration tests, 29 speak HTTP: `v1` 7, `v2` 11,
`v1_5` 11, `v2_5` 12 by the domain prefix in their names. This branch converts the
`v1` seven and leaves the rest for their own slices.

L3 (evidence, read from the crate): `oneshot` lives in `tower::util`, behind the
`util` feature, which this crate did not enable — `tower = "0.5"` with no features.
So the dependency line changes, and that is the only new dependency cost of F3.

L4 (correction to my own count, measured): `v1` has **4** HTTP tests, not 7. The 7 I
first counted included the 3 pool-only `v1` tests that F2 already converted. The true
split is `v1` 4 · `v1a` 2 · `v1_5` 11 · `v2` 11 · `v2_5` 12, which is 40 tests, plus
the 3 `smoke`/unit-only ones — the point is that the domain prefix alone overcounts.

L5 (the harness): `tests/common/mod.rs` gives a `Client` that clones the router and
sends a request in-process through `oneshot`. Three things it deliberately does not
do:

- it does not connect to anything: `client(pool)` takes the pool `#[sqlx::test]`
  already provided, so "the handler wrote it" and "the test reads it" are the same
  database **by construction**, not by configuration. That is the whole of F3.
- it does not read the environment: the shop's zone is fixed, so a test result cannot
  depend on the machine.
- it does not cover boot: no socket, no `serve()` middleware, no migrations at
  startup. F4 is for that.

L6 (RED observed on the first converted test, and it was not my change): the first
attempt asserted `200` on `POST /api/products`. The handler answers **`201 Created`**,
and the old `reqwest` test only asserted `resp.status() == 201` after the fact, so
nobody had written it down. The `expect_status` helper made the assumption visible.

L7 (RED observed, the S3 evidence): with `StatusCode::CREATED` changed to `OK` in
`sale_of_...`/`product...` handler, the converted
`v1_product_without_initial_stock_starts_at_zero` failed with
`unexpected status; body was: {...}` — the failure names the cause, in-process, with
no harness and no network. Restored, it passes.

A note on how I had to do it: my **first** probe broke a `sqlx::query!` string, and
the compiler answered with an internal error (`verify_ich`). Changing a status code
proves the same thing without touching a macro, which is what a probe should do.

L8 (verification): 46 integration (4 converted here, 25 HTTP still on the harness, 14
pool-only from F2, 3 others), 39 unit, 139 frontend; `fmt` and `clippy -D warnings`
clean, with `delete` explicitly allowed since no `v1` test uses it yet and the later
domains will. The unconverted HTTP tests still pass through the harness, so the two
styles coexist as S5 requires.

L9 (review, lineage review-eade4ce806cb395c, approved, authority burned): tier
**medium**, three lenses, no refuter. Four advisory findings, all non-blocking. One
was closed anyway:

`R1-query-encoding` — the search helper interpolates a name into
`/api/products?q={name}` with no encoding. Dormant today, because `unique` produces
hex, but a name carrying `&` or `+` would silently split the query and the failure
would name the assertion rather than the URL. `unique` now guarantees the property
itself, and `common::encode_query` percent-encodes for the general case. The guarantee
belongs to the helper, not to the caller's knowledge of what `uuid` returns.

The other three are wording in comments and this document.

L10 (state, slice boundary): **563 authored lines in 2 commits**, over the ~400
guidance. Kept as one PR on purpose, same reasoning as F1: the first commit builds the
harness and converts four tests, the second fixes what review found in it. Parting them
would leave a PR with a harness and no consumer, or a fix for a defect that the first
PR has not introduced yet. The four remaining domains are separate branches by design
(S4), so the size does not repeat.

## Slice 2: `v1a` (2026-10-19)

L11 (decision, and it is the one this domain forced): `v1a_sales_list` carried a 2001
date window **and** a `clear_window` helper that deleted those rows before and after.
Both existed because the database was shared — the window avoided colliding with other
tests, and the clear made the absolute counts true. In a per-test database the clear is
a no-op describing a mechanism that cannot happen. Removed, with the comment rewritten
to say why the dates stayed: the boundary rule reads better with concrete instants.

The tail assertion also changed shape: it counted rows *inside the 2001 window* on a
shared ledger, and now counts `SELECT count(*) FROM sales` on a database that holds
only this test's rows. The second is strictly stronger — an exact total rather than a
window into someone else's data.

L12 (RED observed, both tests, and the first two probes were blocked by design):

- Breaking `to` from `<` to `<=` in the sales list query is **not possible** without
  regenerating the sqlx offline cache: `SQLX_OFFLINE=true but there is no cached data
  for this query`. That is the guard working as intended, not an obstacle to route
  around — the cache is what lets the crate compile with no database.
- So both probes moved to plain Rust that feeds the query: adding `+ 1` to the
  computed `offset` made `v1a_sales_list` fail with *"the day's last sale is missing"*,
  and clamping `limit` to `2` instead of `MAX_LIST_LIMIT` made the products test fail.
  Restored, both pass. **A probe should change the input a test controls, not the SQL a
  macro verifies at compile time.**

L13 (verification): 46 integration + 39 unit + 139 frontend; fmt and clippy clean. The
four domains still on the harness (`v1_5` 11, `v2` 11, `v2_5` 12) continue to pass.

L14 (review, lineage review-be90709ec288bdc4, approved, authority burned): tier
medium, two lenses (readability, reliability), no refuter. Four advisory findings, all
non-blocking — and two of them were more serious than their label, so they were fixed:

`R2-stale-ignore-sales` and `R2-stale-ignore-products` pointed at `// Ignored by
default: needs the live stack (backend + db)` still sitting above `#[sqlx::test]`
tests. Both were introduced by my own conversion in this slice: the attribute changed
and the sentence next to it did not. Checking the whole file found **five** such stale
clauses, not two — the three from the earlier `v1` slice had the same problem and the
previous review missed them.

A comment that describes a mechanism which no longer exists is worse than no comment:
it tells the next reader that a test needs a stack it does not need. The rule that
generalises: **when a test's attribute changes, the sentence above it is part of the
change.**

The `R2-verification-domain-count` finding was also right: this document said the
remaining domains were `v1_5` 11, `v2` 11, `v2_5` 12, summing to 34, when the measured
HTTP total after `v1` and `v1a` is 23. The earlier per-domain counts came from the
prefix heuristic that L4 already flagged as overcounting; they are restated here as
measured:

| domain | converted | still on the harness |
| --- | --- | --- |
| `v1` | 4 | — |
| `v1a` | 2 | — |
| `v1_5` | — | 11 |
| `v2` | — | 11 |
| `v2_5` | — | 12 |

Which is 34 tests total, of which 28 speak HTTP and 6 are pool-only or day-only.

## Slice 3: `v1_5` (2026-10-19)

L15 (scope, measured): `v1_5` has 11 tests — 8 that speak HTTP and read the database, 3
that only exercise the shop-day endpoint. This slice converts the 8; the 3 day tests
follow the same pattern and are cheap, but they need no pool, so they are their own
commit.

L16 (shared helpers, and they were the real work): `reversal_fixture`, `cash`,
`post_refund` and `find_sale` all took a `&str` base URL and built a `reqwest::Client`.
They now take the harness `Client`, which is what makes the 8 conversions small. Two
things this exposed:

- A helper that returns `reqwest::Response` returns `common::TestResponse`, so every
  `.json().await.expect(...)` at a call site became `.json().clone()`. A blanket
  regex for that **also rewrote six `reqwest` tests that are not converted in this
  slice** and broke the build. They were reverted line by line — and the reversion was
  **incomplete**: two `.expect` labels in unconverted tests (`"purchase json"` and
  `"return json"`) stayed rewritten to a generic `"json"`, because a revert driven by a
  failing build only reverts what the build notices. The review caught it. The lesson
  this document already states in another form needed sharpening: a mechanical rewrite
  across a file that holds two styles needs the style boundary named first, not a
  pattern broad enough to cross it — **and the undo needs the same boundary, not the
  compiler's error list.**
- `v1_5_credit_note_history_guards_are_enforced` declared `DATABASE_URL` before
  `BACKEND_BASE`, so a fixed-prefix replacement missed it. One more reason the
  conversion was done against the compiler rather than assumed.

L17 (the question this slice existed to answer): **`--test-threads=1` cannot be
relaxed yet, and the reason is not the cash test.** Measured, by running the suite
without the flag:

```text
$ cargo test --test integration -- --include-ignored
test v2_5_a_rejected_return_writes_nothing ... FAILED
test v2_cash_nets_purchases_and_still_counts_sales ... FAILED
      "a rejected return wrote something: document, line or movement survived"
      left: (48, 49, 49)
```

Both failures are `v2` and `v2_5` tests — the two domains **still on the shared
`tempox_test` database**. `v1_5_cash_still_counts...` passed in parallel, because it
now reads the cash of its own database.

So the dependency is ordinal, not architectural: the flag stays until the last domain
that reads global state is converted. That is a much better sentence than "it is
because of cash", and it names which branch removes it.

L18 (RED observed, third attempt and the first two were my own error): the probe
changed `if *already > line.quantity` to `>=` in the credit-note handler. Two tests
failed — `v1_5_cumulative_refunds_stop_at_the_quantity_sold` and
`v1_5_repeated_line_is_decided_on_the_exact_boundary` — which is what those tests are
for. Restored, 46 pass.

The first two attempts used `sed -n '355s/...'` on a line that had moved to 354, and
silently changed nothing: a probe that reports `ok` because it never applied is a
false GREEN. **Verify the probe landed before trusting its result** — print the line,
as here.

L19 (verification): 46 integration (8 converted here, 3 day tests and 20 others still
on the harness), 39 unit, 139 frontend; fmt and clippy clean.

L20 (the day tests, and the second data point on parallelism): the three shop-day tests
need no pool, but they go through the same harness, so they were converted with it.
After them all **11** `v1_5` tests are in-process.

Re-measuring parallelism is what makes L17 a finding rather than an anecdote:

| state | fails without `--test-threads=1` |
| --- | --- |
| before `v1_5` | `v2_5_a_rejected_return_writes_nothing`, `v2_cash_nets_purchases_and_still_counts_sales` |
| after `v1_5` | `v2_cash_nets_purchases_and_still_counts_sales`, `v2_a_rejected_purchase_writes_nothing` |

Two failures both times, but different ones: converting `v1_5` removed the `v2_5`
failure. The remaining pair is entirely `v2`. That is the ordinal dependency L17
describes, measured twice instead of once — and it predicts that converting `v2`
removes the flag's last justification, which is a claim the next slice can falsify.

## Slice 4: `v2` (2026-10-19)

L21 (scope): `v2` has 11 tests — 5 speak HTTP and 6 are pool-only (converted in F2).
This slice converts the 5. Two helpers were involved, and one of them belongs to
`v2_5`, which is the trap this slice fell into (L22).

L22 (mistake, caught by the compiler and by reading): converting
`v2_5_create_purchase` looked like part of the same job, because `v2`'s tests create
purchases too. It is not: all seven of its call sites are `v2_5` tests, still on the
harness. Changing it broke seven tests that are not in this slice. Reverted. The same
happened with a local `cash` helper inside `v2_5_cash_...` — two helpers with the same
name, one per slice, and a global rename does not know that.

**A shared helper name is not a shared helper.** Before changing one, look at who calls
it; a name that appears in the file twice may be two functions.

L23 (the prediction, and it did not hold): the previous slice predicted that converting
`v2` would remove the flag's last justification. First measurement agreed — one run
without `--test-threads=1` passed, in 2.9s instead of 13.8s. Then three runs disagreed:
**one green, two red.** The failing test is always the same:

```text
thread 'v2_5_cash_adds_returns_back_and_still_counts_sales_and_purchases' panicked:
assertion `left == right` failed: a return raises the till by what the supplier gave back
  left: 0
 right: 1000
```

That test is **not converted**: it calls `/api/cash` over the harness and reads a
**global** total, while the converted tests beside it write to their own databases. It
sees 0 where it expects 1000 because the refund it just created is not in the total it
read — the two are different databases.

So the ordinal dependency was real but I mis-stated its terminus: it is not "the last
domain converted", it is **the last test that reads global state**, and one of those is
in `v2_5`. The prediction was falsified by a flaky run rather than confirmed by a single
green one, which is the reason three runs were worth more than one.

With the flag: three consecutive runs, 46 passed each time.

L24 (verification): 46 integration + 39 unit + 139 frontend; fmt and clippy clean.

## Slice 5: `v2_5`, and the flag finally goes (2026-10-19)

L25 (inventory, measured before touching anything): `v2_5` has nine tests, not the
twelve the domain-prefix count produced. Four were already pool-only (`schema_creates_*`,
`the_ledger_refuses_*`, `the_return_tables_are_immutable`, `the_pre_check_runs_*`);
five spoke HTTP. Two helpers needed converting (`v2_5_create_purchase`, `v2_5_product`),
and `v2_5_fixture` was already transactional and untouched.

L26 (the fix the previous slice's mistake pointed at): this time I read the call sites
first. Both helpers are used **only** by `v2_5` tests, which is what made them safe to
convert — the opposite of the `v2` slice, where the same helper name belonged to a
later slice. Reading first cost one grep; not reading cost two reverts.

L27 (RED, in plain Rust, per the lesson about `SQLX_OFFLINE`): `backend/.sqlx/` is
populated and `SQLX_OFFLINE` is on, so breaking a `sqlx::query!` string yields a cache
error rather than a failing test — a false signal. The probe instead changed a decision
the build does not verify: `let line_total = line_totals[idx];` became
`Decimal::ZERO` in `create_supplier_return`.

Observed RED, exactly one test:

```text
test v2_5_a_return_takes_stock_out_and_returns_the_money_paid ... FAILED
assertion `left == right` failed
  left: String("0.00")
```

One finding worth recording: **only one of the five failed.** `v2_5_cash_...` stayed
green because it asserts deltas of 1000 and 4000 against a return worth 10.00 — the
return total comes from `supplier_returns.total`, written from `return_total`, not from
`line_totals`. So the probe broke a value the list test and the money test both read,
and the cash test never touches. A suite where every test failed for one change would
mean the tests are not measuring different things; this one gates each fact separately,
which is what makes it worth trusting.

Reverted; `git diff backend/src/` empty before running GREEN.

L28 (the flag, measured properly this time): **ten consecutive runs without
`--test-threads=1`, all 44 passed**, in 2.9s instead of 15s. The previous slice's
mistake was trusting one green run; this slice's rule is that the number has to survive
repetition before an instruction is removed from the build.

So the flag came out of all three places that carried it:

- `Makefile` — `test-backend` no longer passes it, with a comment saying why.
- `.github/workflows/ci.yml` — the integration step no longer passes it. Leaving it
  here while the Makefile dropped it would have been an inconsistency introduced by
  this very slice: the documented reason and the executed command would disagree.
- `docs/en/testing.md` — the flakiness section and the CI section both described the
  flag as current. Both now describe what is true: each test owns its database, and a
  test that reads global state through something other than its own pool reintroduces
  the problem.

That last bullet is the one I nearly missed. Removing a flag from a Makefile is a
two-line change; removing the *sentence that explains it* from three files is the part
that keeps the next reader from re-adding it.

L29 (verification): `make test` 46 integration + 39 unit + 139 frontend + 16 e2e, with
the flag gone and the suite at 2.88s. `cargo fmt --check` and
`clippy --all-targets -D warnings` clean. Two tests remain on the harness by design —
`health_endpoint_returns_ok` and `notes_crud_roundtrip` — and both need a real server,
which is F4's job.

L30 (a correction the review earned, and it was mine): the first version of this slice
removed `--test-threads=1` from `ci.yml` and the `Makefile` outright, and
`docs/en/testing.md` was rewritten to say each test owns its database. The readability
lens flagged the `ci.yml` hunk as a warning, and it was right about the substance even
though the finding was advisory:

The `--ignored` selection in CI does **not** contain converted tests. It contains
`health_endpoint_returns_ok` and `notes_crud_roundtrip`, and neither has a
`#[sqlx::test]` attribute. Both read `BACKEND_BASE` and talk to the single database
the job started. Removing the flag there, while writing a comment saying they own
their databases, made the file assert something false about the two tests it was
running — and the comment three lines above it already said "They are serialized on
purpose". Two adjacent comments disagreeing is the kind of thing a reader hits before
the reviewer does.

The honest shape: `--test-threads=1` is still needed, for a **narrower and different**
reason than the one documented for years. It is needed by the two live-server tests
sharing one database, not by the cash tests reading a global total. The ten parallel
runs remain valid and remain the evidence for the converted domains — they are simply
not evidence about those two.

All three files were corrected to say that, and `make test` re-run green.

The lesson is not "restore the flag". It is that **removing a mechanism and removing
its explanation are different edits, and only the first one is safe to do by search and
replace.** The sentence I wrote to replace the old explanation was read by the reviewer
as a claim about the code; it was false, and it survived my own verification because I
verified the tests, not the comment.

L31 (slice boundary): this change is 431 authored lines in one commit, past the ~400
guideline. It stays one commit on purpose. The code and the three files that explain it
are one fact — "the flag is needed for the live-server pair, not for the cash total" —
and splitting the tests from the sentences describing them is exactly the edit
separation that produced L30. A reader of `ci.yml` alone would otherwise see a flag
with no tests behind it, or tests with no explanation.

L32 (the approval no longer matches the tree): `review-4be5c9ac3105602f` approved the
candidate that *removed* the flag from `ci.yml`. The correction that restored it came
after acknowledgement, so the authority is burned and the tree has moved. The approved
receipt therefore describes a candidate that is not the one being delivered. A fresh
review is required, and the previous receipt is recorded here as history rather than
reused as cover.

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

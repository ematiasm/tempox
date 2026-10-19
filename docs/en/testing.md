# Testing — Tempox

This document is the single authority for how tests run in this repository. Other
files link here instead of restating it. Every number, port and path below was
measured on 2026-10-18, not recalled.

The rule this document replaces lived only in an agent's memory and in ODD logs.
That is why it failed twice. It lives in a file now.

## The three levels

| Level | Count | What it needs running | What it **cannot** see |
| --- | --- | --- | --- |
| Unit (backend) | 29 | nothing | the database, the router, a real order of operations |
| Unit (frontend) | 139 cases in 9 files | nothing | the API, the DOM as the browser builds it |
| Integration | 43 | a postgres container and a backend on `:3001` | whether the screen renders |
| End to end | 16 cases in 4 specs | postgres, a backend, chromium | why a row is missing |

```bash
make test-unit        # backend + frontend, nothing running
make test-backend     # integration, owns :3001 briefly
make test-e2e         # Playwright, builds and serves its own preview
make test             # all three
```

### Why each level exists

They are not a ladder from cheap to expensive. Each one covers something the
others structurally cannot.

- **Backend unit tests** (`cargo test --bins`) cover pure logic that needs no I/O:
  money arithmetic, whole-cent rounding, the shop's calendar day, sale totals.
- **Frontend unit tests** (Vitest, jsdom) cover pure derivations: amount
  formatting in the shop's own shape, the ledger's arithmetic, API client request
  construction with `fetch` stubbed. They never touch the network.
- **Integration tests** exist because **the database is the product**. Seventeen
  append-only triggers enforce that stock may only be corrected by a compensating
  row, never by an edit. No unit test can see a trigger, and a handler can be
  perfectly correct while the schema silently permits an edit. They drive the real
  handlers and the real schema.
- **End-to-end tests** exist because **the stamp is invisible to everything else**.
  A recorded sale wearing `Confirmada` is a rendered fact: the browser, the
  compiled bundle and the real API all have to agree. That is what a Playwright
  spec covers and what no unit or integration test can.

## Two databases, one stack

| Database | Who writes it | Who reads it |
| --- | --- | --- |
| `tempox` | you, through the app | the app, adminer |
| `tempox_test` | `make test-backend`, `make test-e2e` | nobody |

Both live inside `tempox-postgres-1`, on the same `pgdata` volume, on the same
`postgres:18-alpine` image. There is no second stack, no second container and no
second image.

### Why not the development database

The ledger is append-only by trigger. There is no `DELETE` for a sale, a movement
or a purchase — the guards reject it. The only reset is `docker compose down -v`,
which discards the whole volume, including your own data. A test that writes to
`tempox` therefore leaves rows that **cannot be removed individually**.

Two failures this repository already paid for came from sharing it:

1. `v1_5_cash_still_counts_every_confirmed_sale_and_nothing_else` passed locally
   and failed in CI, because it read a global cash total that local runs had
   perturbed. CI starts from an empty database.
2. Four specs in PR #55 selected ledger documents by position, by a derived total,
   and by the wall clock. All three are lies when other specs write to the same
   ledger.

Both are the same defect: **a test that assumes pre-existing state**. A separate
database does not make that defect impossible, but it removes the state that
hides it.

### Where the connection strings live

This was the confusing part, so it is written down explicitly. Four places read a
database URL, for two different purposes:

| Consumer | Where the value comes from | Which database |
| --- | --- | --- |
| The `backend` container | `docker-compose.yml`, composed from `POSTGRES_*` | `tempox` |
| `cargo run` on the host | `.env` via `dotenvy::dotenv()` | `tempox` |
| `make test-backend` | `.env`: `TEST_DATABASE_URL` | `tempox_test` |
| `make sqlx-prepare` | `.env`: `DATABASE_URL` | `tempox` |

**The container never reads `.env` for `DATABASE_URL`.** Compose sets it
explicitly in the service's `environment:`, and `.env` is used only to substitute
`${POSTGRES_*}`. Changing `.env` therefore does not move the container, and editing
`docker-compose.yml` does not move `cargo run`. They are separate paths that
happened to agree.

`DATABASE_URL` in `.env` uses `@localhost:5432` because it is **exclusively the
host's**, which is what makes `cargo run` work with no exported variables.

## The development loop

Postgres and adminer stay in Docker. Backend and frontend run on the host, the same
way `npm run dev` already ran the frontend.

```bash
docker compose up -d postgres adminer     # the database, in Docker
cd backend && cargo run                   # API on :3001, from .env
cd frontend && npm run dev                # UI on :5173, hot reload
```

To run the backend from Docker instead, stop the host one first — they cannot both
hold `:3001`:

```bash
cd backend && cargo run &                 # development: the branch's code
# later
kill %1
docker compose start backend
```

### Why `cargo run` and not the image

|  | `cargo run` | rebuilding the image |
| --- | --- | --- |
| Compile feedback | seconds, incremental | minutes, full `--release` graph |
| Touches `tempox-backend:latest` | no | **yes, silently** |
| Orchestration in the loop | none | `build` + `up`, then undo |

It is also what CI does: `ci.yml` starts `./target/debug/tempox`, never the image.

### The one thing only the image can prove

An image can go **stale**, and a stale image fails in a way no other check
reproduces. On 2026-10-17 the image had been built with `migrations/` ending at
`0008` while the volume carried `0009`, and the container refused to start with:

```text
Error: migration 9 was previously applied but is missing in the resolved migrations
```

`cargo run` **cannot** produce that: it compiles fresh, embeds the current
migrations, and applies them on boot.

So rebuilding the image is a **delivery check**, not a development step: do it once,
before shipping, when the branch is the state you intend to deliver. Note that
**CI never builds the backend image** — only the frontend one — so a green CI says
nothing about `tempox-backend:latest`.

## Writing an integration test that does not lie

The rule, in one line: **create the artifact you measure, and find it by identity.**

- **Never assume pre-existing rows.** Each test creates the product, customer or
  supplier it needs, through the same interface a user would.
- **Never select by position.** `locator(...).first()` means "the newest document",
  which is yours only if nothing recorded one since.
- **Never select by a derived value.** A total is not unique: `100.00 x 7` and
  `175.00 x 4` both come to `700.00`. Both draft screens expose `data-doc-id` after
  recording; read your own document's id and find exactly that row. See
  `frontend/tests/e2e/unique.ts` for the long version and the two failed attempts
  that led to it.
- **Never seed from the wall clock.** Use run entropy: two CI runs in the same
  second are a real scenario.
- **Assert every observable effect of the rule you are covering**: the status code,
  the exact body, and that a refused input left stored data and counters unchanged.

### A migrated test that does not bite is worse than the original

If an integration test is ever converted to a new harness, it has to prove it still
fails when the behaviour it covers breaks: invert the sign of a movement, remove the
`BEGIN` from a transaction, drop the `WHERE` from a query, and watch the new test
fail with a message that names the cause. This repository already treats that as the
standard (`odd/tasks/v2.5-supplier-returns.md`); it applies to any refactor of the
suite as well.

## What makes a test flaky here

- **Parallel writes to one database.** No longer possible in the backend suite, and
  the reason is structural rather than a flag.

  Every test runs on the database `#[sqlx::test]` creates for it. The ones that need
  a real socket — `health_endpoint_returns_ok`, `notes_crud_roundtrip`, and the F4
  migration proof — start their own binary on an ephemeral port against that same
  database, so two of them cannot meet. `make test-backend` therefore passes neither
  `--test-threads=1` nor an external backend.

  If you add a test that reads global state through something other than its own
  pool, you have reintroduced the problem. Convert it, do not serialize it.
- **A leftover backend on a fixed port.** Neither suite has this failure mode any
  more, and the two levels solved it differently because they own different things.

  The backend suite binds port `0` and reads the real address from the binary's own
  log, so there is no port to collide over. The e2e level cannot do that: it runs one
  long-lived backend and one long-lived frontend, and the frontend is a **compiled
  artifact** — `vite` inlines `PUBLIC_API_BASE` at build time (measured: a build given
  `PUBLIC_API_BASE=http://probe-1234` contains that literal in
  `_app/immutable/chunks/*.js`). So the URL must be known before the build runs, which
  rules out discovering it.

  Playwright therefore starts the backend on the fixed `:3099` and owns its lifetime,
  replacing the external `scripts/test-stack.sh` backend.

  That backend entry sets `reuseExistingServer: false` in every environment, which is
  the opposite of the preview entry and deliberate. Reusing a server on `:3099` would
  adopt a backend from an earlier run, a different branch, or another database, and the
  suite would pass against code the candidate does not contain. `test-stack.sh` refused
  to start on an occupied port for that reason, and F5 keeps the refusal rather than
  trading it for convenience. An occupied `:3099` therefore fails the run, naming the
  URL.
- **A leftover preview on `:4173`.** `reuseExistingServer` is on outside CI, so a
  stale preview is reused. Stop it before trusting an e2e result.
- **Running a suite directly.** `npx playwright test` on its own reaches
  `localhost:3001` — the development backend — because `frontend/src/lib/api.ts`
  defaults to it and cannot tell a test from a user. The suites that write must go
  through `make`, which points them at the test backend. This repository has
  already paid for this shortcut once, with 5 sales, 11 products and 22 movements
  in the development database that no `DELETE` can remove.
- **A stale test database.** The schema applies on the backend's boot, so a
  unit-only run migrates nothing. `make test-db` recreates it when the schema has
  moved.

## Resetting the test database

`tempox_test` is disposable. Unlike `tempox`, it needs no permission from anyone:

```bash
make test-db          # drop and recreate, then migrate on the next boot
```

`sqlx::migrate!` applies the migrations when the backend starts, so the test
harness migrates `tempox_test` for free and `make test-db` only has to recreate it.
Neither touches `tempox`. A clean **development** database is still the owner's
decision: it is `docker compose down -v`, and it takes the volume.

## What CI does, and how it differs

CI has no development stack, so it builds its own:

- **Backend job**: a `postgres:18-alpine` service, then one run of
  `cargo test --locked --all-features`. No thread limit and no separately started
  server: every test creates its own database, and the tests that need a socket start
  `CARGO_BIN_EXE_tempox` themselves on an ephemeral port. The job applies migrations
  once only so `cargo sqlx prepare --check` has a schema; the tests do not depend on
  that database.

  CI never builds the backend image — see the note above about a green CI saying
  nothing about it.
- **Frontend job**: lint, `svelte-check`, Vitest, build.
- **Image job**: builds the frontend image and asserts it serves `/sales`.
- **E2E job**: the same ephemeral shape, with the backend started by the job, then
  Playwright.

That is the shape **of CI**, which owns its own services. It is not a pattern to
copy locally, and copying it is what produced throwaway stacks and rebuilt images
here.

## Direction: making the guard unnecessary

The guard exists for one reason:

> Something else owns what the test needs to control. Change the owner and the guard
> goes away.

Today `docker-compose` owns the backend, so a test must either use that one or
provoke a second one. The target architecture assigns ownership explicitly:

| Level | Proves | Owns the process | Owns the database | Desync possible? |
| --- | --- | --- | --- | --- |
| T1 unit | pure logic | nothing | nothing | n/a |
| T2 handler, in-process | routing, extractors, status, body, effects | the test, same process | the test, fresh per test | **no** |
| T3 boot and reality | migrations on boot, real TCP, `serve()` middleware | the test, via `CARGO_BIN_EXE_tempox` | the test, fresh per test | **no** |
| T4 end to end | the real screen in a browser | Playwright | an external backend | **yes — the guard stays, permanently** |

T2 is `tower::oneshot`: the router runs inside the test, with no socket and no
second process. It keeps the public surface and removes the desync. T3 is the real
binary, which is the only level that covers boot, migrations and `serve()`.

### The six phases

Ordered by dependency. **Only the prerequisites are documented here; no phase is
authorized.**

| Phase | Work | Size |
| --- | --- | --- |
| **F1** | The seam: `src/lib.rs` with `build_app(state) -> Router` and `Config::from_env()`; `main.rs` keeps boot only. `BACKEND_PORT=0` reports the real port. **Touches product code.** | ~150 lines |
| **F2** | `tests/common/mod.rs` harness on `#[sqlx::test]`; converts the 14 pool-only tests. `--test-threads=1` stops being needed for those. | per domain |
| **F3** | `tower::oneshot` for the 29 HTTP tests, including the 23 that use both. Removes the desync. | per domain |
| **F4** | **Done.** T3: the test spawns `CARGO_BIN_EXE_tempox` on `BACKEND_PORT=0` against its own database, with `migrations = false` so boot's migration step is what gets proven. `docker-compose` no longer needs a backend for tests. | small |
| **F5** | **Done.** E2E: Playwright's `webServer` starts the backend on its own database, in array order, and owns its lifetime. `PUBLIC_API_BASE` still comes from the target, because `webServer.env` cannot reach the build. | small |
| **F6** | Retire the old plumbing: the 38 hardcoded URLs, the `Makefile` health guard, the `sqlx-prepare` rewrite, and **make a direct suite invocation unable to reach the development backend** (`api.ts` currently defaults to `:3001`, so a bare `npx playwright test` writes to it). **Last, not first.** | cleanup |

Cut each phase by domain, which is how the tests are already named: `v1` (7),
`v1a` (2), `v1_5` (11), `v2` (11), `v2_5` (12). An old test is deleted only once its
replacement is green and proven to bite.

### Measured constraints

- `tower 0.5.3` is already in `Cargo.lock`; `oneshot` needs only
  `features = ["util"]`. `axum 0.8`. **There is no `src/lib.rs` today** — F1 is real
  work, not a rename.
- Role `tempox` has `rolcreatedb`, so `#[sqlx::test]` can create databases.
- `max_connections = 100`, shared with the development stack. 43 tests each holding
  a pool will exhaust it before it exhausts the 12 cores. Cap `--test-threads`.
- `#[sqlx::test]` reads `DATABASE_URL`, so `TEST_DATABASE_URL` is mapped at the
  command boundary. The variable does not change name or shape across the
  migration: what changes is who reads it.

### Why the harness is worth doing first anyway

The harness is plumbing, not architecture, and this document says so plainly so
nobody reads "the harness solves it" and stops. It is worth building now because:

1. It removes the desync for the **29 tests that speak HTTP — 67% of the real
   problem** — at a cost of one script and two Makefile targets.
2. `TEST_DATABASE_URL` is the input F2–F5 consume unchanged.

Starting the process with a known `DATABASE_URL` and reaching `/health` is already
the proof that it connected: a missing or unreachable database stops the binary at
boot, in `main.rs`, before it can serve anything. That is why no `/health` change is
needed, and why the guard is a property of how the harness starts the process
rather than an endpoint that reports on it.

It became unnecessary in F4, and only for the levels the test owns — which is now
every backend level. The e2e need for an external backend stays for good.

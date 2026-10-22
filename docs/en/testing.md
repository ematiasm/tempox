# Testing — Tempox

This document is the single authority for how tests run in this repository. Other
files link here instead of restating it. Every number, port and path below was
measured on 2026-10-19, after the six phases below had landed, not recalled.

The rule this document replaces lived only in an agent's memory and in ODD logs.
That is why it failed twice. It lives in a file now.

## The three levels

| Level           | Count                | What it needs running | What it **cannot** see                               |
| --------------- | -------------------- | --------------------- | ---------------------------------------------------- |
| Unit (backend)  | 39                   | nothing               | the database, the router, a real order of operations |
| Unit (frontend) | 139 cases in 9 files | nothing               | the API, the DOM as the browser builds it            |
| Integration     | 47                   | a postgres container  | whether the screen renders                           |
| End to end      | 16 cases in 4 specs  | postgres, chromium    | why a row is missing                                 |

```bash
make test-unit        # backend + frontend, nothing running
make test-backend     # integration: a database per test, no external backend
make test-e2e         # Playwright starts the backend, builds and serves its own preview
make test             # all three
```

No level needs a backend someone else started. `make test-backend` gives every test
its own database and lets the tests that need a socket spawn the binary themselves;
`make test-e2e` has Playwright own the long-lived backend on `:3099`. The development
backend on `:3001` is never stopped, and nothing in a test run points at it.

### Why each level exists

They are not a ladder from cheap to expensive. Each one covers something the
others structurally cannot.

- **Backend unit tests** (`cargo test --lib --bins`, as `make test-unit` runs it)
  cover pure logic that needs no I/O: money arithmetic, whole-cent rounding, the
  shop's calendar day, sale totals. Both flags matter: since F1 the tests live in
  the library target, and `--bins` alone reports "0 passed" and exits 0 — a green
  that proves nothing (`odd/tasks/f1-config-seam.md`, L10).
- **Frontend unit tests** (Vitest, jsdom) cover pure derivations: amount
  formatting in the shop's own shape, the ledger's arithmetic, API client request
  construction with `fetch` stubbed. They never touch the network.
- **Integration tests** exist because **the database is the product**. Seventeen
  triggers — append-only, truncate and coherence guards alike — enforce that stock
  may only be corrected by a compensating row, never by an edit. No unit test can
  see a trigger, and a handler can be perfectly correct while the schema silently
  permits an edit. They drive the real handlers and the real schema.
- **End-to-end tests** exist because **the stamp is invisible to everything else**.
  A recorded sale wearing `Confirmada` is a rendered fact: the browser, the
  compiled bundle and the real API all have to agree. That is what a Playwright
  spec covers and what no unit or integration test can.

## Two databases, one stack

| Database      | Who writes it                        | Who reads it     |
| ------------- | ------------------------------------ | ---------------- |
| `tempox`      | you, through the app                 | the app, adminer |
| `tempox_test` | `make test-backend`, `make test-e2e` | nobody           |

Both live inside `tempox-postgres-1`, on the same `pgdata_dev` volume, on the same
`postgres:18-alpine` image. There is no second stack, no second container and no
second image.

The deliverable stack declares `pgdata_prod`, so running
`docker compose -f docker-compose.prod.yml up` on a developer machine gets its own
empty volume instead of writing into this ledger — which is append-only and cannot
be undone row by row.

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

| Consumer                 | Where the value comes from                        | Which database |
| ------------------------ | ------------------------------------------------- | -------------- |
| `cargo run` on the host  | `.env` via `dotenvy::dotenv()`                    | `tempox`       |
| The production `backend` | `docker-compose.prod.yml`, composed from `PROD_*` | `tempox`       |
| `make test-backend`      | `.env`: `TEST_DATABASE_URL`                       | `tempox_test`  |
| `make sqlx-prepare`      | `.env`: `DATABASE_URL`                            | `tempox`       |

There is no `backend` service in `docker-compose.yml` any more: development runs
it with `cargo run`, and the containerized backend lives only in the deliverable
stack.

**A container never reads `.env` for `DATABASE_URL`.** Compose sets it explicitly
in the service's `environment:`. Changing `.env` therefore does not move a
container, and editing a compose file does not move `cargo run`. They are separate
paths that happened to agree, which is why the table above names both.

`DATABASE_URL` in `.env` uses `@localhost:5432` because it is **exclusively the
host's**, which is what makes `cargo run` work with no exported variables.

## The development loop

Postgres and adminer stay in Docker. Backend and frontend run on the host, the same
way `npm run dev` already ran the frontend. `docker-compose.yml` contains exactly
those two services now, so naming them is optional.

```bash
docker compose up -d                      # the database and adminer, in Docker
cd backend && cargo run                   # API on :3001, from .env
cd frontend && npm run dev                # UI on :5173, hot reload
```

No API origin is exported: the browser calls its own origin and the server reads
`BACKEND_URL`, which defaults to `http://localhost:3001` outside production.

The backend is not a service in `docker-compose.yml` any more. It runs with
`cargo run`, which is what makes the edit loop fast, and the deliverable stack
(`docker-compose.prod.yml`) is where a built backend image exists. Running both at
once is impossible anyway: they would both hold `:3001`.

### Why `cargo run` and not the image

|                                 | `cargo run`          | rebuilding the image            |
| ------------------------------- | -------------------- | ------------------------------- |
| Compile feedback                | seconds, incremental | minutes, full `--release` graph |
| Touches `tempox-backend:latest` | no                   | **yes, silently**               |
| Orchestration in the loop       | none                 | `build` + `up`, then undo       |

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
  long-lived backend and one long-lived frontend, so the port has to be agreed before
  either starts. That is `:3099`.

  Playwright therefore starts the backend on the fixed `:3099` and owns its lifetime.
  The external `scripts/test-stack.sh` that used to do this had no caller left once both
  suites owned their own process, and was removed in F6.

  That backend entry sets `reuseExistingServer: false` in every environment, which is
  the opposite of the preview entry and deliberate. Reusing a server on `:3099` would
  adopt a backend from an earlier run, a different branch, or another database, and the
  suite would pass against code the candidate does not contain. The removed
  `test-stack.sh` refused to start on an occupied port for that reason, and F5 kept the
  refusal rather than trading it for convenience. An occupied `:3099` therefore fails
  the run, naming the URL.

  A third guard refuses `TEST_BACKEND_PORT=3001` by name: that is the development
  backend's port, and starting the e2e backend on it would make every check agree with
  the wrong database.

- **A leftover preview on `:4173`.** `reuseExistingServer` is on outside CI, so a
  stale preview is reused. Stop it before trusting an e2e result.
- **Running a suite directly.** The dangerous half is gone; the inconvenient half
  is not, and the difference is worth knowing precisely.

  What F6 closed was a build-time value that could point at development. That value
  no longer exists: `PUBLIC_API_BASE` and `REQUIRE_PUBLIC_API_BASE` were **retired**
  when the browser started calling its own origin and the server began reading
  `BACKEND_URL` per request (`odd/tasks/docker-prod.md`, S9 and S10). The guarantee
  that remains is F5's, and it is a different mechanism: the e2e starts its own
  backend on `:3099` against `tempox_test` and refuses `:3001` by name.

  Measured on 2026-10-19, the shapes were —

  | invocation              | result                                                          |
  | ----------------------- | --------------------------------------------------------------- |
  | no `TEST_DATABASE_URL`  | `error with configuration: relative URL without a base`, exit 1 |
  | `TEST_DATABASE_URL` set | lists and runs; the proxy's backend comes from the harness      |
  | `make test-e2e`         | sets the database, and is the supported path                    |

  The first row fails because the backend entry hands the binary an empty
  `DATABASE_URL`, not because of the API origin. `playwright.config.ts` supplies the
  preview's `BACKEND_URL` from its own `:3099`.

  So `TEST_DATABASE_URL` is what the harness still owns, and a bare invocation that
  supplies it runs against the test database. Nothing here can reach `tempox`. That
  is a weaker promise than "shout at anyone who skips the Makefile", and it is the
  accurate one.

  The shortcut this repository already paid for was different and cannot recur: 5
  sales, 11 products and 22 movements written to `tempox` while `api.ts` defaulted
  to `:3001`.

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
- **Image job**: builds the **frontend** image and asserts it serves `/sales`. It is
  the only artifact CI covers; the backend image is deliberately not built here.
- **E2E job**: its own `postgres:18-alpine` service and a `tempox_test` database, then
  `cargo build` for the binary Playwright will start, then `npx playwright test`.
  Playwright starts both servers itself — the backend on `:3099` and the preview on
  `:4173` — so the job has no separate server step, and no API origin to set: the
  preview is handed `BACKEND_URL` by `playwright.config.ts`.

That is the shape **of CI**, which owns its own services. It is not a pattern to
copy locally, and copying it is what produced throwaway stacks and rebuilt images
here.

## Direction: how the guard became unnecessary

The guard existed for one reason:

> Something else owns what the test needs to control. Change the owner and the guard
> goes away.

`docker-compose` used to own the backend, so a test had to either use that one or
provoke a second one on `:3001` and then verify it was talking to the right
database — the check the six phases exist to retire. Each level now owns what it
measures:

| Level                  | Proves                                             | Owns the process                              | Owns the database        | Desync possible?                                                                              |
| ---------------------- | -------------------------------------------------- | --------------------------------------------- | ------------------------ | --------------------------------------------------------------------------------------------- |
| T1 unit                | pure logic                                         | nothing                                       | nothing                  | n/a                                                                                           |
| T2 handler, in-process | routing, extractors, status, body, effects         | the test, same process                        | the test, fresh per test | **no**                                                                                        |
| T3 boot and reality    | migrations on boot, real TCP, `serve()` middleware | the test, via `CARGO_BIN_EXE_tempox`          | the test, fresh per test | **no**                                                                                        |
| T4 end to end          | the real screen in a browser                       | Playwright, one long-lived backend on `:3099` | the test, `tempox_test`  | **no — Playwright starts the backend itself**, so there is no other process to be wrong about |

T2 is `tower::oneshot`: the router runs inside the test, with no socket and no
second process. It keeps the public surface and removes the desync. T3 is the real
binary, which is the only level that covers boot, migrations and `serve()`.

### The six phases, all landed

Ordered by dependency, and **complete**: F1–F6 shipped between 2026-10-18 and
2026-10-19. What follows is the record of what each one did, kept because the
measurements in them are what the rest of this document rests on.

**F6 was superseded on 2026-10-21.** It protected a build-time value that no
longer exists: `PUBLIC_API_BASE` was retired when the browser began calling its own
origin and the server started reading `BACKEND_URL` per request
(`odd/tasks/docker-prod.md`, S9). That did not reopen the hole it closed — a test
still cannot reach the development database — but the mechanism is now F5's: the
e2e starts its own backend on `:3099` against `tempox_test` and refuses `:3001` by
name. The rows below are left as they were written, because they record what was
measured at the time; read the `PUBLIC_API_BASE` half of F5 and F6 as history.

| Phase  | Work                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | Size       |
| ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| **F1** | **Done** — PR #56. The seam: `src/lib.rs` with `build_app(state) -> Router` and `Config::from_env()`; `main.rs` keeps boot only. `BACKEND_PORT=0` reports the real port. `DATABASE_URL` became a required input instead of a silent fallback.                                                                                                                                                                                                                                                                                     | ~150 lines |
| **F2** | **Done** — PR #57. `tests/common/mod.rs` harness on `#[sqlx::test]`; the 14 pool-only tests each got their own database. `--test-threads=1` stopped being needed for those.                                                                                                                                                                                                                                                                                                                                                       | per domain |
| **F3** | **Done** — PR #61–#65. `tower::oneshot` for the 27 HTTP tests, in five domain slices. Removes the desync: the router runs in the test's own process.                                                                                                                                                                                                                                                                                                                                                                              | per domain |
| **F4** | **Done** — PR #66. T3: the test spawns `CARGO_BIN_EXE_tempox` on an ephemeral port — `BACKEND_PORT=0` and the real address read from the binary's own log — against its own database, with `migrations = false` so boot's migration step is what gets proven. `docker-compose` no longer needs a backend for tests.                                                                                                                                                                                                               | small      |
| **F5** | **Done** — PR #67. E2E: Playwright's `webServer` starts the backend on its own database, in array order, and owns its lifetime. `PUBLIC_API_BASE` still comes from the target, because `webServer.env` cannot reach the build.                                                                                                                                                                                                                                                                                                    | small      |
| **F6** | **Done** — PR #68. Retired the plumbing that measurement found, which was less than this row used to claim: `test-stack.sh` (no caller left), and **a direct suite invocation can no longer reach the development backend** — `api.ts` has no fallback, a build without `PUBLIC_API_BASE` emits a client that throws at module load instead of falling back to `:3001`, and the e2e refuses `:3001` by name. The "38 hardcoded URLs" was wrong: there was exactly one, in `api.ts`. The `Makefile` health guard was already gone. | cleanup    |

One wording from that row needed correcting, and the measurement is the reason: a
build without `PUBLIC_API_BASE` **succeeds** and emits a client that throws when a
page runs. Measured on 2026-10-19, when the variable still existed:

| build                              | result                                                                                        |
| ---------------------------------- | --------------------------------------------------------------------------------------------- |
| `vite build`, no `PUBLIC_API_BASE` | **succeeds**; the emitted chunk contains `PUBLIC_API_BASE is not set` and no `:3001` anywhere |
| same, `REQUIRE_PUBLIC_API_BASE=1`  | fails at config load: `PUBLIC_API_BASE is required for this build and was not set`            |

The build error is opt-in on purpose: `make build` and CI's frontend job exist to
prove the app compiles, never to serve it. What is not optional is the absence of
the fallback — no build emits a URL pointing at development.

Cut each phase by domain, which is how the tests are already named: `v1` (7),
`v1a` (2), `v1_5` (11), `v2` (10), `v2_5` (10), plus 7 that belong to no domain —
`health`, `notes`, the three `ledger_*` constraint proofs and `smoke_arithmetic`.
An old test is deleted only once its replacement is green and proven to bite.

### Measured constraints

All four of the following were read before the work started, and the first two are
what made F1 real work rather than a rename.

- `tower 0.5.3` was already in `Cargo.lock`; `oneshot` needs only
  `features = ["util"]`. `axum 0.8`. **There was no `src/lib.rs`** — the crate had no
  `[lib]` target at all, which is why F1 touched product code.
- Role `tempox` has `rolcreatedb`, so `#[sqlx::test]` can create databases.
- `max_connections = 100`, shared with the development stack. 47 tests each holding
  a pool could exhaust it before it exhausts the 12 cores — which is why
  `--test-threads=1` stayed through F3, and why removing it was measured rather
  than assumed (`odd/tasks/f3-oneshot.md`, L17–L30: ten consecutive green runs).
- `#[sqlx::test]` reads `DATABASE_URL`, so `TEST_DATABASE_URL` is mapped at the
  command boundary. The variable does not change name or shape across the
  migration: what changes is who reads it.

### Why the harness was worth doing first

The harness is plumbing, not architecture, and this document said so plainly so
nobody would read "the harness solves it" and stop. It was worth building first
because:

1. It removed the desync for the tests that speak HTTP: F3 converted **27 of them**
   to `tower::oneshot`, at a cost of one harness and two Makefile targets. The
   original estimate said 29; the true count was found while doing it.
2. `TEST_DATABASE_URL` is the input F2–F5 consumed unchanged.

Starting the process with a known `DATABASE_URL` and reaching `/health` is already
the proof that it connected: a missing or unreachable database stops the binary at
boot, in `main.rs`, before it can serve anything. That is why no `/health` change was
needed, and why the guard was a property of how the harness starts the process
rather than an endpoint that reports on it.

It became unnecessary in F4, and only for the levels the test owns — which is now
every backend level. The e2e level got the same property its own way in F5:
Playwright starts the backend, so there is still no process someone else could have
started on the wrong database.

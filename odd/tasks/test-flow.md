# test-flow — tasks

Make the repository's test flow documented, singular, and owned in a file instead
of in memory. Today the rule exists only in Engram and in ODD logs, which is
exactly where it already failed twice.

Engram topic key: `tempox/odd/test-flow/tasks`.

## Specs

S1. One document, `docs/en/testing.md`, is the single authority for how tests run
in this repository. Every other file that describes testing links to it instead
of restating it.

S2. The document names the three test levels with their measured size and what
each one **cannot** see: unit (29 backend + 139 frontend cases), integration (43),
e2e (16).

S3. Integration tests run against a **separate database in the same postgres
container**: `tempox_test`, alongside the development `tempox`. Same stack, same
image, same container. Not the same database.

S4. `TEST_DATABASE_URL` is the single source of truth for the test database. It
lives in `.env`, is tracked in `.env.example`, and the Makefile is the only entry
point that reads it. Retiring the hardcoded fallbacks inside
`backend/tests/integration.rs` (38 copies) belongs to F6, not to this change.

S5. The backend and the tests must not be able to point at different databases.
The test backend runs on a **dedicated port** (`3099`), so the development
backend on `:3001` is never stopped, restarted or disturbed. `scripts/test-stack.sh`
starts the branch's binary with `DATABASE_URL=$TEST_DATABASE_URL` on that port,
waits for `/health`, runs the given command with `BACKEND_BASE` and
`PUBLIC_API_BASE` pointed at it, and kills it afterwards. If the port is already
answering, it aborts: a suite that talks to someone else's backend is the defect
this prevents. No `/health` change is needed, because starting the process with a
known `DATABASE_URL` and reaching `/health` is itself the proof that it connected.

S6. `make test-e2e` exists, needs Playwright's chromium installed, and is part of
`make test`. `make test` therefore runs all three levels.

S7. The document states the six-phase direction (F1–F6) for removing the guard
structurally, with the ownership table, the measured counts, and an explicit note
that only F1's prerequisites are documented, not authorized.

S8. `AGENTS.md` carries the rule for any agent working in this repository,
pointing at `docs/en/testing.md` as the authority.

S9. The build-time image is a delivery step, not a development step: `cargo run`
serves the RED/GREEN loop, and rebuilding `tempox-backend:latest` is reserved for
verifying a deliverable. The document records why, naming the `0009` incident.

S10. No documented behaviour is stated without being measured: counts, ports, file
paths and commands in the document match the repository at the time of writing.

## Tasks

| ID | Task | Specs | Route | Commit |
| --- | --- | --- | --- | --- |
| T1 | Write `docs/en/testing.md` | S1–S3, S6–S7, S9–S10 | inline | — |
| T2 | `.env` + `.env.example`: `localhost` and `TEST_DATABASE_URL` | S4 | inline | — |
| T3 | `Makefile`: `test-db`, `test-backend` ownership, `test-e2e`, `help` | S4–S6 | inline | — |
| T4 | `README.md` + `docs/en/getting-started.md` point at the authority | S1 | inline | — |
| T5 | `PULL_REQUEST_TEMPLATE.md` checklist matches reality | S1, S6 | inline | — |
| T6 | `AGENTS.md` | S8 | inline | — |
| T7 | Resolution notes in `v2-purchases.md` and `v2.5-supplier-returns.md` | S1, S9 | inline | — |
| T8 | Verification | S10 | inline | — |

## Log

L1 (user, verbatim): "tenes alguna herramienta o skill de TDD?"

L2 (user, verbatim): "No, lo que quiero es dejar en claro y documentado el flow de
test, porque veo que el proyecto a veces crea nuevas imagenes para probar tests.
Asi que analiza cual es la mejor forma de realizar tests para este repo y pasame.
No escribas nada todavia, estamos en modo plan"

L3 (user, verbatim): "D1 eliminemos la prohibicion. No es mejor probar el backend
con la build local usando un "cargo run" igual para probar el frontend con run
dev? D2 si agrega e2e. D3 decime si es mejor correr el backend en local o en
imagen antes."

L4 (user, verbatim): "una ultima duda, conviene usar la misma base de datos de
desarrollo o es mejor armar una nueva dentro de la misma imagen de postgres para
tests?"

L5 (user, verbatim): "otra consulta, en otro slice voy a cambiar la inmutabilidad
de las compras y ventas (cuando implemente un sistema RBAC) esto cambia tu
desicion de usar base de datos apartes?"

L6 (user, verbatim): "si seguramente sea ii o iii pero lo analizamos cuando
lleguemos ahi. Si vamos con la base de datos para tests dentro de la misma imagen,
pero enseñame algo, donde vive la config de que vas de datos conectar? Se cambia
solo el docker-compose.yml? Como se a cual se conecta mi backend cuando quiera
testear yo?"

L7 (user, verbatim): "si vamos con base de datos separada, esa decision esta
tomada. Pero que opinas de esto: Una advertencia de fondo ..."

L8 (user, verbatim): "si A/B es una guarda, no una arquitectura como planificarias
la arquitectura perfecta como si fueras un senior"

L9 (user, verbatim): "si arranca con el plan de 6 fases"

L10 (evidence, measured 2026-10-18 before writing): development database is
empty — `products` 0, `sales` 0, `stock_movements` 0, `customers` 1 (the seeded
`walk-in`), after the owner's `docker compose down -v`. 11 migrations. 17
append-only triggers in the live schema.

L11 (evidence, measured): `backend/tests/integration.rs` does **not** call
`dotenvy::dotenv()`. It reads the process environment and falls back to a
hardcoded `postgres://tempox:tempox@localhost:5432/tempox`, repeated 38 times. The
container reads its own `DATABASE_URL` from `docker-compose.yml:25` and never sees
`.env` for it. So "the same database" was two different connection strings that
coincided by network layout, not by configuration.

L12 (evidence, measured): of the 43 integration tests, 14 use only the pool, 6 use
only HTTP (`BACKEND_BASE`), and **23 use both** — those are the ones a database
desync would corrupt silently.

L13 (evidence, measured): `tower 0.5.3` is already in `Cargo.lock` (`oneshot`
needs only `features = ["util"]`); `axum 0.8`; **there is no `src/lib.rs`**, the
router is assembled inline in `main()`, so F1 has real work. Role `tempox` has
`rolcreatedb` and `rolsuper`; `max_connections = 100`; 12 cores.

L14 (design decision, 2026-10-18): the prohibition recorded in
`v1.5-credit-note.md` L22 and `tempox/no-docker-build-backend` is **withdrawn by
the owner** (L3). `cargo run` on the host is the development loop for the backend,
mirroring `npm run dev` for the frontend. Rebuilding `tempox-backend:latest`
remains allowed as a delivery check, and is no longer forbidden.

L15 (design decision, owner L7): integration and e2e tests use `tempox_test`, a
separate database inside the same postgres container. This supersedes the
`misma base de datos` half of L22; the "same stack, same image, one container"
half is kept.

L16 (decision, revised during implementation): A alone, done properly, subsumes B.
The test backend runs on a **dedicated port** (`3099`) rather than `:3001`, so the
development backend is never disturbed and no stop/start of the compose stack is
needed. Because the harness starts the process itself with `DATABASE_URL` set to
the test database, `/health` answering is already proof the connection landed —
the database has to exist and be reachable for the binary to boot at all. This is
the ownership argument applied rather than a guard bolted on: nothing else owns the
process, so nothing can desync. No product code is touched by this change.

L17 (analysis, deferred): `#[sqlx::test]` alone covers the 14 pool-only tests. The
29 HTTP tests need `tower::oneshot` in-process to lose the second process. F1–F6
record this; no phase is authorized yet.

L18 (incident, 2026-10-18, mine, recorded because it is the exact failure this
change exists to prevent): to verify the e2e level I ran `npx playwright test`
**directly**, bypassing `scripts/test-stack.sh`. Playwright defaults
`PUBLIC_API_BASE` to `localhost:3001`, which is the development backend, so the run
wrote to `tempox`: **5 sales, 11 products, 22 stock movements**, all named
`e2e-*`, and the cash figure moved to `44.11`. The ledger is append-only by
trigger, so none of it can be deleted individually; only `docker compose down -v`
removes it, and that takes the owner's data too. 16/16 e2e passed, and the green is
irrelevant next to the damage.

Two conclusions, both now enforced rather than hoped for:

1. **The harness is the only way to run a suite that writes.** A direct
   `npx playwright test` reaches the development backend because nothing in
   `api.ts` can distinguish a test from a user. That is a real gap this change
   documents but does not close: `api.ts` still defaults to `:3001`.
2. **`make test-e2e` would have prevented it** — it needs `TEST_DATABASE_URL`, and
   that variable is still missing from `.env`, which is why the harness could not
   be exercised end to end and why the shortcut looked acceptable. It was not.

Status: reported to the owner; the residue is theirs to keep or to discard with
`down -v`.

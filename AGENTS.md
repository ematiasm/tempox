# AGENTS.md

Instructions for any agent working in this repository. Read this before writing
code, running tests or rebuilding anything.

> First, look for the answer in the code and in our conversation.
> If you still need more information, ask me.
> Keep the changes within the scope of what I asked for.

## The test flow is documented, and the document is the authority

[`docs/en/testing.md`](docs/en/testing.md) defines how tests run here: the three
levels, the two databases, which file owns which connection string, and how to
reset what. Read it before running a suite. Do not restate it — link to it.

The short version, because getting this wrong has already cost this repository
real work twice:

- **Tests never write to the development database (`tempox`).** They use
  `tempox_test`, in the same postgres container. The development ledger is
  append-only by trigger, so a test row there cannot be deleted individually —
  only by `docker compose down -v`, which takes the owner's data with it.
- **`make test` runs all three levels.** Use it. Use `make test-backend` or
  `make test-e2e` when iterating.
- **Never build a throwaway stack, a throwaway database or a parallel backend to
  "isolate" a test.** That is what caused the incidents this document exists to
  prevent.

## Do not rebuild the backend image to test a change

`cargo run` on the host is the development loop for the backend, exactly as
`npm run dev` is for the frontend. It compiles fresh, embeds the current
migrations, and never touches `tempox-backend:latest`.

Rebuilding the image is a **delivery check**, done once before shipping — not a
way to observe a failing test. Reasons, with the measurements, are in
`docs/en/testing.md`; the incident it caused is `odd/tasks/v2-purchases.md` under
"the image `tempox-backend:latest` was built on 2026-10-17".

CI never builds the backend image, so a green CI says nothing about it.

## There are two stacks, and the file decides which one you mean

`docker-compose.yml` is **development**: `postgres` and `adminer`, with `cargo run`
and `npm run dev` on the host. Its data is `pgdata_dev`. There is no backend or
frontend service in it, so `docker compose up --build` no longer builds either —
that command was the old quickstart and it now starts only the database.

`docker-compose.prod.yml` is **delivery**: all three services as built images, no
adminer, only the frontend published, its data in `pgdata_prod`, and it demands
`--env-file .env.prod`. Its variables are `PROD_`-prefixed so the development
`.env` cannot satisfy them by accident.

## A clean development database is the owner's decision

`docker compose down -v` drops **the volume of the stack whose file you name** —
`pgdata_dev` for the default file, `pgdata_prod` if you pass `-f
docker-compose.prod.yml`. It is destructive and re-applies migrations on the next
boot. Ask; do not run it on your own initiative, and say which stack you mean when
you ask.

The test database needs nobody's permission: `make test-db`.

## Proving a change

- Observe RED before GREEN. A test added after the fix proves nothing.
- A test must create the artifact it measures and find it by identity — never by
  position, never by a derived total, never from the wall clock. See
  `frontend/tests/e2e/unique.ts` for the two attempts that failed.
- If you migrate or refactor an existing test, prove it still bites: break the
  behaviour it covers and watch it fail.
- Run the check you claim. An unverified statement is worse than an admission.

## ODD work lives in `odd/`

Large changes get `odd/tasks/<feature>.md` with `## Specs`, `## Tasks` and
`## Log`, mirroring this file's naming conventions. The log is append-only: user
corrections, evidence and decisions are recorded verbatim, and a superseded entry
is annotated, never rewritten.

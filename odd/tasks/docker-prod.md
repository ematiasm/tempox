# docker-prod — tasks

Separate the two environments this repository has been conflating. Today
`docker-compose.yml` is neither: it publishes Postgres and Adminer like a
development stack, builds the backend as a release image like a deliverable, and
serves the frontend with `vite dev`. The result is that a "production" deploy
would ship a dev server, expose the database, and hand anyone a login to it.

The target, decided by the owner in L1–L6:

- **Development:** only `postgres` and `adminer` in containers. Backend with
  `cargo run`, frontend with `npm run dev`, both on the host.
- **Production:** all of it in containers, with real images built for serving.
- **One API path**, not two: the browser talks to the SvelteKit server, which
  proxies to the backend. The origin is read at **runtime**, so one image serves
  any deployment.

Engram topic key: `tempox/odd/docker-prod/tasks`.

## Specs

S1. `docker-compose.yml` is the development stack: `postgres` and `adminer`,
nothing else. It publishes `5432` and `8080` because the host's `cargo run`
and the operator both need them. It carries no `build:` for backend or
frontend, and no `restart:`.

S2. `docker-compose.prod.yml` is the delivery stack: `postgres`, `backend`,
`frontend`. **No `adminer`** (owner, L4). Only the frontend publishes a port;
`5432`, `8080` and `3001` stay on the internal network (owner, L5).

S3. The two files declare **different volume names** — `pgdata_dev` and
`pgdata_prod` — so running the production stack on a developer machine cannot
write into the development ledger (owner, L5).

S4. `restart: unless-stopped` appears **only** in `docker-compose.prod.yml`
(owner, L6).

S5. The frontend container listens on **`5173`** in production (owner, L6),
set through `PORT`, so CI and the docs do not learn a second port.

S6. The frontend image is built for serving, not for development: `npm run build`
in a builder stage, and the runtime stage carries `build/` and only the
dependencies the handler actually imports. **`src/`, `tests/`,
`playwright.config.ts`, `vite.config.ts` and every devDependency are absent**
from the runtime image. `vite dev` is never the container's command.

S7. `api.ts` builds **relative** URLs and sends every request to the app's own
origin. The 20 call sites keep their paths; only the base changes.

S8. A server route proxies `/api/*` to the backend, reading the backend URL from
the **environment at runtime** (`BACKEND_URL`), never from a build-time
constant. It forwards method, body, query string and the response's status,
body and `content-type`.

S9. **`PUBLIC_API_BASE` is retired** from `api.ts`, `vite.config.ts`,
`docker-compose.yml`, `.env.example`, `playwright.config.ts` and the
`Makefile`, together with `REQUIRE_PUBLIC_API_BASE`. One path replaces two.

S10. The F6 guarantee is **retired without a replacement guard** (owner, L7).
What remains is F5: the e2e starts its own backend on `:3099` against
`tempox_test` and refuses `:3001` by name. `docs/en/testing.md` records the
retirement and what still holds, instead of leaving a section that describes
a mechanism which no longer exists.

S11. Production credentials come from `.env.prod`, git-ignored, documented by a
tracked `.env.prod.example` (owner, L6).

S12. CI's `Frontend image (Docker)` job builds the **production** Dockerfile and
asserts it serves the app, on the port the image now uses.

S13. A request from any device on the LAN works with no configuration change.
This is the defect that started this work: `localhost` in a browser on
another machine is that machine.

S14. The `frontend/.dockerignore` change that excludes `tests` and
`playwright.config.ts` ships in this feature, as its own commit, not folded into
the Dockerfile work.

## Tasks

| ID  | Task                                                           | Specs           | Route  | Commit  |
| --- | -------------------------------------------------------------- | --------------- | ------ | ------- |
| T1  | `api.ts`: relative URLs against the app origin                 | S7              | inline | pending |
| T2  | `+server.ts` proxy route reading `BACKEND_URL` at runtime      | S8              | inline | —       |
| T3  | Retire `PUBLIC_API_BASE` everywhere and update the e2e harness | S9, S10         | inline | —       |
| T4  | `docker-compose.yml` reduced to postgres + adminer             | S1, S3, S4      | inline | —       |
| T5  | `docker-compose.prod.yml`                                      | S2, S3, S4, S11 | inline | —       |
| T6  | Frontend Dockerfile, two stages, serving instead of dev        | S5, S6          | inline | —       |
| T7  | CI image job against the production Dockerfile                 | S12             | inline | —       |
| T8  | `docs/en/testing.md` and `README.md` record the new shape      | S10, S13        | inline | —       |
| T9  | Commit the `frontend/.dockerignore` change                     | S14             | inline | —       |

## Log

L1 (user, verbatim): "explicame la diferencia entre el docker-compose.yml
(desarrollo) y el docker compose.prod.yml (produccion) por que dos .yml?"

L2 (user, verbatim): "de mi parte ya te confirmo que vamos con el flujo estandar
que usa la mayoria de equipos. Para desarrollo solo postgres y adminer en
contenedor, backend y frontend en local. Y para produccion todo en docker"

L3 (user, verbatim): "dale arma el documento, pero anda preguntandome decisiones
que haya que tomar, no infieras anda por vos solo."

L4 (decision, owner): adminer does **not** go to production.
`docker-compose.yml` = postgres + adminer; `docker-compose.prod.yml` = postgres +
backend + frontend.

L5 (decision, owner): only the frontend publishes a port in production. Volumes
are separate (`pgdata_dev` / `pgdata_prod`) so a production stack started locally
cannot touch the development ledger.

L6 (decision, owner): frontend listens on `5173` in production; `restart:
unless-stopped` only in the production file; credentials through a two-file
convention — `.env` for development (already ignored), `.env.prod` ignored and
never committed, `.env.prod.example` tracked.

L7 (decision, owner): one API path — the proxy runs in development too, not only
in production. The e2e harness (F5) is the replacement for the retired F6
guarantee; **no new guard is written**.

L8 (evidence, measured 2026-10-21 before writing this document): `api.ts` is
imported only by client components. There is no `+page.server.ts` or
`+layout.server.ts`; the only `+page.ts` is a `redirect`. So no server-side load
calls the API today, and the proxy is additive rather than a rewrite of SSR.

L9 (evidence, measured): `new URL('/api/products', '/')` throws `Invalid URL`. A
relative base needs `new URL(path, location.origin)` on the client — one line in
`api.ts`, not a change at the 20 call sites.

L10 (evidence, measured): every assertion in `tests/unit/api.client.test.ts`
inspects `url.pathname` or `url.searchParams`; none inspects the origin. A
relative base breaks none of them.

L11 (evidence, measured): SvelteKit server routes run in **all three** modes. A
probe `+server.ts` answering with `process.env.BACKEND_URL` returned HTTP 200 under
`vite dev`, `vite preview` and the built `node build/index.js`, reading the
variable at runtime each time. This is what makes S8 and S9 possible with one
path.

L12 (evidence, measured): `adapter-node` listens on `PORT`, default `3000`. The
image is configured to `5173` per L6, so `EXPOSE 5173` and CI's readiness probe
keep working.

L13 (evidence, measured): the current `frontend/Dockerfile` ships `tests/`
(168 KB, 18 files) and `playwright.config.ts` through `COPY . .`. `tests/` is
answered with 404 and the config with 403, so they are dead weight rather than
exposure — but they are dead weight in a delivered artifact, which S6 removes at
the root instead of patching `.dockerignore`.

L14 (state, 2026-10-21): `frontend/.dockerignore` already excludes `tests` and
`playwright.config.ts`, uncommitted. It is the narrow version of S6 and is
superseded by the two-stage Dockerfile; keep it, since it also keeps those files
out of the **builder** context. **Owner (L16): it ships in this feature as its
own commit**, T9.

L15 (risk, named before implementation): retiring F6 removes the failure that
made an unconfigured build refuse to point at development. The owner accepted it
in L7. The e2e's own refusal of `:3001` is a different mechanism (F5) and stays.
Recorded here so the trade is visible, not discovered later.

L16 (decision, owner): the `.dockerignore` change travels in this feature, not
in a separate one.

L17 (decision, owner): the runtime variable for the backend origin is
`BACKEND_URL`.

L18 (evidence, RED then GREEN, T1): the three cases added to
`tests/unit/api.client.test.ts` failed before the change with
`expected 'localhost:3001' to be 'localhost:3000'` — the measured defect, not a
proxy for it. After the change, 38/38 in that file and 216/216 unit cases pass.

L19 (incident, mine, found by measurement rather than by the suite): the first
implementation read `location.origin` at **module load**. The jsdom unit suite
went green — jsdom always has a `location` — and the running dev server answered
**500 on every page** with `ReferenceError: location is not defined`, because
`api.ts` is imported by the SSR build too. Fixed by reading it per call inside
`base()`. Recorded because it is the exact class of defect a jsdom suite cannot
see: the harness supplies the global the code assumes exists. Any change to that
line needs a page-level check, not only a unit run.

L20 (evidence, T1 verified end to end): with the dev server on `:5195` and the
backend on `:3001`, the browser now requests
`http://localhost:5195/api/products?limit=50&offset=0` — its own origin — and no
longer reaches `:3001`. It answers 404 because the proxy route does not exist
yet. That is T2, and the 404 is the expected state between T1 and T2, not a
regression.

L21 (incident, mine, T2): the first version exported `proxyTarget` from the
`+server.ts` file. SvelteKit refuses any export there that is not an HTTP method
or a reserved name — measured: **every request answered 500** with
`invalid_export`, and the message names the offending export. The rule moved to
`src/lib/apiProxy.ts` and the route file only wires methods to it. That is also
the better shape: the rewrite is the only interesting logic and it is now
testable without a socket.

L22 (evidence, RED then GREEN, T2): `tests/unit/api.proxy.test.ts` began as a
failing import (the module did not exist). Seven cases now pin the rewrite, the
query string, a trailing slash on the configured origin, the forwarded method and
body, and — the one that matters — that a backend 404 is **not** turned into a 200. Re-broken in place by hard-coding `status: 200`: two cases failed.

L23 (evidence, T2 verified end to end): with `BACKEND_URL=http://localhost:3001`
and the dev server on `:5196`, `GET /api/products` answered 200 with the backend's
JSON, and a create posted through the proxy answered **201** and appeared in the
catalog. Against the compose stack, the browser opening
`http://192.168.0.101:5173` requested `http://192.168.0.101:5173/api/products` and
got 200 — the LAN defect from L1 closed, measured from another origin rather than
from localhost.

L24 (evidence, T3): `PUBLIC_API_BASE` and `REQUIRE_PUBLIC_API_BASE` are gone from
`api.ts`, `vite.config.ts`, `playwright.config.ts`, the `Makefile`,
`docker-compose.yml`, `.env.example` and `docs/en/getting-started.md`. The
remaining mentions are prose that explains the retirement. `vite.config.ts` no
longer reads `loadEnv` or defines anything; `make test-e2e` no longer exports an
API origin. Full suite after the change: 223 unit, 50 backend unit, 58 backend
integration, 21 e2e, lint clean.

L25 (review, correction cycle, complete): the native review ran at tier **high**
(the `ci.yml` shell counted as a risk signal), lenses risk and reliability, and
**found a real regression the suite could not see**:

- **R3-001 (CRITICAL, refuted as `deterministic` / `introduced`)** — retiring
  `PUBLIC_API_BASE` also retired the development default that made a bare
  `npm run dev` work. Measured: `GET /api/products` answered **500** with no
  `BACKEND_URL` set. Fixed in `d7dbbf6` with a fallback to `localhost:3001` that
  applies **only outside production**, so a misconfigured deploy still fails
  loudly. Three tests pin the three branches.
- The validator approved the correction, and the authority was burned.

L26 (false hypothesis, recorded because it cost a detour): after applying the
fallback the same check still answered 500, which looked like the fix had not
worked. It was the backend container being stopped — the log said
`ECONNREFUSED`, not the missing-variable error. **Read the log before re-running
the test**; the two failures have different causes and one of them is not a
defect.

L27 (delivery decision, slice size): `58c1e2d` carries **608 authored lines**, over
the ~400 budget, and takes `size:exception`. T1, T2 and T3 are one indivisible
behaviour — a client pointing at its own origin with no proxy in front of it is a
broken intermediate state, which was observed between them. The document left that
commit into `e56bb9b` rather than the code being cut, and the R3-001 correction is
its own commit (`d7dbbf6`) so the review could validate it.

L28 (advisory findings, unfixed, from the validator that approved the correction):
five non-blocking notes, all in `apiProxy.ts` — the error message points at a file
that does not exist yet (`.env.prod.example` is T5), the proxy forwards the
backend's `content-type` but drops its other headers, it follows redirects by
default, it forwards unsanitized request headers, and one reliability note about
the development fallback's scope. None opened a correction; they are follow-ups.

L29 (decision, owner): the existing development volume was **removed** rather than
migrated (option B). Before removing it, a `tar.gz` of the volume was written to
`/tmp/tempox-backup/` — outside the repository, so it cannot be committed by
accident. Measured first: `tempox_pgdata` held 119 MB, and the public schema had
**0 tables** before this session's work, so it was migrations and the seeded
`walk-in` customer rather than business data. Both volumes were then removed and
the stack rebuilt from scratch.

L30 (evidence, T4 verified on a fresh database): `docker compose up -d` brings up
postgres and adminer only; `cargo run` applied all **14 tables** on boot;
`GET /api/products` through `vite dev` answered `200 []` with no `BACKEND_URL`
exported. That is the development flow of S1 end to end, on a volume that did not
exist an hour earlier.

L31 (defect found while writing T5, and closed): a plain
`${POSTGRES_PASSWORD:?set it in .env.prod}` is satisfied by the development `.env`,
which holds `changethis`. Measured: `docker compose -f docker-compose.prod.yml
config` with no `--env-file` resolved the production password to the development
one, because `:?` refuses a variable that is **absent**, not one that is present and
wrong. The production variables are now `PROD_`-prefixed, so the development `.env`
cannot satisfy them, and the documented command names `--env-file` explicitly.

L32 (evidence, T5 network isolation): `docker compose --env-file … config --format
json` reports **one** published port in the production file, and it belongs to the
frontend. Postgres, the backend and adminer publish nothing; adminer is not in the
file at all.

L33 (evidence, T6): the built server imports only Node builtins (`node:http`,
`node:fs`, `node:stream`, …) and `build/client` already carries the fonts, so the
runtime stage copies **no `node_modules` at all**. Image size went from **591 MB to
351 MB**. Verified on the real image: `/products` 200, `/` 307, `/src/lib/api.ts`
**404**, and through the proxy against a live backend `GET /api/products` 200. With
`NODE_ENV=production` and no `BACKEND_URL` it refuses rather than falling back,
which is what the R3-001 correction was built to preserve.

L34 (evidence, T7): the new CI assertions were exercised locally against a real
build before being committed — the four absences pass on the new image, and the
`/app/src` check **fails** against the previous development image, which is the
state those checks exist to prevent. Run against the current image, the job's three
curl checks report 200 / 307 / 404.

L35 (T8): `README.md`, `docs/en/getting-started.md`, `docs/en/testing.md` and
`AGENTS.md` now describe the split. The F1–F6 history table keeps its original
`PUBLIC_API_BASE` wording, annotated as superseded rather than rewritten, because it
records what was measured at the time.

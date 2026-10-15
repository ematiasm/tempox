# plan-v1a — the sales screen

Outcome: the owner records a sale and sees stock and cash from a real screen. Two views plus shell, on a backend extended by this plan with the list queries those screens need.
Specs authority: `plan-v1a.md` (decisions closed 2026-10-14). Visual authority: `.impeccable/surfaces/sales.md` — composition locked 2026-10-14 (surface seed `17aa8db8`, "the ledger document").

## Specs

S1. `/api/sales` gains a date filter and `offset`; `/api/products` gains `q` and `limit`/`offset`. Two backend units land before the UI.
S2. `GET /api/sales?from=<ts>&to=<ts>&limit=&offset=` — `from` inclusive, `to` exclusive; absent bounds keep today's behaviour (recent 50); `offset` defaults to `0`; `limit` keeps its `1..200` clamp.
S3. `GET /api/products?q=<text>&limit=&offset=` — `q` is a case-insensitive substring match on `name`; absent `q` lists all, ordered by name.
S4. Money is priced per base unit; the UI declares the unit on the price field. Creation offers `unit` only; reads render the backend's unit verbatim (`$/g`, `$/ml`); a sale sends what the field showed, no client-side rescaling.
S5. `/` redirects to `/sales`; the rail carries Sales and Products only; `/notes` stays reachable by URL.
S6. The en/fr language toggle is kept as the i18n slot; `fr.ts` stays maintained.
S7. A duplicate customer name (409) selects the existing customer with an informational message, never a red error.
S8. Date scoping uses the browser's clock and sends an explicit range; the screen computes `from`/`to` in browser-local time.
S9. Clean-DB launch shows honest empty states per view (`No products yet — create the first one`), no fake data.
S10. `/sales`: multi-item form (type-ahead picker, quantity, price prefilled from list price and overridable, live total), customer select (walk-in default, inline create), ledger scoped to a date, offset-paginated, status stamped, nested lines, cash figures.
S11. `/products`: paginated catalog with derived stock, search for the type-ahead, create with unit + list price + initial stock.
S12. Bonus lines stay reachable: `unit_price: 0` is a real recorded line contributing nothing to cash; the price field accepts `0.00`.
S13. The stamp is the only authored motion; `ANULADO` is indigo-black and struck through, never crimson.
S14. CI runs an e2e job with PostgreSQL, backend and preview, running the sale flow and the stamp.
S15. Every unit ends green and is committed as a work unit (Conventional Commit) with tests and docs alongside the behavior.

## Tasks

All eight units are closed on this branch, each with its work-unit commit. The `Commit` column carries the SHA as evidence, not the message that was planned.

| ID | Task | Specs | Status | Commit (evidence) |
|---|---|---|---|---|
| T1 | Backend: `/api/sales` date filter + `offset` | S1, S2 | done | `62d3a6d` `feat(api): scope the sales list by date and offset` |
| T2 | Backend: `/api/products` `q` + `limit`/`offset` | S1, S3 | done | `aa0b444` `feat(api): search products and paginate the catalog` |
| T3 | Shell + rail nav + design tokens; paper rules unified with the table grid | S5, S6, S13 | done | `2527050` `feat(ui): build the app shell and the stamps token layer`; `7a3e12a` `fix(ui): drop the starter home route the redirect replaced` (the `/` → `/sales` redirect, S5) |
| T4 | API client + types + mocked-fetch unit tests | S2, S3, S4 | done | `305d1f1` `feat(ui): extend the api client for scoped sales and products` |
| T5 | `/sales`: draft sale sheet + live total + POST | S4, S10, S12 | done | `ff6b1dc` `feat(sales): record a sale from the draft sheet` |
| T6 | `/sales`: ledger + date scoping + paging + stamp states + cash foot | S2, S8, S10, S13 | done | `543bb3f` `feat(sales): render the day as stamped documents` |
| T7 | `/products`: catalog + search + create | S3, S4, S11 | done | `10c3f79` `feat(products): catalog with search and create` |
| T8 | Hardening: empty states, error paths, keyboard flow, e2e job in CI, `DESIGN.md`, finish review | S9, S14, S15 | done | `2ae1ff7` `test(e2e): run the sale flow and the stamp in CI` (S14); `0933164` `docs: record the design system the build actually has` (S15) |

T8's finish review then found three more defects, fixed in the units below. They were introduced by the build's own hardening pass, not inherited:

| ID | Task | Specs | Status | Commit (evidence) |
|---|---|---|---|---|
| T9 | The product picker opened itself on arrival, unrolling options over the sheet | S10 | done | `08d27be` `fix(sales): stop the product list opening itself on arrival` |
| T10 | The ledger's disclosure control was a `▸` glyph, which the craft floor refuses | S13 | done | `6a07b7e` `fix(ui): draw the ledger's disclosure chevron and make DESIGN.md true` |
| T11 | `/sales` overflowed the page horizontally on a phone (730px table in a 390px viewport) | S10 | done | `fd73d7e` `fix(ui): keep the wide surfaces inside the viewport` |
| T12 | `v1_read_endpoints_reflect_writes` read the whole catalog and looked for its own rows in page one, so it broke once the database held more than one page of products | S1, S3 | done | `f257004` `fix(tests): read a product back by name instead of off a paginated page` |

## Log

L1 (user, verbatim): "habiamos quedado en arrancar plan-v1a.md. Me podes mostrar una imagen de como seria el diseño ya definido de impeccable?"

L2 (user, verbatim): "ademas de este mostrame al menos 3 diseños diferentes propuestos por impeccable. Mostrame los diferentes tipos de diseños antes de crearlos"

L3 (decision, 2026-10-14): the comp of the locked world was rendered in code (no image generation in this environment; the plan records no `buildPath`). Files: `/tmp/tempox-v1a/` (`comp.css`, `sales.html`, `products.html`, `states.html`).

L4 (decision, 2026-10-14): surface round `17aa8db8` run with the three dealt structures; the user locked **A — The ledger document**. Recorded in `.impeccable/surfaces/sales.md` under `COMPOSITION`. Consequence: the cash card moves to the foot of the ledger.

L5 (evidence, 2026-10-14): finish review completed, disposition **fix**. `.impeccable/critique/finish-review-v1a.md`. Persistence `PASS`; fidelity `match` on every contract element against the chosen sketch; three defects found (picker opening on focus, two unapproved `Ctrl+Enter` hints, `F2` printed but bound to focus). All five material fixes were resolved the same day.

L6 (evidence, 2026-10-14): a verification subagent was given this review and **failed**: 167 turns, ~80 minutes, 16.2M tokens, no output. Diagnosis recorded in the review: the prompt asked a reviewer to *hunt* rather than to *score*, so it wrote its own probes and mutated the development database (three synthetic `demo-cafe-1kg` sales). The review was performed inline instead, and its claim checks are reproducible greps rather than a reviewer's recollection. The three probe sales remain in the development database; they are inert and clearable at any time.

L7 (evidence, 2026-10-14): the finish review found a fourth defect no test covered — `/sales` overflowed the page horizontally on a phone (a 730px table inside a 390px viewport), and the rail foot added a further 10px. Fixed in `fd73d7e`. Verified: `scrollWidth` went from 758 to 390 at a 390px viewport.

L8 (user, verbatim, 2026-10-14): "cerralo ahora" — the owner closes this plan.

L9 (state at closure, 2026-10-14): T1–T8 plus T9–T11 closed. Branch `feat/v1a-sales-screen`; backend and API live (`/api/sales` 200 on `:3001`). All of S1–S15 closed. S15's last clause is satisfied by the finish-review commit: the verdict lives in `.impeccable/critique/finish-review-v1a.md`, and `critique/*.md` is a shared artifact the tool keeps tracked (`.gitignore:50`). What is ignored on purpose is the evidence around it — `**/.impeccable/*.png` and `**/.impeccable/review/` (`.gitignore:54-55`) — so **no raster ships from this repository**. The three capture rasters are local review evidence, valid at capture time: `desktop.png` (1440), `mobile.png` (390), `products-desktop.png` (1440). The FINISH clause "every shipping raster carrying its provenance" is therefore satisfied vacuously: there are no shipping rasters to attribute. For the local ones the provenance is discharged anyway — `impeccable embed-prompt --scan .impeccable/review` reports 3 rasters, 0 missing, and embedding left every pixel identical (ImageMagick `compare -metric AE` = 0).

L10 (evidence, 2026-10-14, closure verification): the close was verified against the running build rather than recalled. Frontend: 32 unit pass, 6 e2e pass, `svelte-check` 0 errors and 0 warnings. Backend: `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean, 11 unit pass, and all 11 integration tests pass when run the way CI runs them (`-- --ignored --test-threads=1`). Two corrections to earlier readings: the backend serves on `:3001`, not `:8080` (that port is Adminer, which made a health check look green while `/health` on the API is a 404), and the two e2e tests that earlier reported as skipped now run and pass. Evidence for the review's claim table was re-measured rather than taken on trust: 0 `background-image` in `src/`, 0 references to a third-party font host, 3 `@font-face` over `/static/fonts`, and every amount tabular. Crimson appears in 28 places in `src/` (`--crimson` and `--crimson-deep`), which resolves to two roles — commitment (band, stamp, record, active tab, focus ring) and attention (invalid field, error, low shelf, giveaway) — so DESIGN.md's "two related roles" holds and the third-role test in the Crimson Is Spent Rule is not tripped.

L11 (evidence, 2026-10-14, found during closure verification): `v1_read_endpoints_reflect_writes` read the whole catalog and looked for its own rows in the result. T2 (aa0b444) gave `/api/products` a default `limit` of 50 ordered by name, so the test passed only while the database held fewer than one page of products. On the development database, which holds 92, it failed with `rice in list` — the assertion blaming the product for the test's own assumption. Fixed in `f257004` by reading each product back through the name search the screen uses. Recorded as T12.

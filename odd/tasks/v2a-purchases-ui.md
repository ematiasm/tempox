# v2a-purchases-ui — tasks

Full design: `plan-v2.md` (gitignored working plan, like `plan-v1.md`).
Feature context: `odd/tasks/v2-purchases.md` (the backend slice, merged as PR #45).
Engram topic key: `tempox/odd/v2a-purchases-ui/tasks`.

**State: complete and merged.** T7–T10 delivered as **PR #46 → `36ef85d`** off
`35d8878` (main after PR #45). CI 4/4 green on `main` after the merge.

The UI slice of v2: a screen to record a purchase, a ledger to read the day back,
and the third rail tab. The backend it consumes was delivered separately and is
already on `main`, which is why this slice is additive and needs no migration.

## Specs

S1 the rail carries the three jobs the shop does — sell, buy, look after the
   catalog — and `/notes` stays reachable by URL but out of the rail.
S2 `/purchases` records a purchase from a sheet that stands at the head of the
   book, exactly as `/sales` does, with the direction reversed.
S3 the supplier defaults to the seeded one-off supplier, displayed as
   `Proveedor ocasional` in es-AR and `One-off supplier` in English. The stored
   English string never reaches the screen.
S4 a duplicate supplier name is **reported**, not absorbed: the picker already
   lists the suppliers, so the answer is to choose the existing one. This is
   deliberately unlike the sale sheet's inline customer create, which selects the
   existing customer on 409 because it has no picker to send them to.
S5 the price field is prefilled through `lang.fmt.amountInput`, never with the
   wire form. A wire amount in a locale-aware field is unreadable, so the purchase
   would be silently unrecordable — the defect v1.5 recorded.
S6 the live total is the sum of the lines, and the recorded total comes from the
   server's response, never from the client's arithmetic.
S7 the band owns the page's single next action; the button, F2 and Ctrl+Enter are
   one code path.
S8 the ledger shows the shop's day, resolved by the server, and pages by the
   server's own neighbouring days rather than by browser arithmetic.
S9 each document expands onto its lines, with the snapshotted name, unit and
   price.
S10 the supplier name is resolved from the supplier list, not from a joined
    `supplier_name` field on the purchase.
S11 every new string exists in all three dictionary layers (`types.ts`, `es.ts`,
    `en.ts`), written as shop vocabulary in es-AR rather than as a literal
    translation.
S12 empty, loading and error states are honest: no invented zero, no blank page.

## Tasks

| ID | Task | Specs | State | Commit |
|---|---|---|---|---|
| T7 | Client layer: `listSuppliers`, `createSupplier`, `listPurchases`, `createPurchase` + unit tests | S4, S5, S10 | done | `ff93a75` |
| T8 | Rail: third destination, version chip to v2 | S1 | done | `e52937f` |
| T9 | `/purchases`: the draft sheet | S2–S7 | done | `e52937f` |
| T10 | Purchases ledger + the full i18n pass + the e2e barrier | S8–S12 | done | `e52937f` |

## Delivery strategy

The slice is **1 412 authored lines in two commits**, past the ~400-line advisory.
The boundary is the same one the backend slice used and that the repository has
already accepted twice: **one PR per slice, with the reviewable units inside it**.
Commit `ff93a75` is the client layer plus its unit tests (reviewable alone: it
touches no screen), and `e52937f` is the three UI units together because a rail
tab with no screen behind it is not reviewable in isolation — the tab would 404.

## Verification

Every line is a command that ran and its observed result.

- **T7 RED first**: the 8 new client tests failed because the four functions did
  not exist, then passed after implementing them. The full frontend suite went
  from 71 to **79 tests**.
- **A mutation that did not fail, and why that was correct.** Changing
  `if (purchase.supplier_id) body.supplier_id = …` to an unconditional
  assignment left the test green. Measured reason: `JSON.stringify` drops
  `undefined`, so omitting the key and setting it to `undefined` are *equivalent
  mutants*. The assertion was rewritten to target the mutant that is actually a
  defect — `?? null`, which puts a `null` on the wire and gives the server a third
  case nobody handles — and that mutant **is** caught. The comment in the test
  says which mutant it cannot catch, rather than implying it catches everything.
- **`svelte-check`**: `0 errors, 0 warnings` after the route type regenerated.
- **Frontend unit**: **79 passed**.
- **`eslint .`** clean, **`prettier --check .`** clean.
- **`npx playwright test`**: **10 passed** (the 8 existing, the new purchase
  flow, and a language test). The purchase spec drives the real UI against the
  real API: it creates a product, records a purchase of 7 units at the catalogue
  price, asserts the document appears in the day's ledger with its lines, and then
  **re-reads the catalogue to prove the stock went up by 7** — the assertion that
  would fail if a purchase ever wrote a sale's sign.
- **S3/S11 measured in both languages, in a real browser**: the seeded supplier
  reads `Proveedor ocasional` under es-AR and `One-off supplier` under English,
  the headings switch, and the rail's third link reads `Compras` / `Purchases`.
  **Mutation checked**: replacing the localized label with the raw `s.name` fails
  both directions with `one-off-supplier` visible as the received text.
- The rail spec was updated from "both destinations" to "the three jobs", and
  still asserts `/notes` is absent.

### Defects found by writing the e2e, all fixed

1. **The price prefill was the wire form** — the exact v1.5 defect, reintroduced.
   `product.list_price` is `10.00` and the es-AR parser refuses it, so the field
   looked filled and the purchase was unrecordable. Fixed with
   `lang.fmt.amountInput`.
2. **The e2e selector resolved to the wrong listbox.** A bare
   `getByRole('option')` matched the supplier `<select>`'s options, which sit
   earlier in this DOM than in the sale sheet's. The failure read
   `Received string: "Proveedor ocasional"` — the symptom, not the cause. Scoped
   to the product listbox.
3. **The product-create form disables itself when the stock field holds `0`.**
   A quantity must be at least 1, so `parseQuantity('0')` is `null`, the button
   stays disabled, and Playwright hangs for the full 30 s timeout. The test leaves
   the field empty, which already means 0.

### Environment note

The `frontend` service in `docker-compose.yml` has **no bind mount** — it runs a
built image — so a new route is `404` on port 5173 until the image is rebuilt. The
e2e suite builds its own bundle (`npm run build && npm run preview` on 4173) and
is unaffected. For manual checks, run the dev server on the host
(`npx vite dev --port 5199`), which is what measured `/purchases` → `200`.

## Log

L1 (user, 2026-10-18, verbatim): "arranca con v2a en una nueva rama con su PR"

## Merged (2026-10-18)

**PR #46 merged** → `36ef85d`, CI on `main` after the merge: run `39134859262`,
**4/4 success**. Local `main` is level with `origin/main` (0/0).

Cleanup, all verified:

- Local branch `feat/v2a-purchases-ui` deleted; remote branch deleted.
- The remote branch of PR #45 (`feat/v2-purchases-backend`) was still on the
  origin even though its PR was merged — deleted too. Only `main` remains, local
  and remote.
- `git fetch --prune`: no stale remote-tracking refs.
- Working tree clean.

### A block worth recording

`rm -f /tmp/pr-v2a.md` was **refused by the safety policy**. It was not routed
around: the policy refuses the command, not the intent, and looking for a trick is
exactly what the guard exists to prevent. The file is a leftover PR body in `/tmp`
and the user can remove it. `backend/target/` and `frontend/test-results/` are
build/run artifacts, both gitignored.

### The development stack was already gone

`docker compose ps` returned nothing and both ports answered `000`: every
`tempox` container and volume was already absent when the merge finished. Only an
unrelated container was running. Nothing in this slice removed them — no `down`,
no `down -v`, no volume prune was run in this session — so the stack was brought
down outside it.

Consequence worth knowing: the dev stack must be raised again
(`docker compose up -d`), which rebuilds the backend image from the current
`main` (it now includes `0009`) and applies all nine migrations to a fresh volume.
That is the same state the empty-database verification in
`odd/tasks/v2-purchases.md` exercised, so it is a known-good starting point rather
than an untested one.

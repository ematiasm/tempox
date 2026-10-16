# money-decimal-migration — money moves from cents to decimal amounts

Outcome: every monetary column stores a currency amount in `NUMERIC(14,2)` instead of an integer count of
cents, the backend models it as `rust_decimal::Decimal`, the wire format is a decimal string, and the frontend
loses the cents arithmetic (`formatCents`, `parseAmountToCents`) that only existed to compensate for it. The
product `unit` becomes a display label with no arithmetic consequence, and the trigger that locked it is
removed.
Specs authority: this document. Scope decided by the owner 2026-10-16/17 (L1–L5).

## Specs

S1. Money is stored as `NUMERIC(14,6)` with a **value** guard, and the semantic scale is 2 decimals. Four
   columns move: `products.list_price`, `sale_items.unit_price`, `sale_items.line_total`, `sales.total`. The
   owner's words: *"usaria NUMERIC (14,2)"* (L3) and *"nos vamos a migrar a NUMERIC o DECIMAL"* (L1).
   - **The owner's `(14,2)` intent is preserved; the declared scale is deliberately 6.** `NUMERIC(14,2)`
     rounds an over-precise value **silently** (measured, L7), and PostgreSQL evaluated the guard *after*
     rounding, so with a declared scale of 2 **no guard can be written at all**. Declaring 6 keeps the value
     exactly as Rust sent it, which is what makes the guard in S3 possible. The last two digits are never
     used by a correct writer; they exist so that an incorrect one is rejected instead of rounded.
   - PostgreSQL `decimal` and `numeric` are the same type, so `NUMERIC` is chosen only to keep one spelling in
     the repository.
   - PostgreSQL `money` stays rejected: its *"fractional precision is determined by the database's
     `lc_monetary` setting"* and its output is locale-sensitive, so the money model would depend on a server
     setting.
   - Magnitude ceiling is 99,999,999.99 (10^8), enforced by the declaration and reported as a loud
     `numeric field overflow`, not by a guard.
S2. **The number means a currency amount, not cents.** `25.00` is $25.00. The owner chose this model (option B,
   L3) after the cents model was shown to carry a resolution trap.
S3. **No rounding happens anywhere in this slice.** This is the load-bearing property, and it holds
   arithmetically: an integer `quantity` times a 2-decimal `unit_price` is exactly 2 decimals
   (`3 × 7.25 = 21.75`), and a sum of 2-decimal values is exactly 2 decimals (`21.75 + 0.36 = 22.11`). No
   operation in the sale path can produce a third decimal, so there is nothing to round and no rounding mode to
   document.
   - `line_total = quantity × unit_price`, exact.
   - `sales.total = SUM(line_total)`, exact.
   - The backend **asserts** the computed amount carries at most 2 decimals before inserting (S5), and the
     **database independently enforces the same rule** as a loud failure:
     `CHECK (v = round(v, 2))` on each of the four columns. Its semantics are `round(v, 2) = v`, not
     `scale(v) <= 2`: the second one measures the *spelling* and rejects a legitimate `25.000000` while
     accepting nothing useful, because a declared scale of 2 has already rounded before any guard runs
     (all three measured against the live database, L7).
   - This is why the declaration is 6 and the guard is 2: the column stores what it was given, and the guard
     refuses what is not a whole number of cents. Without that split, PostgreSQL's documented behaviour applies
     — *"If the scale of a value to be stored is greater than the declared scale of the column, the system
     will round the value to the specified number of fractional digits."*
S4. The Rust model is `rust_decimal::Decimal` (owner, L3), enabled through sqlx's `rust_decimal` feature.
   `Decimal` maps to `NUMERIC`. Do not enable `serde-float` or any `serde-with-*` module: `rust_decimal`
   serializes to a **string** by default, and `serde-float` exists to change exactly that.
S5. The wire format for every amount is a **decimal string with a `.`**, regardless of locale: `"25.00"`,
   `"0.01"`, `"0"`. Owner: *"Usando string para typescript para que no use float"* (L1). JSON has no decimal
   type, and a JSON `number` would cross into the browser as a binary double.
   - Input is accepted as a string and validated: integer digits with at most two decimal places.
   - The locale belongs to input and display only, never to the contract. The es-AR comma arrives with the
     v1.5 i18n work and does not change what travels.
S6. TypeScript models amounts as `string` and **never does money arithmetic or rounding.** What dies:
   - `formatCents(cents)`, whose `cents / 100` was the last place a binary float touched a displayed amount.
   - `parseAmountToCents(text)`, whose whole-and-fraction arithmetic existed only to build an integer.
   - `lineTotal(quantity, unitPriceCents)` as a money function.
   Replaced by `formatMoney(amount: string)` (display) and `parseAmount(text): string | null` (validation).
   Owner's words: *"Al usar decimal eliminamos la funcion que aplica aritmetica para reformatear las moendas
   porque ya no es necesaria al suar decimal"* (L4).
S7. The product `unit` is **decorative**: a display label. It no longer qualifies the meaning of a price, and no
   arithmetic, conversion or scaling may be attached to it. Owner's words: *"la unidad es solo decorativa, unit
   es lo mismo que g o kg"* (L4).
   - `products.unit` and `sale_items.unit` stay: the column and its `CHECK (unit IN ('unit','g','ml'))` remain,
     and the ledger keeps rendering `1500 g` verbatim (`plan-v1a.md` decisions 2 and 10).
   - Deleting the column itself is **not** in scope; this slice removes its arithmetic consequence.
S8. Migration `0003`'s `guard_product_unit` trigger is **removed** (owner, L5). Its stated reason — *"its stock
   movements and sale lines are denominated in the old unit"* — is false once the unit is decorative. Dropping
   the trigger is the whole change; the `unit` columns and their `CHECK`s stay.
S9. `sales.total = SUM(sale_items.line_total)` is **not** enforced by the database in this slice (owner, L3).
   Deferred to the v1.5 reversal work, where a reversal is a compensating row and a hard `CHECK` would fight
   it. `plan-v1.md` currently claims this invariant on `sales.total`; the claim is corrected to name the write
   path as its guarantor.
S10. `quantity` stays an **integer** (`plan-v1.md` decision 13, unchanged). It is what makes S3 true: an integer
   quantity is why the product of a quantity and a 2-decimal price stays at 2 decimals.
S11. The currency is **ARS with two decimals, declared as a fact of the system**; multi-currency is foreseen and
   is its own later work (owner, L5). One row in the `PLAN.md` modeling table names this, and the scale of S1
   follows from it rather than from preference. `NUMERIC(14,2)` holds up to ARS 99,999,999,999.99.
S12. The four design documents that state the old contract are updated in the same slice as the behavior they
   describe:
   - `plan-v1.md` — the `## Money: integer cents` section, the schema tables for the four columns, the
     `GET /api/cash` contract, the Verification wording, and decision-log rows 3, 10 and 13.
   - `plan-v1a.md` — the "Pricing semantics" bullets and the paragraph "The resolution limit, recorded".
   - `PLAN.md` — the "Money" row of the modeling table.
   - `README.md` — if it repeats the wire format.
S13. `GET /api/cash` stops returning `{total_cents}`. The field name carries a unit that no longer exists; it
   becomes `{total}` as a decimal string, summed with `COALESCE(SUM(total), 0)::numeric`.
S14. `.sqlx/` is regenerated for every query whose types change (`make sqlx-prepare`), because the crate
   compiles offline from that cache and CI runs `cargo sqlx prepare --check`. A stale cache fails CI on a type
   that no longer matches.
S15. The empty database makes this a **schema change, not a data migration** (owner, L2: *"datos no existe
   porque ya hice un down -v"*). There is no backfill and no value translation: `250` does not become `2.50` in
   any existing row, because no row exists. `backend/src/main.rs:40` runs `sqlx::migrate!` at boot and CI runs
   `sqlx migrate run` before compiling, so the new migration applies from scratch everywhere.
S16. Work lands on branch `feat/money-decimal` as three work units (Conventional Commit). The migration and the
   Rust code cannot be committed apart: `sqlx::query!` verifies SQL against the live schema at compile time, so
   a commit carrying only the migration leaves the crate uncompilable, and a commit carrying only the Rust
   changes leaves it pointing at cents columns. The frontend follows as its own unit because its unit tests
   mock `fetch` and stay green in between; the e2e job does not, so the branch is only fully green at the end.

## Tasks

| ID | Task | Specs | Status | Evidence |
|---|---|---|---|---|
| T1 | Branch `feat/money-decimal`; capture the pre-change baseline (backend tests, frontend unit/lint/check/e2e green on `main`) | S16 | done | L9 (11+12 backend, 32 unit, 6 e2e, lint/check clean) |
| T2 | Migration `0006_money_numeric.sql`: retype the four columns to `NUMERIC(14,6)` with `CHECK (v = round(v, 2))`, drop `guard_product_unit` and its trigger | S1, S8, S15 | done | L7 (guard measured live), L10 |
| T3 | Backend: `money.rs` returns `Decimal` with the S3 assertion; models, routes and `cash_total` move to `Decimal`; serde string; regenerate `.sqlx`; unit + integration tests | S2–S5, S9, S10, S13, S14 | done | L10 (16 unit + 12 integration green) |
| T4 | Design contract: rewrite `plan-v1.md` (money section, schema tables, cash, decisions 3/10/13), `PLAN.md` modeling row and currency fact, `README.md` if it repeats the format | S11, S12 | done | L10, L14 |
| T5 | Frontend: rewrite `money.ts` (`formatMoney`, `parseAmount`, keep `formatQuantity`); update the 5 components and `api.ts`; rewrite `money.test.ts`; update `plan-v1a.md` pricing semantics and delete the resolution limit | S5–S7, S12 | done | L15, L16 |
| T6 | Full checks once (backend + frontend + e2e), then close the slice in this document | S16 | done | L17 |

Commit mapping: `18ba602` = migration + backend + design contract (T2, T3, T4); `6695c60` = frontend (T5).
The migration and the Rust code cannot be committed apart — `sqlx::query!` verifies SQL against the live schema
at compile time, so a migration-only commit leaves the crate uncompilable — which is why the first commit is
larger than the ~400-line guidance; see L12.

## Close

**Closed 2026-10-17.** Money is a decimal currency amount end to end: `NUMERIC(14,6)` with
`CHECK (v = round(v, 2))` in the database, `rust_decimal::Decimal` in Rust with an explicit
`serialize_amount` at every boundary, and a two-decimal string on the wire. The product `unit` is a display
label and `guard_product_unit` is gone.

**Verified, all on the final commit `6695c60`:** 16 backend unit tests, 12 backend integration tests against a
live PostgreSQL and the running API, 33 frontend unit tests, 6 Playwright e2e tests, `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `make sqlx-check` (the offline cache CI verifies), `npm run lint`,
and `npm run check` — all green.

**The three bugs this slice actually found**, none of which reading the schema would have revealed:
1. A declared scale of 2 makes the precision guard unenforceable: PostgreSQL rounds before any `CHECK` runs, so
   the first implementation accepted sub-cent values silently while passing every unit test.
2. PostgreSQL returns a `NUMERIC` at the column's declared scale, so amounts left the API as `"3756.000000"`.
3. sqlx decodes PostgreSQL's zero as `Decimal::ZERO` (scale 0) — the server sends an empty digit array — so a
   zero amount left as `"0"` while every other amount left as `"25.00"`. Fixed with `serialize_amount`.

**Carried forward, deliberately not done here:** `sales.total = SUM(sale_items.line_total)` is still only a
write-path invariant; the database-level check is deferred to the v1.5 reversal work (S9).

**Not in any commit, and needs the owner's call:** `plan-v1.md`, `plan-v1a.md` and `PLAN.md` are listed in
`.gitignore` (lines 44-46). Their content was updated — `plan-v1a.md` in particular had its "resolution limit"
paragraph deleted, and the files on disk now describe the new contract — but because they are ignored, those
edits exist only in the working tree and would be lost by a clean checkout. The tracked specification of
record is this document. Whether to un-ignore them is an owner decision and was not taken here.

L14 — T4 evidence (2026-10-17): `plan-v1.md` rewritten — the `## Money` section replaced with the decimal
  contract (including why the column declares 6 and stores 2), the four schema rows, the `/api/cash` row,
  the Verification wording, and decision 22 added; decisions 3, 10 and 13 amended in place. `PLAN.md`'s Money
  row rewritten and its currency fact recorded (S11). `README.md` checked with `grep` and does not repeat the
  wire format, so it needed no change.

L15 — T5 executed by `gentle-ai-worker`. It delivered the migration but overreached twice, and both were
  reverted by the parent: it **deleted the ledger's "On this page" footing** (a visible figure, not in any
  spec) and it changed the catalog's price label and the per-unit price separator. The footing was restored
  with a new exact `sumAmounts` helper (BigInt cents, no float) plus its own tests, and the labels were put
  back. The worker self-reported that it did not run RED before implementing, which is a process deviation
  from S16 and is recorded rather than glossed.

L16 — T5 evidence after the parent's corrections (2026-10-17): 33 frontend unit tests, 6 e2e, lint and
  `svelte-check` clean. `formatMoney` formats by splitting on the decimal point so `Intl` never sees a float;
  `parseAmount` replaced `parseAmountToCents` and never calls `parseFloat`. The only arithmetic left in the
  client is `lineTotalAmount` (draft preview) and `sumAmounts` (page footing), both exact BigInt cents, both
  named as derived figures — the recorded total always comes from the server response.

L17 — T6 evidence (2026-10-17), on commit `6695c60`: 16 backend unit, 12 backend integration, 33 frontend
  unit, 6 e2e, `cargo fmt --check`, `clippy -D warnings`, `make sqlx-check`, `npm run lint`, `npm run check`.
  `cargo fmt --check` failed on the first pass — the one check that CI runs and that no test would have caught
  — and the formatting was folded into the commit that introduced it with a `--fixup` + `--autosquash` rebase,
  so the branch has two clean commits rather than three with a formatting tail.

## Log

L1 — Owner's original request, verbatim (2026-10-16):
> vamos a empezar a trabajar en un slice para cambiar un plan de diseño. Al principio decidi usar BIGINT para datos monetarios, pero vamos a migrar a NUMERIC o DECIMAL. Usando string para typescript para que no use float. No escribas codigo todavia, no tengo en claro los pros y contras de cada uno asi que antes de escribir codigo haceme un resumen del cambio. Estas de acuerdo? Que se usan softwares ya establecidos en la industria?

L2 — Owner correction, verbatim:
> datos no existe porque ya hice un down -v. Explicame mejor a y b antes

L3 — Owner decisions, verbatim, answering the option matrix:
> 1. Vamos con B pero no voy a usar mas de 2 decimales en el frontend, usaria NUMERIC (14,2) o es mejor usar 3 o 4 decimales para tener mejor presicion y formatear o redondear en el frontend?
> 2. rust_decimal::Decimal
> 3. No, decidirlo con la reversión de v1.5

L4 — Owner correction on the unit dimension, verbatim:
> te estas enroscando con el tema de las unidades y es irrelevante. Para que no te confundas eliminemos las unidades en productos o mejor dicho la unidad es solo decorativa, unit es lo mismo que g o kg. Al usar decimal eliminamos la funcion que aplica aritmetica para reformatear las moendas porque ya no es necesaria al suar decimal

L5 — Owner decisions, verbatim:
> 1. ARS hoy, multi-currency previsto
> 2. Quitar guard_product_unit

L6 — Evidence gathered before writing this document (2026-10-17):
- Current state: the four columns are `BIGINT` in `backend/migrations/0002_v1_sales_stock.sql:14,21,35,36`;
  `backend/src/money.rs:10` is `pub type Cents = i64`; `backend/src/routes/sales.rs:313` sums
  `COALESCE(SUM(total), 0)::bigint`; `frontend/src/lib/money.ts` holds `formatCents` (`cents / 100`),
  `parseAmountToCents` (`whole * 100 + fraction`) and `lineTotal`; five Svelte components import them.
- The claimed resolution trap was real but mis-attributed by me: it came from pricing *cents per base unit*,
  not from `BIGINT`. The owner is right (L4). `plan-v1a.md:77` recorded it as "the finest expressible price for
  a gram product is 1¢/g = $10.00/kg".
- 2-decimal prices would have reproduced that exact trap ($12.00/kg is not expressible at 2 decimals), which is
  why the price scale was probed. Once the unit is decorative the problem does not exist, and 2 decimals is
  correct everywhere.
- PostgreSQL docs: `numeric` is *"especially recommended for storing monetary amounts"*; `decimal` and
  `numeric` *"are equivalent"*; excess fractional digits are rounded **silently**; stored values carry no
  trailing zeroes so the declared scale is a maximum, not an allocation.
- Industry: Stripe and Square send integers in the currency's minor unit; PayPal sends a decimal **string**
  (`Money.value`) with a `currency_code`; Shopify declares `MoneyV2.amount` as `Decimal`; Google Ads uses
  micros of the account currency. Every one of them fixes the currency and derives the exponent from it —
  no system stores a per-row exponent.
- `plan-v1.md:31` rejected `NUMERIC` because it *"would allow sub-cent values (69.93¢) to be stored"*. That
  rejection was correct against an unconstrained `NUMERIC`. A declared `NUMERIC(14,2)` makes sub-cent values
  **irrepresentable**, which is the property that paragraph asked for; decision 3 is corrected accordingly.

L7 — Guard design measured against the live database (2026-10-17), before writing the migration. Four probes
  via `docker compose exec postgres psql`, on temp tables:
- `NUMERIC(14,2) CHECK (scale(v) <= 2)` receiving `1.235`: **inserted successfully** as `1.24`. PostgreSQL
  rounds to the declared scale *before* evaluating the CHECK, so the guard never saw the extra digit.
- `NUMERIC(14,6) CHECK (scale(v) <= 2)` receiving `25.00`: **rejected**, because `scale('25.00') = 2` but
  `scale('25.000000') = 6`. The guard is representation-dependent and false-rejects a valid amount.
- `NUMERIC(14,6) CHECK (v = round(v, 2))`: `25.00`, `25.000000` and `0` accepted; `1.235` and `0.036` rejected
  with `violates check constraint`. Value-based, representation-independent, loud.
- Magnitude: `100000000.00` raises `numeric field overflow` ("must round to an absolute value less than
  10^8"), so the declaration alone bounds the amount loudly.
  Decision: S1 and S3 amended to `NUMERIC(14,6)` + `CHECK (v = round(v, 2))`. The owner's 2-decimal semantic
  scale is unchanged; only the enforcement moved from a silent rounding to a loud rejection.
- Also confirmed: the local `sqlx-cli` was 0.8.6 against a 0.9 crate, and `make sqlx-check` passed with it
  (the 0.9-era cache is accepted for checking). CI installs `^0.9`; the local CLI was updated to 0.9.0 for
  regeneration.

L9 — Baseline captured on `main` before the first write (2026-10-17), all green: backend 11 unit + 12 integration,
  frontend 32 unit, 6 e2e, `npm run lint` and `npm run check` clean. Branch `feat/money-decimal` cut from that
  state.

L10 — T2 and T3 evidence (2026-10-17):
- Migration `0006_money_numeric.sql` applied to the live database. Verified by query: the four columns read
  `numeric(14,6)`; the four `>= 0` checks from 0002/0005 **survive** alongside the four new
  `= round(v, 2)` checks; `products_unit_guard` and `guard_product_unit()` are both gone (0 rows in
  `pg_trigger` / `pg_proc`); and inserting `1.235` into `products.list_price` fails with
  `violates check constraint "products_list_price_whole_cents"`.
- Backend: 16 unit tests + 12 integration tests green; `make sqlx-check` clean after regenerating `.sqlx`.
- **Bug 1, found by running the integration tests, not by reading:** the first implementation used a declared
  scale of 2. It compiled, passed every unit test, and silently accepted sub-cent values — because PostgreSQL
  rounds to the declared scale before any guard runs (L7). Fixed by moving to a declared scale of 6 plus the
  value guard S1/S3 now specify.
- **Bug 2, found the same way:** with the column declaring 6, the API returned `"692659.000000"` and a zero
  amount returned `"0"` instead of `"0.00"`. Root cause read from
  `sqlx-postgres-0.9.0/src/types/rust_decimal.rs:51` — `if digits.is_empty() { return Ok(Decimal::ZERO); }`,
  so PostgreSQL's zero (an empty digit array) decodes to a scale-0 `Decimal`, and `round(x, 2)` in the SELECT
  cannot help because the scale is lost at decode time. Fixed with an explicit
  `serialize_amount` used by every money field, plus `round(col, 2)` in the reads so a stored value does not
  leave at the column's declared scale. Confirmed afterwards: `curl /api/cash` returns
  `{"total":"692659.00"}`.
- Both bugs were invisible to unit tests and to reading the schema. The lesson recorded for the slice: the
  money contract needed an end-to-end check against the running API, which is why T6 runs the full suite.
- **Method note:** the local `sqlx-cli` was 0.8.6 against a 0.9 crate. `make sqlx-check` passed with it, but
  CI installs `^0.9`; the local CLI was updated to 0.9.0 before regenerating, so the cache is not written by a
  generation CI will refuse.

L12 — Slice boundary, recorded because the tool flagged it (2026-10-17). Commit `26da7ad` is **912 authored
  changed lines**, over the ~400-line guidance. It was committed as one unit deliberately, and the reason is
  mechanical rather than a wish to bundle: `sqlx::query!` verifies every query against the live schema *while
  the crate compiles*, so a commit carrying only the migration leaves the crate uncompilable and a commit
  carrying only the Rust changes points at columns that no longer exist. Migration, models, routes and the
  regenerated `.sqlx/` cache are one indivisible reviewable change. Two things soften the count and are worth
  naming so the number is not read as more hand-written code than it is:
- `odd/tasks/money-decimal-migration.md` (208 lines) is the specification document itself.
- `backend/.sqlx/*.json` (10 files) is tool-generated query metadata, required for the offline build and
  checked by CI.
  The remaining work is genuinely separable and stays that way: T5 (frontend) is its own commit, and T6 closes
  the feature. No further slicing is possible without breaking the compile, which is the honest statement of
  the trade-off rather than a preference for large commits.

L13 — Open point carried into T5 (not a blocker, decided here so it is visible): the sale form previews a live
  total before the sale is recorded (`plan-v1a.md` scope; `SaleDraft.svelte:94` sums `lineTotal` over the
  draft). With amounts as strings the client cannot sum them as text. The decision: the preview sums in
  **integer cents internally and never lets that value leave the browser** — it is a preview of an unrecorded
  draft, and the recorded total always comes from the backend response. This is the one place a cents-shaped
  integer survives, it is named as preview-only, and S6 still deletes the two formatting functions.

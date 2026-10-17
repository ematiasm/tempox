# v1.5-es-ar-locale — tasks (Engram mirror)

Full document: `odd/tasks/v1.5-es-ar-locale.md` (specs authority, verbatim user log).
Engram topic key: `tempox/odd/v1.5-es-ar-locale/tasks`.

**State: merged.** T1–T7 done, S1–S19 satisfied, every check green.
Branch `feat/v1.5-es-ar-locale` off `main` (`03abbd9`), merged 2026-10-17 as
[PR #44](https://github.com/ematiasm/tempox/pull/44) → `70a5e20`. CI 4/4 green on
the PR and on `main` after the merge; branches deleted, `main` level with
`origin/main`.

Scope: the es-AR locale half of v1.5 — UI language, ARS money and number
formatting in both directions, dates, a server-owned day boundary with a real
timezone, and the strings that had escaped the dictionary. `fr` is removed;
`es-AR` is the default and the choice is not persisted.

## Specs (one line each — the document is authoritative)

S1 the locale is a frontend concern; the one exception is the day.
S2 two languages, `es-AR` (default) + `en`; `fr` deleted; not persisted.
S3 one `Dict` type, two complete dictionaries with identical key sets.
S4 money formatted per locale (`$ 1.234,56`), wire format never moves.
S5 `parseAmount` reads per locale, always emits the en-US wire form; a pre-filled
   field goes through `amountInput` so it can always be read back.
S6 dates and times formatted per locale.
S7 `GET /api/day?at=` → `{from,to,prev,next,zone}`, shop timezone from config.
S8 the ledger asks the server what day it is; no client-side fallback range.
S9 a malformed `at` is `400`, not a 500; nothing written.
S10 every visible string comes from the dictionary, including the escaped ones.
S11 the unit label is localized; the value is never converted.
S12 no dictionary string carries a hardcoded amount or currency symbol.
S13 clean vocabulary both ways (`Consumidor Final`/`Anulada` vs `Walk-in`/`Annulled`).
S14 `<html lang>` follows the active language.
S15 the es-AR dictionary is shop vocabulary, not a literal translation.
S16 docs follow the behavior.
S17 test-first per unit, existing behavior preserved, differential check where shared.
S18 work-unit commits with their tests and docs; the last slice closes the document.
S19 a document's time is rendered in the shop's zone (`zone` from `/api/day`),
    never the browser's.

## Tasks

| ID | Task | Specs | Route | Status |
|---|---|---|---|---|
| T1 | Backend: `SHOP_TIMEZONE` + `GET /api/day` + integration test + docs row | S1, S7, S9, S16, S17 | inline | done |
| T2 | Locale core (`Lang`, `Locale` bundle, `fr` deleted, `<html lang>`) | S2, S3, S14 | inline | done |
| T3 | Money/number formatting per locale, both directions, all call sites | S4, S5, S17 | inline | done |
| T4 | Dates per locale + ledger range from the server | S6, S8, S19 | subagent + inline | done |
| T5 | Escaped strings into the dictionary; unit label; amounts out of prose | S10, S11, S12 | inline | done |
| T6 | `es.ts` as shop vocabulary; `en.ts` cleaned | S13, S15 | inline | done |
| T7 | e2e + docs + close (differential check, final green) | S16, S17, S18 | inline | done |

## Verification (summary — the document holds the commands and their output)

- Backend: 29 unit (`bins`), 22 integration against live PostgreSQL, `fmt`/`clippy`
  clean, `.sqlx` unchanged, `SQLX_OFFLINE=true` build OK.
- Frontend: 63 unit, `svelte-check` 0/0, `eslint` clean, `prettier` clean.
- e2e: 8 passed against a real backend on the branch build.
- Mutation checks on `day.rs` and `locale.ts` prove the new tests fail when the
  behavior is wrong.
- Differential check against `03abbd9`: the `en` locale reproduces the base
  implementation exactly across 50 cases, zero differences.
- S19 measured in a browser set to `UTC`, `America/Argentina/Buenos_Aires` and
  `Asia/Tokyo`: identical chip and times in all three.

## Defects found and closed

1. The draft pre-filled the price field with the wire form (`25.00`), which the
   es-AR parser correctly refuses — the sale was silently unrecordable. Closed
   with `amountInput` + a round-trip test.
2. The price placeholder showed `0.00` in a form that reads `1.234,56`. Closed
   with `amountHint`, whose test asserts the parser accepts it.
3. The day was the shop's but each document's hour was the machine's. Closed by
   returning `zone` and formatting with `Intl` + `timeZone` (S19).
4. A first attempt at 3 (deriving an offset client-side) was measured, found
   inverted and zone-blind, and reverted in full rather than patched.

## Process defects (recorded, not hidden)

- T1's unit tests were never RED: implementation and tests were written together.
  Closed afterwards with mutation checks, which is weaker evidence.
- Two e2e failures were test bugs (a locator keyed on a translated `aria-label`;
  an unchanged `'Main'`).
- A subagent's `npx` left an empty root `node_modules/`; removed after inspection.

## Log (summary — the document holds the verbatim text)

L1 the request: finish v1.5's locale; decide general-locale vs frontend-only.
L2 decision: `es-AR` default + `en`, remove `fr`.
L3 decision: the day boundary is a server endpoint with the timezone in backend config.
L4 decision: the language choice is not persisted.
L5 analysis: the backend has no human-readable string outside `GET /api/hello`.
L6 discovery: the scoping fix without the rendering fix leaves the page
   inconsistent; S7 gained `zone` and S19 was added.

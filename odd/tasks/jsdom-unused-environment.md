# jsdom-unused-environment — pendiente

Vitest is configured with `environment: 'jsdom'` (`frontend/vite.config.ts:99`), but
the whole frontend unit suite passes with no DOM at all. jsdom is installed,
versioned, audited by Dependabot and upgraded across five majors — and nothing in
the suite needs it.

This is recorded as debt, not as an open task. It was noticed while merging
Dependabot PR #60 (jsdom 25.0.1 → 30.1.3) and deliberately kept out of that merge
so the dependency bump stays a pure dependency bump.

**Gap 1 from this note is now closed** by `lib/draftLines.ts` and its 38 cases
(`refactor/draft-lines-rule`): the draft line rule no longer lives inside a
component, and it is tested without a DOM. The open question is only what to do
with the `jsdom` dependency and environment itself.

## Evidence

Measured on 2026-10-20 on `main` (jsdom 30.1.3, after #60), and earlier with
jsdom 25.0.1 with the same result. Three runs each, to not report a single-run
number:

```
cd frontend && npx vitest run                      → 9 files, 139 tests passed
                                                     ~810ms, "environment 93%"
cd frontend && npx vitest run --environment node   → 9 files, 139 tests passed
                                                     ~213ms
```

Identical counts. The difference is entirely the ~600ms jsdom spends building a
DOM nobody reads: under jsdom the profiler attributes 93% of the run to
`environment`, and under `node` there is no `environment` segment at all.

That 600ms is the *whole* of the cost inside the suite, and it is worth being
honest about the scale: the suite runs in under a second either way. Per run, this
is not a felt improvement.

### First, the flag has to actually bite

"The suite passes without jsdom" only means something if `--environment node`
really removes the DOM. Verified with a throwaway probe spec (deleted after):
`expect(typeof document).toBe('object')` **passes** under the configured jsdom and
**fails with `expected 'undefined' to be 'object'`** under `--environment node`.
So the DOM is genuinely present in the default run and genuinely absent in the
`node` run. The comparison is valid.

### Why, structurally

All nine specs under `frontend/tests/unit/` import pure derivations and never touch
the DOM. Their whole import surface is `vitest`, `#lib/*` modules (money, locale,
screenTrust, api types) and one helper from `tests/e2e/unique.ts`. Greps for
`matchMedia`, `getComputedStyle`, and direct `window.`/`document.` access in the
specs return nothing.

Two near-misses worth naming, because each one *would* have made jsdom
load-bearing:

- `tests/unit/money.test.ts` imports `#lib/i18n/index.svelte.ts` — a Svelte 5
  runes module (`$state`), not a component. Checked: it uses no `localStorage`,
  no `window`, no `document`. It cannot, by design — the code comments record that
  the language choice is deliberately **not persisted** (S2), so there is nothing
  to read from storage.
- `@testing-library/svelte` is a declared devDependency (`package.json:24`) but is
  imported by **no** spec in `src/` or `tests/`. Nothing in the suite mounts a
  component.

## Why it is not urgent

It costs install time and lockfile churn, not correctness. The suite is green
either way, and #60 landed as a clean patch-level bump with the environment
untouched. Concretely, the ~600ms per run is the whole of the cost inside the
suite; outside it, jsdom still drags its own dependency tree into the lockfile
(and #60 is what showed how heavy that tree is to move across majors).

## What a future change would have to decide

Not decided here, on purpose — these are product choices, not mechanical ones.

### First, an honest sizing of the gap

An earlier draft of this note claimed option 2 covers "a real gap: nothing tests a
component in isolation". That is technically true and **misleading** about the size
of the gap. Checked: the repo already follows a deliberate pattern of **extracting**
business rules out of components into `src/lib/*.ts` and testing them there, with no
DOM:

| module | unit cases |
| --- | --- |
| `lib/salesRefund.ts` (`planRefund`, `refundableOf`, `parseRefundQuantity`) | 28 |
| `lib/supplierReturns.ts` (`planReturn`, `returnableOf`, `parseReturnQuantity`) | 22 |
| `lib/money.ts` (BigInt arithmetic, both languages) | 20 |

`Ledger.svelte` is 1192 lines but its `refundPlan` **delegates** to `lib/salesRefund`,
and its own comment says so: "`planRefund` owns the rule so a unit test can pin it".
The strong rules are covered. Saying "there is no component coverage" and stopping
there would misrepresent this codebase.

### Where the gap actually is

Two concrete places, both narrower than "components". **Gap 1 is now closed**; it is
kept here as the record of what the extraction was for.

**1. (CLOSED) The `$derived` logic inside the draft components had no test at all.**
`SaleDraft.svelte:81–121` computed, inline:

- `complete` — a line counts only when quantity **and** price both parsed. A
  half-typed line is not money.
- `total` — reimplements the BigInt cent sum by hand instead of calling
  `sumAmounts`, which exists in `lib/money.ts` and is already tested.
- `hasInvalid` — distinguishes "field empty" from "field holds text that does not
  parse".
- `canRecord` — `complete.length > 0 && !submitting`.

Greps over `tests/unit/` for `complete`, `canRecord`, `hasInvalid` returned nothing,
and the e2e did not cover the invalid-line case either (no request-leaves / invalid
assertion in any spec). `PurchaseDraft.svelte` had the same shape. So **the rule that
decided what a valid line is lived in the component and no test pinned it** — exactly
the kind of rule that breaks silently.

Closed by `frontend/src/lib/draftLines.ts` + `frontend/tests/unit/draft-lines.test.ts`
(38 cases), with both components delegating. Same pattern as `salesRefund` and
`supplierReturns`. The three hand-rolled sums are now one (`draftTotal`), and the two
screens' different readiness rules are explicit rather than implicit (`canRecord` vs
`everyLineComplete`). No DOM involved — which is the point: the fix for "logic trapped
in a component" is extraction, not a DOM runtime.

**2. (STILL OPEN) The render layer above that rule.** `hasInvalid` read
`lines[i].qtyText.trim()` inside the component; extracted, it takes a plain string and
is now tested with no DOM. What remains uncovered is the layer above it: that the
`<input>` is actually wired to it, and that the warning renders when it should. That
is genuine render/interaction coverage — option 2, where a DOM is earned.

### The options, including one this note originally missed

1. **Drop jsdom, switch the global to `environment: 'node'`.** Removes a
   devDependency and the `@asamuzakjp/*` / `@csstools/*` tree that #60 had to move
   across five majors. Saves ~600ms per run. Reversible in one line, and per-file
   `// @vitest-environment jsdom` still gives any future spec a DOM — see above.
   Risk: purely signalling, so the reason has to live next to the config.
2. **Add component specs with `@testing-library/svelte`** (already declared at
   `package.json:24`, imported by nobody) for render and interaction coverage —
   props, events, what a component emits. That is a real category nothing covers
   today. Needs a DOM *for those files only*, which the docblock provides.
3. **Leave it alone.** Defensible while the suite is small.
4. **Extract a rule into `lib/` and test it with no DOM.** This is what was done
   for the draft line rule (`lib/draftLines.ts`), following
   `salesRefund`/`supplierReturns`. Worth reaching for whenever a rule is found
   buried in a component's `$derived`.

**These are not a single either/or.** Option 4 is done. Option 1 and option 2 can
both happen, in either order, because the environment is per file. The honest
sequencing is: do 4 whenever a rule turns up unreachable (cheapest, no
infrastructure), reach for 2 when render/interaction coverage is the actual goal,
and treat 1 as a tidy-up that is always one line from being undone.

What would justify **doing nothing** is also worth stating: a single devDependency
that Dependabot keeps current costs about one lockfile churn a quarter. What made
#60 expensive was the five-major jump, not jsdom's existence. So "leave it alone"
is a defensible default, and this note exists so the choice is deliberate rather
than forgotten.

### The two options are NOT incompatible — this note got that wrong

An earlier draft argued that dropping jsdom and adding component specs were
mutually exclusive. **That is false**, and the measurement below is why.

Vitest supports an environment **per file**, via a docblock at the top:

```ts
// @vitest-environment jsdom
import { render } from '@testing-library/svelte';
```

Verified with two throwaway specs run together (deleted after): one carrying
`// @vitest-environment node` saw `typeof document === 'undefined'`, while a second
spec with no docblock inherited the global `jsdom` and saw `typeof document ===
'object'`. Both passed in the same run. So the global can be `node` while specific
files opt into a DOM.

That makes the shape of a future decision much cheaper than this note first said:

| what you remove now | cost to bring it back |
| --- | --- |
| `environment: 'jsdom'` in the config | change one line to `'node'` |
| `jsdom` from `devDependencies` | `npm i -D jsdom` |
| the `@asamuzakjp/*` / `@csstools/*` tree | returns on that install |

All three are reversible. The real risk is not technical, it is **signalling**:
with the global set to `node`, the first future spec that touches `document` fails
with `document is not defined` and no clue why. That is what the comment next to
the config has to prevent — and this note is that explanation.

Also verified: **`@testing-library/svelte` does not depend on jsdom.** Its peers
are `svelte`, `vite`, `vitest`; its dependencies are `@testing-library/dom` (no
peers) and `@testing-library/svelte-core`. So the component-testing library can be
installed and idle without a DOM runtime, and only a spec that actually mounts a
component needs one.

## Where this fits in the existing test flow

For context, because this note only makes sense against it. The authority is
`docs/en/testing.md`; this is the shape, not a replacement:

| Level | What it proves | Needs | Runtime |
| --- | --- | --- | --- |
| Backend unit (`cargo test --lib --bins`) | pure logic: money, day, totals | nothing | ~0s |
| **Frontend unit (Vitest)** | pure derivations | **jsdom today, nothing in practice** | ~810ms, ~600 of it jsdom |
| Integration (`cargo test --test integration`) | real schema + handlers, a database per test | postgres | ~3.3s |
| End to end (Playwright) | the real screen: compiled bundle, browser, real API | postgres, chromium, one backend on `:3099` | ~7.5s |

Two structural facts worth carrying into any decision here:

- **The e2e is the only level that exercises a `.svelte` file today.** 4594 lines of
  components rest entirely on 16 e2e cases that cost a database and a browser to run.
  That is why adding component-level coverage is attractive: it is not duplication,
  it is a cheaper tier for a class of bug that currently only expensive tests can see.
- **`make test` runs the three levels in sequence** (`test: test-unit test-backend
  test-e2e`), and the sequencing is load-bearing, not stylistic: `make test-backend`
  runs `scripts/test-db.sh`, which drops and recreates `tempox_test` — the same
  database the e2e backend is connected to. See the watch-out below.

## Watch-out if jsdom is ever dropped

`frontend/vite.config.ts` already documents its `test.include` scoping
(`tests/unit/**` only, because `tests/e2e/unique.ts` is a pure helper the e2e
specs depend on and is unit-tested from there). That comment block is load-bearing
and predates this note — do not fold it into an environment change.

Related note, measured while doing this work: `make test-e2e` and
`make test-backend` must never run concurrently. `make test-backend` runs
`scripts/test-db.sh`, which drops and recreates `tempox_test` — the same database
the e2e backend is connected to. Observed: 3–10 e2e failures with 30s timeouts,
against 16 passed in 7s when run alone. `make test` runs them in sequence for
exactly this reason.

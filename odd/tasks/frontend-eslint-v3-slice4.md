# frontend-eslint-v3-slice4 — eslint-plugin-svelte 3 + eslint 10, as one change

Outcome: the frontend lints on `eslint-plugin-svelte` 3 and `eslint` 10, the parser the v3 config expects for
`*.svelte.ts` is wired up, and the four lint findings the v3 rules raise are fixed in code rather than
suppressed. Dependabot's #31 and #32 propose these two bumps as separate PRs and neither can land alone: #31
does not install (L2) and #32 fails lint (L3, L4).
Specs authority: this document. Scope decided by the owner 2026-10-16 (L1).

## Specs

S1. `frontend/package.json` moves `eslint` to `^10.12.0` and `eslint-plugin-svelte` to `^3.23.1` **together,
   never separately**. The pair is the unit: `eslint-plugin-svelte@2.46.1` peers `eslint ^7.0.0 || ^8.0.0-0 ||
   ^9.0.0-0`, so `eslint@10` with the v2 plugin is an ERESOLVE failure on install, before any code runs (L2).
S2. `@eslint/js` becomes an explicit `devDependency` at the version matching the new eslint major. It is
   imported by `eslint.config.js` today and resolved only **transitively** from `eslint@9.39.5`, which lists it
   as a dependency; `eslint@10.12.0` does not, so the import stops resolving the moment eslint moves (L5).
S3. `eslint.config.js` extends the block that sets `parserOptions.parser: ts.parser` to also cover
   `**/*.svelte.ts` and `**/*.svelte.js`, the setup plugin-svelte v3 documents. Without it, v3's own `flat/base`
   config claims `*.svelte.ts` for `svelte-eslint-parser` with no TypeScript sub-parser and
   `src/lib/i18n/index.svelte.ts` fails to parse (L4). This is a configuration gap v2 never exposed, not a code
   defect.
S4. `src/lib/components/Ledger.svelte` stops holding a mutable built-in `Set` in reactive state and uses
   Svelte's reactive alternative, clearing `svelte/prefer-svelte-reactivity` at `110:18`. Behaviour is
   unchanged: clicking a sale id still opens and closes its lines, and every reset path still empties the set.
S5. `src/routes/+layout.svelte` routes both internal links through `resolve()` from `$app/paths`, clearing the
   two `svelte/no-navigation-without-resolve` errors at `20:22` and `27:13`. The rendered destinations are the
   same two paths as before (no `paths.base` is configured), so this is a correctness fix for a future base path,
   not a navigation change.
S6. All five checks pass on the branch: `lint`, `check`, `test:unit -- --run`, `build`, `test:e2e`. **No test
   assertion changes**: this is a linter and dependency move. If a test needs editing to pass, that edit is a
   behaviour claim and is recorded as such. No new test is added: the behaviour S4 and S5 touch is already
   covered end to end (`tests/e2e/sale.spec.ts` clicks `.doc-id` and asserts the lines appear;
   `tests/e2e/shell.spec.ts` asserts both rail links navigate).
S7. The lockfile is updated **surgically**. `@playwright/test` stays at `1.64.0` and no package outside the
   moved set changes version, appears or disappears. slice 1 measured that a regenerated lock moved 76 packages
   and took `@playwright/test` to 1.64.1, which broke e2e on a missing chromium build
   (`odd/tasks/frontend-vite8-slice1.md`, L13).
S8. `frontend/Dockerfile` and `.github/workflows/ci.yml` are unchanged. The Node floor is satisfied: `eslint@10`
   needs `^20.19.0 || ^22.13.0 || >=24`, and this repository is on Node 26 (`node-version: "26"`,
   `node:26-bookworm-slim`).
S9. One work-unit commit (Conventional Commit) on branch `chore/eslint-v3`. S1-S5 cannot be split: the parser
   config, the two code fixes and the dependency move must land together, or every intermediate commit is red
   (`lint` fails on the v3 rules until the code is fixed, and on the parser until the config is fixed).
S10. Once this slice is merged, #31 and #32 are closed with a pointer to it, following slice 2's precedent
   (`odd/tasks/frontend-sveltekit3-slice2.md`, S10). **Not before the merge**: slice 2's log records that
   closing early was premature, because Dependabot's post-merge bookkeeping has not yet run.

## Tasks

| ID | Task | Specs | Status | Evidence |
|---|---|---|---|---|
| T1 | Branch `chore/eslint-v3`; capture the pre-change baseline (lint green on v2) and the RED | S6 | done | L7 |
| T2 | Move the pair + declare `@eslint/js`; update the lock surgically | S1, S2, S7 | done | L8 |
| T3 | Fix the parser config for `*.svelte.ts` | S3 | done | L9 |
| T4 | Fix the two code findings (`Ledger.svelte`, `+layout.svelte`) | S4, S5 | done | L9, L10 |
| T5 | Verify: five checks, `check` at 0 errors, and the lock guards of S7 | S6, S7 | done | L10, L11 |
| T6 | Commit, push, open the PR | S9 | done | `34b8bfb` work unit + `0a86fbc` docs, PR #37 |
| T7 | CI green on the PR | S6 | done | L12 |
| T8 | Independent read-only verification (risk item 5: dependency + lint-config change) | all | done | L13 |
| T9 | Merge, then close #31 and #32 with pointers | S10 | done | L14 |

## Log

L1 (user, decision, 2026-10-16, verbatim): "si arrancalo en un nuevo branch como slice". This authorises the
step 3 that was put to the owner as: fix the lint findings and the parser error, then rebase #31 — in a branch
with its own slice document. The owner's earlier decision in the same thread is what makes the scope: the scan
of Dependabot's PRs left #31 and #32 as the only two open, and the owner chose to make the Node declaration true
before merging #30 rather than holding the types back.

L2 (exploration, 2026-10-16, verified against the registry and the installed tree, not recalled): the pair cannot
be separated, and the obstacle is not the one the scan's headline suggested.

| package | `peerDependencies.eslint` | accepts eslint 10 |
|---|---|---|
| `eslint-plugin-svelte@2.46.1` (installed) | `^7.0.0 \|\| ^8.0.0-0 \|\| ^9.0.0-0` | **no** |
| `eslint-plugin-svelte@3.23.1` (proposed, #32) | `^8.57.1 \|\| ^9.0.0 \|\| ^10.0.0` | yes |
| `typescript-eslint@8.71.2` (already installed, from #28) | `^8.57.0 \|\| ^9.0.0 \|\| ^10.0.0` | **yes already** |
| `eslint-config-prettier@10.1.8` | `>=7.0.0` | yes |

The PR #31 ERESOLVE log blames the v2 plugin, and that is the whole of it: `typescript-eslint` already widens to
`^10.0.0` at the version #28 landed. So the only thing standing between `eslint@10` and a clean install is the
plugin version — which is #32. That is why this is one slice and not two rebases.

L3 (exploration, 2026-10-16, reproduced from #32's CI log and located in the tree): four findings, three of them
code and one of them not.

| finding | location | rule |
|---|---|---|
| mutable built-in `Set` in reactive state | `src/lib/components/Ledger.svelte:110:18` | `svelte/prefer-svelte-reactivity` |
| `href` literal without `resolve()` | `src/routes/+layout.svelte:20:22` | `svelte/no-navigation-without-resolve` |
| `href` binding without `resolve()` | `src/routes/+layout.svelte:27:13` | `svelte/no-navigation-without-resolve` |
| `Parsing error: Unexpected token {` | `src/lib/i18n/index.svelte.ts:3:13` | not a rule — the parser itself |

The fourth is the one worth separating, and L4 is why. The three real findings are small: `expanded` is
`$state<Set<string>>`, and `tabs` is a `$derived` array whose `href` values are route-relative strings.

L4 (root cause, 2026-10-16 — the finding this slice turns on): the parsing error is a **configuration gap that
plugin v3 opens and v2 never had**. Read from the published tarballs, not from documentation:

- v2's `flat/base` config has exactly one block for Svelte files:
  `files: ['*.svelte', '**/*.svelte']`. It never claimed `*.svelte.ts`.
- v3's `flat/base` adds a **second** block:
  ```js
  {
    name: 'svelte:base:setup-for-svelte-script',
    files: ['*.svelte.js', '*.svelte.ts', '**/*.svelte.js', '**/*.svelte.ts'],
    languageOptions: { parser }   // svelte-eslint-parser, no TypeScript sub-parser
  }
  ```
- This repository's `eslint.config.js` sets `parserOptions: { parser: ts.parser }` for `files: ['**/*.svelte']`
  only. `**/*.svelte` does **not** match `index.svelte.ts` — the name ends in `.ts` — so the file is parsed by
  bare `svelte-eslint-parser`, which does not understand `import type { Dict, Lang } from './types'` on line 3,
  and reports `Unexpected token {` at column 13, exactly where that `{` is.

So the file was never linted as TypeScript because nothing claimed it under v2, and under v3 something claims it
with the wrong parser. The fix is the glob v3's own README documents
(`files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js']`), not a change to `index.svelte.ts`. Worth stating
plainly because the natural misreading of the CI log is "this file has a syntax error":
`src/lib/i18n/index.svelte.ts` is valid Svelte 5 and valid TypeScript, and `svelte-check` reports 0 errors on it
today.

Also confirmed while measuring, so the fix is known to be available: `SvelteSet` is exported from
`svelte/reactivity` in the installed `svelte@5.57.2` (`export class SvelteSet<T> extends Set<T>`), and
`resolve()` is exported from `$app/paths` in the installed `@sveltejs/kit@3.1.0`, typed as
`resolve<T extends RouteIdWithSearchOrHash | PathnameWithSearchOrHash>(...args)` with
`RouteId = '/' | '/notes' | '/products' | '/sales'` generated for this app.

L5 (exploration, 2026-10-16, verified against the registry and the lock — a **fifth** problem the CI log could
not show): `@eslint/js` is imported by `eslint.config.js` but is **not** declared in `package.json`. It resolves
today only because `eslint@9.39.5` carries it as a dependency (`node_modules/eslint` → `@eslint/js 9.39.5` in
the lock). `eslint@10.12.0`'s dependency list does not contain `@eslint/js` at all. So the eslint bump silently
removes a module the config imports, and the failure would surface as a module-resolution error at lint time with
nothing in the diff pointing at it. It is checked against the registry, and it is why S2 exists.

The version pairing is not free either: `@eslint/js@10.0.1` peers `eslint ^10.0.0`, and it is the release line
matching the eslint major being installed.

L6 (plan, 2026-10-16, before the first write): the branch is `chore/eslint-v3` off `main` at `e735f8b`. The
change is one indivisible commit touching `frontend/package.json`, `frontend/package-lock.json`,
`frontend/eslint.config.js`, `frontend/src/lib/components/Ledger.svelte` and
`frontend/src/routes/+layout.svelte` (S9). The RED to observe is `npm run lint` failing with the four findings
of L3 after the dependency move and before the fixes; GREEN is `lint` plus the other four checks passing.

The lock strategy is the one slice 1 established and not the obvious one: `rm package-lock.json && npm install`
is **wrong** here too, because it re-resolves the whole tree and slice 1 measured that it moved
`@playwright/test` 1.64.0 → 1.64.1, which fails e2e on a chromium build the machine does not have. The plan is
the surgical path: start from `main`'s lock, drop only the entries being moved and their incompatible children,
synchronise the root `devDependencies`, then resolve with `npm install --package-lock-only`, and count what
actually moved (S7).

L7 (measurement, 2026-10-16, T1 — the baseline and the RED, captured in that order): baseline on `main` at
`e735f8b` before any write: `npm run lint` **exit 0** on plugin v2, `@playwright/test` 1.64.0, 335 packages in
the lock.

After the dependency move and the `npm ci` that followed it, with **no** code or config fix applied yet, the RED
appeared exactly as L3 predicted — same four findings, same locations, and `lint` exit 1:

```
src/lib/components/Ledger.svelte
  110:18  error  Found a mutable instance of the built-in Set class. Use SvelteSet instead  svelte/prefer-svelte-reactivity

src/lib/i18n/index.svelte.ts
  3:13  error  Parsing error: Unexpected token {

src/routes/+layout.svelte
  20:22  error  Unexpected href link without resolve()  svelte/no-navigation-without-resolve
  27:13  error  Unexpected href link without resolve()  svelte/no-navigation-without-resolve

✖ 4 problems (4 errors, 0 warnings)
```

This is the RED of the slice: the four findings are real and pre-existing in the sense that no code changed to
create them — the plugin's rules are what changed. L4 explains why one of them is not a finding in the code at
all.

L8 (measurement, 2026-10-16, T2 — the surgical lock, measured rather than asserted): from `main`'s lock, the 17
entries belonging to the moved subtree were dropped (the root `devDependencies` synchronised to the new
`package.json`), then resolved with `npm install --package-lock-only`.

Resolved to the intended versions: `eslint` 10.12.0, `eslint-plugin-svelte` 3.23.1, `@eslint/js` 10.0.1,
`svelte-eslint-parser` 1.9.0 (was 0.43.0). `npm ci` then installed all four cleanly, which is what proves the
manifest and the lock agree.

**59 entries changed, not the handful S7's wording implies**, so the guard was checked properly rather than
counted. Every changed entry was traced along dependency edges to one of the four roots — `eslint`,
`eslint-plugin-svelte`, `@eslint/js`, `svelte-eslint-parser` — using the union of the old and new dependency
graphs:

```
entradas con version distinta/alta/baja: 59
sin camino a una raiz: 0

  keyv:    eslint -> file-entry-cache
  js-yaml: eslint -> @eslint/eslintrc
  chalk:   eslint -> chalk
  globals: eslint-plugin-svelte -> globals
```

So the honest reading of S7 is not "few entries changed" but "**no** entry outside the moved subtree changed",
and that is the claim the trace establishes. The packages that matter for other checks are all held:
`@playwright/test` **1.64.0** (unchanged), and likewise `svelte` 5.57.2, `vite` 8.3.4, `vitest` 5.0.5,
`typescript` 6.0.3, `@sveltejs/kit` 3.1.0, `@sveltejs/adapter-node` 6.0.0, `prettier` 3.9.10, `jsdom` 25.0.1.
The lock root's `devDependencies` match `package.json` exactly, and its key order is alphabetical like
`package.json`'s rather than insertion-ordered, so no key-order churn leaked into the diff.

One note on the mechanism, because it is easy to get wrong in the other direction: `npm install --package-lock-only`
reported `up to date in 4s` and looked like a no-op. It was not — it had re-resolved the dropped entries. Reading
the resulting versions rather than the command's summary is what established that.

L9 (measurement, 2026-10-16, T3 and T4 — each fix verified against its own finding):

| after | lint | what changed |
|---|---|---|
| T2 only (no fixes) | **4 errors** | the RED of L7 |
| + T3 (parser globs) | **3 errors** | `index.svelte.ts` parsing error gone; the three code findings remain |
| + T4 (both code fixes) | **0 errors** | `lint` exit 0, prettier check clean |

Splitting the two steps was deliberate: it shows the parser glob is what clears the parsing error, rather than
bundling it with the code fixes where either could have been the cause.

One real obstacle came out of T4 and is worth recording, because the obvious fix did not work. Resolving the
`{#each}` link as `href={resolve(tab.href)}` **failed `check`** with three errors of its own:

```
Error: Argument of type '[string]' is not assignable to parameter of type
  '[route: "/sales"] | [route: "/products"] | [route: "/"] | [route: "/notes"] | ...'
```

`resolve()` is typed against this app's own route ids (`RouteId = '/' | '/notes' | '/products' | '/sales'`), not
against any string, so a `href: string` field cannot be passed to it. The greens came from keeping the route ids
as literals with `as const` on the `tabs` array and calling `resolve(tab.href)` where the value is rendered or
compared. `check` then reported 0 errors. Recorded because it is the kind of fix whose first version passes
`lint` and fails `check`, and only running both catches it.

L10 (verification, 2026-10-16, T5 — the five checks on the branch):

| check | result |
|---|---|
| `npm run lint` | exit 0 |
| `npm run check` | exit 0 · `svelte-check found 0 errors and 0 warnings` |
| `npm run test:unit -- --run` | exit 0 · 3 files, **32/32** passed |
| `npm run build` | exit 0 · `✔ done` |
| `npm run test:e2e` | exit 0 · **6 passed** |

S6 holds as written: `git diff main -- frontend/tests/` is **empty**, so no test assertion was touched, and no
test was added. The e2e half is the one that matters for S4 and S5, and it passed on the real stack
(postgres + backend up, health 200) — `sale.spec.ts` clicks `.doc-id` and asserts the lines appear, which is the
`SvelteSet` behaviour, and `shell.spec.ts` asserts both rail links navigate, which is the `resolve()` behaviour.

L11 (delivery, 2026-10-16, T6): work unit `34b8bfb` — `build(deps): move the frontend to eslint 10 and
eslint-plugin-svelte 3` — carrying all five files together (S9: no split leaves a green intermediate).

L12 (evidence, 2026-10-16, T7): CI passed on the PR's first run. Run `38902615719` on `0a86fbc`, all four jobs
`success`:

| job | result | the part that matters here |
|---|---|---|
| Frontend (SvelteKit) | pass 40s | `node-version: 26`, and `added 270 packages` on a clean runner |
| Frontend image (Docker) | pass 24s | the image builds with the new lock |
| End to end (Playwright) | pass 1m32s | the 6 e2e tests, on the runner's own stack |
| Backend (Rust) | pass 1m9s | unaffected by the move |

Why the Frontend job is the load-bearing one here: it runs `npm install` (not `npm ci`) followed by `lint`,
`check`, `test:unit` and `build` on a tree with **no** pre-existing `node_modules`. So its green is the first
evidence that the three fixes hold on a from-scratch install of the new dependency pair rather than on the
working tree that produced them. `npm ci` is exercised by the image job, which fails loudly if the manifest and
the lock disagree.

One note on a near-miss worth recording: the PR body originally read "Closes Dependabot's #31 and #32", and
GitHub does **not** treat that as a closing keyword for a pull request, so nothing was auto-closed — confirmed:
both stayed `OPEN` with `closingIssuesReferences: 0`. It was reworded anyway, because a reader could take it as
a closure already declared and skip the manual step S10 requires.

L13 (independent verification, 2026-10-16, T8 — `gentle-ai-verify`, read-only, separate context): **zero
defects, S1-S9 PASS**, every claim reproduced with observed output rather than accepted. This is the
verification the change's risk class requires (item 5), and it ran before the merge.

What it reproduced rather than took on trust, which is the part that matters:

- **The pair claim, by construction.** It built a fresh project and installed `eslint@10.12.0` beside
  `eslint-plugin-svelte@2.46.1`: `npm error code ERESOLVE`, exit 1. S1's "neither can land alone" is now a
demonstrated fact, not an inference from peer ranges.
- **The parser root cause, from both tarballs** (`npm pack`, not documentation): v3's `flat/base.js` has the
  `setup-for-svelte-script` block with `files: ['*.svelte.js','*.svelte.ts','**/*.svelte.js','**/*.svelte.ts']`
  and a bare parser; v2's has only `files: ['*.svelte','**/*.svelte']` and no such block. It then isolated the
  cause by cross-swapping: new dependencies + `main`'s config → exactly the one parsing error on line 3; branch
  config + `main`'s code → the parsing error gone, three code findings left. That is L4 confirmed as a
configuration gap rather than a source defect, established by experiment rather than by reading.
- **S2's claim, negatively.** It deleted `@eslint/js` from a scratch copy and resolved: the package vanishes
  from the lock while eslint stays at 10.12.0. The bump really does remove a module the config imports.
- **S7's 59, counted from the trees:** 25 version-changed + 11 added + 23 removed = 59, with
  `@playwright/test` at 1.64.0 and the identical `resolved` URL — the guard this strategy exists for.
- **S6's negative claim:** `git diff main -- frontend/tests/` empty, both e2e specs byte-identical.
- `npm ci` from the committed lock, both in the worktree and in a clean `git archive 34b8bfb` scratch: exit 0.

One correction to my own analysis, recorded because it was a gap in L8 rather than a nicety: my reachability
trace left **4** entries unexplainable — the nested
`@typescript-eslint/typescript-estree/node_modules/{minimatch,brace-expansion,balanced-match}` and
`@typescript-eslint/visitor-keys/node_modules/eslint-visitor-keys`. I had accepted them on the strength of the
other 55. The verifier explained them: they are **dedup hoists** from the moved subtree. `eslint@10` requires
`minimatch ^10.2.5` and `@eslint/config-array@0.23.5` requires `^10.2.4`, which unify with
`typescript-estree`'s `^10.2.2` at 10.2.6, so `main`'s nested older copies are no longer needed; the same for
`eslint-visitor-keys` 4→5. So the trace's conclusion was right and its method was incomplete, which is worth
writing down: "55 of 59 reachable" is not the same evidence as "59 of 59 accounted for".

Both of its durable-probe suggestions are **recorded as follow-ups and deliberately not taken**, because they
add tests and S6 scopes this slice to no new tests: (1) a lockfile guard test asserting the lock root's
`devDependencies` deep-equal `package.json`'s and `@playwright/test === '1.64.0'`, which would have caught a
regenerated lock; (2) keeping the staged-config lint probe as CI discipline. They are decisions for the owner,
not for this slice.

L14 (closing, 2026-10-16, T9 — the merge, and who actually closed #31 and #32): PR #37 merged as **`fb540de`**
with the repository's convention (a merge commit, matching #18, #19, #20, #21 and the Node 26 slice). `main`'s
own run `38904312397` on that sha came back **success**, all four jobs.

S10 said #31 and #32 are closed *after* the merge with a pointer. They **were not closed by hand**, and the way
that resolved is worth recording because a state read taken too early looks exactly like a refuted prediction:

| read | #31 | #32 |
|---|---|---|
| immediately after the merge (12:58) | OPEN | OPEN |
| ~2 minutes later (13:00) | **CLOSED** | **CLOSED** |

Both were closed by `dependabot[bot]` itself — verified from the issue events, not assumed — each with its own
comment:

```
#32 CLOSED 2026-10-16T13:00:25Z  "Looks like eslint-plugin-svelte is up-to-date now, so this is no longer needed."
#31 CLOSED 2026-10-16T13:00:27Z  "Looks like eslint is up-to-date now, so this is no longer needed."
```

So no manual close was needed or made, and S10's intent (that they stop being proposed once the replacement is
on `main`) is satisfied by Dependabot's own post-merge bookkeeping. `odd/tasks/frontend-sveltekit3-slice2.md`
L11 records the identical lesson from the SvelteKit 3 slice: reading the Dependabot PRs seconds after a merge
shows `OPEN` and is **premature**, not contradictory. The honest move was to say what the read showed and
re-read, which is what happened here.

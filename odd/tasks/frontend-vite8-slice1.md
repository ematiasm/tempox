# frontend-vite8-slice1 — Vite 8 + vite-plugin-svelte 7 + vitest 5 on SvelteKit 2

Outcome: the frontend runs Vite 8, `@sveltejs/vite-plugin-svelte` 7 and `vitest` 5 while SvelteKit stays on
2.x, and the branch is green from its first commit. The bundler move and the test-runner move land **together**
because a first attempt proved they cannot be separated (L7, L11).
Specs authority: this document. Scope decided by the owner 2026-10-15 (L2), reshaped by the owner after the
first attempt failed (L11).

## Specs

S1. `frontend/package.json` moves `vite` to `^8.3.4`, `@sveltejs/vite-plugin-svelte` to `^7.3.1`,
   `@sveltejs/kit` to `^2.70.3` and `vitest` to `^5.0.5`. SvelteKit stays on `2.x`. No other dependency moves.
S2. `frontend/package-lock.json` is regenerated and committed with those versions, and **no `vitest` or
   `vite-node` subtree nests its own `vite@5`** — that nesting is the defect this slice exists to remove.
S3. `npm run lint`, `npm run check`, `npm run test:unit -- --run`, `npm run build` and `npm run test:e2e` all
   pass on the first commit of the branch. **No commit on this branch may leave any of the five red.**
S4. The 32 existing unit tests keep their assertions unchanged. The move is a dependency move, not a test
   rewrite: if a test needs editing to pass, that edit is a behaviour claim and is recorded as such.
S5. Nothing under `src/` carries a `server` segment at the end of this slice. `src/lib/api.ts` stays where it
   is and keeps its name: it is the browser HTTP client, imported by four components, two routes, `money.ts`
   and two unit tests, and SvelteKit 3 rejects those client imports once a `server` segment is in the name.
   Introducing a genuinely server-only module is a slice 2 decision.
S6. The check in S5 is a guard against regression, verified and recorded:
   `find src tests -name '*.server.*' -o -name 'server.ts'` returns nothing.
S7. The stale `allowScripts` entry (`esbuild@0.21.5`, a version only Vite 5 depended on) is removed: after
   the move, `npm ls esbuild` returns **empty** — Vite 8 consumes `esbuild` only as an **optional peer**
   (`peerDependenciesMeta.esbuild.optional: true`), which is not installed here, and no package in the tree
   has an install script left to approve. Corrected from the first draft of this spec, which said to approve
   `esbuild@0.28.2` at the root: that package is not in the tree either (L12).
S8. No SvelteKit 2 → 3 configuration changes land in this slice: `svelte.config.js` stays as it is, and the
   only `vite.config.ts` change is the `defineConfig` import the library move requires (L7).
S9. `Dockerfile` and `.github/workflows/ci.yml` keep working unchanged. The Node floors are satisfied as they
   stand: Vite 8 needs `^20.19.0 || >=22.12.0`, vitest 5 needs `^22.12.0 || ^24.0.0 || >=26.0.0`, and both the
   `node:24-bookworm-slim` image and CI's `node-version: "24"` clear them (L3, L8).
S10. Each task ends green and is its own work-unit commit (Conventional Commit) on branch `chore/vite8-slice1`,
   with the evidence recorded in `## Tasks`.

## Tasks

| ID | Task | Specs | Status | Evidence |
|---|---|---|---|---|
| T1 | Branch `chore/vite8-slice1`; capture a green baseline on Vite 5 first | S3 | done | branch created on `12d13a5`; baseline on Vite 5: lint 0·2s, check 0 errors·3s, test:unit 32 passed·1s, build 0·3s |
| T2 | Move all four dependencies; regenerate the lock; install | S1, S2 | done | `npm install` clean; root: vite 8.3.4, plugin-svelte 7.3.1, kit 2.70.3, vitest 5.0.5. Lock strategy corrected in L13 (45 packages, not 76) |
| T3 | Fix `vite.config.ts`: `defineConfig` from `vitest/config` | S8 | done | `check` 0 errors (was 1); the same change was 1→2 errors while vitest 2 nested Vite 5 (L7) |
| T4 | Verify: no nested `vite@5`, and all five checks green before the first commit | S2, S3, S4 | done | `node_modules/vite` 8.3.4 is the **only** vite in the lock; lint 0, check 0 errors, test:unit 32/32 on vitest v5.0.5, build 0 in 2s, e2e 6/6 — on a from-scratch install (300 packages) |
| T5 | Remove the stale `allowScripts` entry | S7 | done | `npm install-scripts prune`: "Removed 1 unused allowScripts entry: esbuild@0.21.5 (package not installed)"; field now absent, `npm ls esbuild` empty |
| T6 | Guard check: no `server` segment under `src/` | S5, S6 | done | `find src tests -name '*.server.*' -o -name 'server.ts'` returns nothing; `src/lib/api.ts` in place; `svelte.config.js`/`tsconfig.json`/`eslint.config.js` untouched (S8) |
| T7 | Closing: surgical lock, `test:e2e`, Dockerfile + CI review, slice 2 gate note | S3, S9 | done | e2e 6/6 on the surgical lock; `Dockerfile` and `ci.yml` unchanged and both clear the Node floors (vite `^20.19\|>=22.12`, vitest `^22.12\|^24\|>=26`, both get 24); slice 2 gate note in L4 |

Work unit: `fc4467f` — `build(deps): move the frontend to Vite 8 with vitest 5` (S10). The commit carries the
three files together, because the config fix and the dependency move are one indivisible change: the same
`defineConfig` edit that makes `check` pass under vitest 5 is the edit that fails under vitest 2 (L7).

## Log

L1 (user, verbatim): "vamos a crear el slice para migrar primero vite 8 + plugin-svelte 7 con kit 2 todavía, después kit 3 + adapter"

L2 (user, decision, 2026-10-15): scope of this slice = **vite 8 + plugin-svelte 7 only** — kit to 2.70.3,
everything else untouched, no config changes; the vitest resolution and the test config are explicitly out of
scope. **Superseded by L11**, which reshaped this scope after the first attempt failed.

L3 (exploration, 2026-10-15, verified against npm and the installed tree, not recalled): the versions the
move needs, and the one hard question in it.

| package | installed (lock) | required for Vite 8 | note |
|---|---|---|---|
| `vite` | 5.4.21 | 8.3.4 | Vite 8 bundles `rolldown` 1.2.12 in place of `rollup` + `esbuild` |
| `@sveltejs/vite-plugin-svelte` | 4.0.4 | 7.3.1 | peer `vite ^8.0.0` |
| `@sveltejs/kit` | 2.70.1 | 2.70.3 | peer is `vite ^5.0.3 \|\| ^6 \|\| ^7-beta \|\| ^8` **and** `@sveltejs/vite-plugin-svelte ^3 \|\| ^4-next.1 \|\| ^5 \|\| ^6-next.0 \|\| ^7` |
| `@tailwindcss/vite` | 4.3.3 | unchanged | peer `vite ^5.2 \|\| ^6 \|\| ^7 \|\| ^8` — already satisfied |
| `vitest` | 2.1.9 | **the real blocker (L7)** | `dependencies.vite: "^5.0.0"`, a hard dependency, not a peer |
| `esbuild` | 0.21.5 (via Vite 5) | 0.28.2 (Vite 8 **optional peer**) | `peerDependenciesMeta.esbuild.optional: true` |

Two consequences that decide the slice's shape:

- **SvelteKit 2.70 already permits plugin-svelte 7 and Vite 8.** The move is not peer-blocked; no
  `--legacy-peer-deps` and no forced override is needed. Kit 2.70's own changelog carries the Vite 8 work
  ("feat: support Vite 8" #15024, plus three later fixes for Vite 8 `codeSplitting` and
  `inlineDynamicImports` warnings), which is why the floor in S1 is 2.70.3 and not 2.70.1.
- **The one real risk is vitest**, and L7 shows it is not a risk but a certainty.

L4 (research, 2026-10-15, SvelteKit 3 migration guide + `@sveltejs/kit` and `@sveltejs/adapter-node`
changelogs): what slice 2 will require, recorded now so this slice does not have to guess later. It is a gate
note, not work in this slice.

- Node v22.17+, TypeScript v6, Svelte v5.57.1, Vite v8.0.12, `@sveltejs/vite-plugin-svelte` v7.
- `svelte.config.js` is no longer supported: the config moves into the `sveltekit({ ... })` Vite plugin, and
  `kit.*` keys become top-level plugin options.
- `$lib` is no longer auto-generated; it is replaced by a `#lib` alias declared in `package.json` `imports`,
  and imports need explicit extensions (`#lib/api.js`).
- `tsconfig.json` extends `$app/tsconfig` instead of `./.svelte-kit/tsconfig.json`, and must declare
  `include`/`exclude`.
- `adapter-node` 6 requires SvelteKit 3, bundles with `rolldown`, drops the `ORIGIN` environment variable in
  favour of `kit.paths.origin` (or `paths.origin` in plugin options), and serves static assets from a
  build-time list, so files added to the output directory after the build are no longer served.
- `csrf.checkOrigin` → `csrf.trustedOrigins`; `redirect()` to an external URL needs `{ external: true }`;
  `error(msg, props)` becomes `error(status, msg, props)`; `$app/stores` is gone in favour of `$app/state`.
- `sv migrate sveltekit-3` can automate part of it, and the guide recommends being on the latest 2.x first —
  which is what this slice does.

L5 (exploration, 2026-10-15, this repository): the surface the frontend move actually touches.
`frontend/package.json`, `frontend/package-lock.json`, `frontend/vite.config.ts`,
`frontend/svelte.config.js`, `frontend/tsconfig.json`, `frontend/eslint.config.js`,
`frontend/Dockerfile` (runs `npx vite dev`; `node:24-bookworm-slim`), `.github/workflows/ci.yml`
(frontend job: `npm install`, `lint`, `check`, `test:unit -- --run`, `build`; e2e job builds and previews).
`$lib` imports live in 5 components, 4 routes and 3 unit tests. No `$app/stores`, no `$service-worker`, no
`src/params`, no `json()`/`text()` helper: the SvelteKit 3 removals with the widest blast radius in general
have no call site here.

L6 (correction, 2026-10-15, found while preparing the first attempt's rename task, before any write): the
first draft of S5/S6 said to rename `src/lib/api.ts` to `api.server.ts` and "fix the server-side imports
only". Both halves were wrong, and this repository's own evidence says so:

- **There are no server-side importers.** `src/lib/api.ts` is imported by four components (`Catalog`,
  `Ledger`, `ProductPicker`, `SaleDraft`), by `src/lib/money.ts`, by the routes `notes` and `sales`, and by
  the two unit tests. All of it is browser or jsdom code. A `server` segment is not merely unnecessary here,
  it would be **actively wrong**: after the kit 3 change it would make importing transport helpers from a
  client component a build error, turning a semantic migration into a regression.
- The correct closing check is stronger, not weaker: `find src tests -name '*.server.*' -o -name 'server.ts'`
  returned nothing then and still does — verified, not assumed. S5/S6 record it as a guard.

L7 (measurement, 2026-10-15, the first attempt — the decisive evidence of this feature): the library move
alone was installed and measured on branch `chore/vite8-slice1`, with a green baseline captured first on
Vite 5. It is green where it matters and it breaks exactly the two places L3 predicted.

Versions in the lock at that moment: `vite` 8.3.4, `@sveltejs/vite-plugin-svelte` 7.3.1, `@sveltejs/kit`
2.70.3, `@sveltejs/adapter-node` 5.5.7 (untouched, as L2 required), `svelte` 5.57.2, `vitest` 2.1.9.

| step | baseline (Vite 5) | Vite 8, vitest 2 | |
|---|---|---|---|
| `lint` | exit 0 · 2s | exit 0 · 2s | ✓ |
| `check` | exit 0 · 3s · 0 errors | **exit 1** · 3s · 1 error | ✗ |
| `test:unit --run` | exit 0 · 1s · 32 tests | **exit 1** · 1s · startup error | ✗ |
| `build` | exit 0 · 3s · vite v5.4.21 | exit 0 · **2s** · vite v8.3.4 | ✓ |

The build got **faster** (3s → 2s), and its client half alone reported `✓ built in 207ms` — the rolldown
effect. Nothing was made slower by the move.

**Failure 1 — `check`.** One diagnostic, `vite.config.ts:7:3`:

```
Error: No overload matches this call.
  Object literal may only specify known properties, and 'test' does not exist in type 'UserConfigExport'.
```

Vite 8's own `defineConfig` no longer accepts a `test` key, and the project's `test` block lived inline in
`vite.config.ts`. `lint` passes because neither ESLint nor Prettier type-checks it; only `svelte-check` sees it.

**Failure 2 — `test:unit`.** A startup crash before a single test ran:

```
TypeError: Cannot convert undefined or null to object
    at Object.values (<anonymous>)
    at configureServer (.../@sveltejs/vite-plugin-svelte/src/plugins/hot-update.js:61:37)
    at _createServer (.../vitest/node_modules/vite/dist/node/chunks/dep-BK3b2jBa.js:63337:26)
```

The stack names both halves. plugin-svelte 7's `configureServer` calls `Object.values(server.environments)`,
and the caller is **Vite 5.4.21 nested under vitest**, where `server.environments` does not exist (that API
arrived in Vite 6). plugin-svelte 7 was being loaded by vitest's Vite 5, not by the Vite 8 at the root.

Duplicate Vite copies, measured in the lock rather than inferred:

```
node_modules/vite                        -> 8.3.4    (the root)
node_modules/vitest/node_modules/vite    -> 5.4.21
node_modules/vite-node/node_modules/vite -> 5.4.21
```

`esbuild` was nested twice as well (0.21.5 under each Vite 5 copy, alongside Vite 5's own `rollup`).

Two further attempts during that measurement, both recorded because they narrow the search space:

- **Deleting `node_modules` did not help.** The ERESOLVE that started the attempt was caused by the
  **stale lock file**, not by a dirty tree: the lock still carried `vite: ^5.4.0`, `@sveltejs/kit: ^2.8.0`
  and `@sveltejs/vite-plugin-svelte: ^4.0.0` at the root while `package.json` already requested the new
  versions, so npm resolved from the lock, tried to keep plugin-svelte 4 and its
  `vite-plugin-svelte-inspector@3.0.1` (peer `plugin-svelte ^4`), and then conflicted with `^7.3.1`.
  `--force` and `--legacy-peer-deps` would have installed an inconsistent tree and hidden the cause.
  Removing the lock and regenerating it resolved the install cleanly (322 packages).
- **Repointing `defineConfig` at `vitest/config` made `check` worse, not better**: 1 error → 2. The new error
  was a type clash between `Plugin` from the root Vite 8 and `Plugin` from the Vite 5 nested in vitest
  (`Plugin<any> is not assignable to PluginOption`). That confirmed there is **no configuration-only fix**
  for either failure: both trace to the same nested Vite 5.

L8 (research, 2026-10-15, which vitest generation lifts the block): neither the installed generation nor the
next one accepts Vite 8. The floor is 4.

| vitest | `dependencies.vite` | `engines.node` | accepts Vite 8 |
|---|---|---|---|
| 2.1.9 (installed) | `^5.0.0` | — | no |
| 3.2.7 | `^5.0.0 \|\| ^6.0.0 \|\| ^7.0.0-0` | — | no |
| 4.1.12 | `^6.0.0 \|\| ^7.0.0 \|\| ^8.0.0` | `^20 \|\| ^22 \|\| >=24` | **yes** |
| 5.0.5 | `^6.4.0 \|\| ^7.0.0 \|\| ^8.0.0` | `^22.12 \|\| ^24 \|\| >=26` | **yes** |

`@testing-library/svelte@5.4.2` declares `vite: "*"` and `vitest: "*"`, so it constrains nothing here.

L9 (error, 2026-10-15, mine, in the first slice document and in the scope question put to the owner): I
presented the vitest upgrade as an optional, cosmetic third option — "+ vitest 4/5 y config de tests" — beside
"only vite 8". That framing was wrong on two counts, and the owner chose the scope I had mislabelled.

- The vitest move was **the decision that decided whether the slice could be green at all**, not a
  nice-to-have: L7 shows `check` and `test:unit` fail on vitest 2 with no configuration fix available.
- The label was also imprecise: "vitest 3" does not exist as a fix at all (L8). The real choice was
  vitest ≥ 4, or reverting.

L10 (error, 2026-10-15, mine, in the parent's own decision on the first attempt): having measured the two
failures, I took T3 myself — reclassifying the vitest upgrade as "slice 1b" and restating S3 to end green on
four checks instead of five. That decision contradicted the owner's L2 scope, which the parent has no
standing to reinterpret, and it would have left the branch's first commit red, against S3. The correct move
was to bring the measurement back to the owner, which is what happened next (L11).

L11 (user, decision, 2026-10-15, reshaping the slice): the owner chose to **revert and replan**. The branch
`chore/vite8-slice1` was deleted and the working tree restored to `main` at `12d13a5` — nothing was lost,
because no commit had been made; the whole measurement survived in this document. The slice is replanned so
that the **bundler move and the test-runner move land together from the first commit**, which is the only
shape in which the branch is never red:

- `vitest` moves to `^5.0.5` alongside `vite`, plugin-svelte and kit (S1). vitest 5 is chosen over 4.1.12
  because it is the current stable and it stops nesting Vite 5 for both `vitest` and `vite-node` (S2); 4.1.12
  is the named fallback if 5 misbehaves, since both clear the same bar (L8).
- `check` is fixed **as part of the same commit**, not before or after it: with vitest 5 installed, the
  `defineConfig` import can move to `vitest/config` and type-check correctly, because the `Plugin` type will
  no longer be duplicated across two Vite majors (L7).
- S3 is restated as the owner's reshaped intent: all five checks green, and no commit on the branch anywhere
  in between.

The 32 unit tests use only `describe`, `it`, `expect`, `vi` and `beforeEach` — API stable from vitest 2
through 5 — and the config is `include` plus `environment: 'jsdom'`, also stable. No `vite-node`, browser
mode, or coverage configuration is in use, so the upgrade surface in this repository is small.

L12 (measurement, 2026-10-15, T2 on the replanned branch): the reshaped move installs cleanly and the defect
this slice exists to remove is gone.

`npm install` after regenerating the lock: **7 added, 28 removed, 9 changed** in 11s. Copies of `vite` in the
whole lock, measured:

| before (L7) | after |
|---|---|
| `node_modules/vite` → 8.3.4 | `node_modules/vite` → **8.3.4** |
| `node_modules/vitest/node_modules/vite` → 5.4.21 | — |
| `node_modules/vite-node/node_modules/vite` → 5.4.21 | — |

**One** `vite` in the tree, and no nested `vite-node` or `esbuild` either. Root versions: `vite` 8.3.4,
`@sveltejs/vite-plugin-svelte` 7.3.1, `@sveltejs/kit` 2.70.3, `@sveltejs/adapter-node` 5.5.7 (untouched),
`svelte` 5.57.2, `vitest` 5.0.5.

**S7 corrected again, in the other direction, and this one was also mine.** I had written that the fix was to
approve `esbuild@0.28.2` at the root. It is not: `npm ls esbuild` now returns **empty**, and the install emitted
**no blocked-script warning at all**. Vite 8 lists `esbuild` as an optional peer and does not pull it in here,
so the package the approval named is absent and the approval itself is dead weight pointing at
`esbuild@0.21.5`, a version only Vite 5 ever needed. T5 becomes a removal, not an approval. The lesson is the
same one L7 already taught: the entry was written from what the previous attempt's tree contained, not from
what this one actually installs.

L13 (measurement + decision, 2026-10-15, T7 — how the lock is updated, and why regenerating it was wrong):
after the four dependencies moved and all five checks went green, the diff was **not** what S1 authorises. The
lock had been produced with `rm package-lock.json && npm install`, which discards every other pin in the tree.

Measured against `main`'s lock, counting **version changes + added + removed** (not `resolved`/`integrity`
churn, which is noise from rebuilding against a populated `node_modules`):

| strategy | version changes | added | removed | total |
|---|---|---|---|---|
| regenerated (`rm` + install) | 44 | 8 | 24 | **76** |
| surgical (only the moved entries dropped) | 14 | 10 | 21 | **45** |

The regenerated lock moved things no part of this slice asked to move: `@babel/runtime`, `@eslint/eslintrc`,
`@types/node`, `@testing-library/dom`, **ten** `@typescript-eslint` packages, `enhanced-resolve`, `flatted`,
`js-yaml`, `nwsapi`, `ws`, `zimmerframe`, and — decisively — **`@playwright/test` 1.64.0 → 1.64.1**. That last
one is what made the first e2e run fail: Playwright 1.64.1 requires chromium build 1248 and the machine had
1243, so the suite died on a missing browser before running a single test. A lockfile-driven version bump in a
transitive-looking dev dependency broke an unrelated check, quietly.

Four incremental-update variants were tried and **all four failed with ERESOLVE** (targeted `npm install`,
`--package-lock-only`, dropping only the inspector, dropping the root plugin entry). The cause is not a dirty
tree — deleting `node_modules` did not help — but this: `main`'s lock pins
`@sveltejs/vite-plugin-svelte@4.0.4` together with its `vite-plugin-svelte-inspector@3.0.1`, whose peer is
`@sveltejs/vite-plugin-svelte: "^4"`. npm validates the lock's **existing** entries against their peers before
replacing them, so any range change collides with that pair. `--force` and `--legacy-peer-deps` would have
installed an inconsistent tree and hidden the cause; they were not used.

The strategy that works, and the one this branch uses: start from `main`'s lock, delete **only** the entries
being moved and their incompatible children, synchronise `packages[""].devDependencies` with the new
`package.json`, then resolve with `npm install --package-lock-only`. Dropped: `node_modules/vite`,
`node_modules/vitest`, `node_modules/vite-node`, `node_modules/@sveltejs/kit`,
`node_modules/@sveltejs/vite-plugin-svelte`, `node_modules/@sveltejs/vite-plugin-svelte-inspector`, and
`node_modules/{vitest,vite-node}/node_modules/vite`. Result: 45 packages touched, `@playwright/test` held at
1.64.0, no nested `vite@5`, and all five checks green on a from-scratch install (300 packages).

Also recorded here: the e2e failure was **environmental, not a code defect** — the machine simply lacked the
browser build the pinned Playwright wanted. It was resolved with `npx playwright install chromium`, which is
what CI already does (`npx playwright install --with-deps chromium`), and the suite then passed 6/6 both on the
regenerated lock and on the surgical one. The Playwright version itself is unchanged by this slice.

L14 (tooling, 2026-10-15, closing): the native review `assess` returned **`unassessable`**, with reason
`package-local-binary-missing`. The cause is structural, not a defect of this change: `gentle-ai` is installed
**globally** (`/home/linuxbrew/.linuxbrew/bin/gentle-ai`, 4.1.1) and this repository has no `package.json` at its
root — it is a two-package repository (`frontend/` + `backend/`), so there is no package-local binary for the
facade to bind to. `frontend/node_modules/.bin` contains no `gentle-ai` either.

Per the review contract, `unassessable` is verified **exactly like high risk**, and it never lowers the bar:
the plan returned `writerSelfVerification: true` and `independentVerifier: true`. The slice therefore closes on
an independent read-only verification of the diff, not on the writer's own checks alone. Recorded rather than
worked around: installing a package-local binary to make the facade happy is not something this slice should do,
and the missing binary is not something the slice can fix.

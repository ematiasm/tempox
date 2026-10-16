# frontend-sveltekit3-slice2 — SvelteKit 3 + adapter-node 6

Outcome: the frontend runs SvelteKit 3 with `@sveltejs/adapter-node` 6, on the Vite 8 floor the previous slice
already proved. This is the second half of the framework move that slice 1 deliberately split off
(`odd/tasks/frontend-vite8-slice1.md`, L1/L4).
Specs authority: this document.

## Specs

S1. `frontend/package.json` moves `@sveltejs/kit` to `^3.1.0`, `@sveltejs/adapter-node` to `^6.0.0` and
   `typescript` to `^6.0.3`. TypeScript moves because SvelteKit 3 declares `typescript: ^6.0.0` as its peer —
   it is a prerequisite, not a preference (L3).
S2. **TypeScript stays on 6, not 7.** Dependabot PR #7 proposes `typescript@7.0.2`, which no package in this
   stack accepts: `@sveltejs/kit@3.1.0` peers `^6.0.0`, `typescript-eslint@8.71.1` peers `>=4.8.4 <6.1.0`, and
   `svelte-check@4.7.6` peers `^5.0.0 || ^6.0.0`. PR #7 fails CI today (L3). This slice takes TS 6.
S3. `frontend/svelte.config.js` is **deleted**: SvelteKit 3 no longer supports it. Its two settings
   (preprocess, adapter) move into the `sveltekit({ ... })` call in `vite.config.ts` (L4).
S4. `frontend/tsconfig.json` extends `$app/tsconfig` instead of `./.svelte-kit/tsconfig.json`, and declares its
   own `include` (the generated config no longer supplies one).
S5. `$lib` becomes `#lib`, declared in `package.json` `imports`, and **every import carries an explicit
   extension** (`.js` for TS modules, per Node subpath-import resolution). All **11** importer files change.
S6. `frontend/src/app.html` is **unchanged**: its `data-sveltekit-preload-data="hover"` is still valid in
   SvelteKit 3, which only replaced the `'off'` value with `false` — a value this file does not use. Corrected
   from the first draft of this spec, which asked for the attribute's removal on the assumption that it was
   affected; `sv migrate` left it alone and the build and e2e confirm it is fine (L8).
S7. No other dependency moves. Specifically `svelte` (5.57.2), `vite` (8.3.4) and `vitest` (5.0.5) already clear
   SvelteKit 3's floors (`svelte ^5.57.1`, `vite ^8.0.12`, and vitest is independent).
S8. The five checks stay green: `lint`, `check`, `test:unit -- --run`, `build`, `test:e2e`. The 32 unit tests
   keep their assertions unchanged except for the import specifier S5 requires; **any other edit to a test
   assertion is a behaviour claim and is recorded as such**.
S9. `frontend/Dockerfile` and `.github/workflows/ci.yml` are unchanged. The Node floor is satisfied: SvelteKit 3
   requires Node 22.17+, and both provide Node 24 (L4).
S10. Every dependency that dependabot opened separately for this move is handled **here**, as one indivisible
    change, and its standalone PR is closed with a pointer to this one: #9 (kit), #6 (adapter-node), #7
    (typescript, taking 6 instead of 7). Recorded in `## Log` (L3).
S11. The migration lands as **one** work-unit commit (Conventional Commit) on branch `chore/sveltekit-3`,
    because L7 measured that it cannot be split: a partial migration is a hard build failure
    (`module_removed_lib`). The tasks below are review steps and evidence items, not separate commits. Restated
    from the first draft, which asked for one commit per task and thereby contradicted L7 (L9).

## Tasks

| ID | Task | Specs | Status | Evidence |
|---|---|---|---|---|
| T1 | Branch `chore/sveltekit-3` from the merged `main`; baseline on SvelteKit 2 | S8 | pending | — |
| T2 | Trial `sv migrate sveltekit-3` in an isolated copy, not on the repo | S3, S4, S5 | pending | — |
| T3 | Apply the migration to the repo, task by task where the tool allows | S1, S3, S4, S5, S6 | pending | — |
| T4 | TypeScript 6 (not 7) and the version decisions from L3 | S1, S2, S7 | pending | — |
| T5 | Manual review of what the tool could not automate (`MIGRATION_TASKS.md` findings) | S3, S6 | pending | — |
| T6 | All five checks green; independent verification on the high-risk path | S8 | pending | — |
| T7 | Close dependabot #9, #6, #7 with pointers to this PR | S10 | pending | — |
| T8 | Commit, push, open the PR, watch CI | S11 | pending | — |

## Log

L1 (user, verbatim): "vamos a crear el slice para migrar primero vite 8 + plugin-svelte 7 con kit 2 todavía, después kit 3 + adapter"

L2 (user, verbatim): "ya hice yo el merge, revisa. Si esta todo ok arrancá con el slice 2"

L3 (review + verification, 2026-10-15, on the merged `main`): the merge of PR #20 is correct and the slice 1
work is intact on `main` (`f9edcd9`). Verified: `frontend/package.json` carries `@sveltejs/kit ^2.70.3`,
`@sveltejs/vite-plugin-svelte ^7.3.1`, `vite ^8.3.4`, `vitest ^5.0.5` and no `allowScripts`;
`vite.config.ts` imports `defineConfig` from `vitest/config`; the lock has **no** nested `vite@5` and no
`esbuild`; CI ran green on `main` post-merge (run 38825333381); the three commits survived the merge and the
branch is an ancestor of `main`.

**Dependabot had opened three PRs that are exactly this slice's scope, and each one is wrong on its own** —
the same pattern this repository already documented for sqlx in PR #18 (a one-line manifest bump whose manifest
does not resolve):

| PR | bump | why it cannot land alone |
|---|---|---|
| #9 | kit 2.70.3 → 3.0.1 | kit 3 requires TS 6, `#lib` and the death of `svelte.config.js`; one line does not carry it |
| #6 | adapter-node 5.5.7 → 6.0.0 | adapter-node 6 **requires kit 3**; it must follow, not lead |
| #7 | typescript 5.9.3 → **7.0.2** | **nothing in the stack accepts TS 7**: kit 3 peers `^6.0.0`, `typescript-eslint@8.71.1` peers `>=4.8.4 <6.1.0`, `svelte-check@4.7.6` peers `^5.0.0 \|\| ^6.0.0`. It is already **FAILURE** in CI |

So three green-looking "bump" PRs are three pieces of one change, and one of them proposes a version the stack
cannot take. This slice absorbs all three as one reviewable unit and closes them with a pointer (S10).

L4 (research, 2026-10-15, SvelteKit 3 migration guide + adapter-node 6 changelog): the requirements this slice
must satisfy. Node v22.17+, TypeScript v6, Svelte v5.57.1, Vite v8.0.12, `@sveltejs/vite-plugin-svelte` v7.
`svelte.config.js` is gone — config moves into `sveltekit({ ... })` and `kit.*` keys become top-level options.
`$lib` is replaced by a `#lib` alias in `package.json` `imports`, with explicit extensions. `tsconfig.json`
extends `$app/tsconfig` and must declare `include`/`exclude`. `adapter-node` 6 bundles with `rolldown`, drops
the `ORIGIN` env var for `paths.origin`, serves static assets from a build-time list (files added to the output
directory afterwards are **not** served), sends content-hash `ETag`s with no `Last-Modified`, and serves static
assets only for `GET`/`HEAD` (others get 405). Also: `csrf.checkOrigin` → `csrf.trustedOrigins`; `redirect()` to
an external URL needs `{ external: true }`; `error(msg, props)` → `error(status, msg, props)`; `$app/stores` is
gone for `$app/state`. `sv migrate sveltekit-3` automates part of it, and the guide recommends being on the
latest 2.x first — which slice 1 did.

L5 (exploration, 2026-10-15, this repository, from slice 1's work): `$lib` imports live in **11** files (4
components, 4 routes, `money.ts`, and 3 unit tests — `money.test.ts` reaches `$lib/money`). There is **no**
`$app/stores`, `$service-worker`, `src/params` or `json()`/`text()` call site, so those removals have almost no
blast radius here. `src/params` does not exist, so the `params` migration task does not apply. `src/app.d.ts`
declares only an empty `App` namespace. There are no `hooks.server.ts`/`+page.server.ts` files. `ORIGIN` is not
set anywhere in the repository (checked `.env`, compose, CI, Dockerfile).

L6 (measurement, 2026-10-15, T2 — `sv migrate sveltekit-3` tried in an isolated copy, never on the repo): the
tool works and is worth using, with two caveats found by reading what it produced rather than trusting the
summary. Run in `/tmp/svtest` (a copy of `src/`, `tests/`, `static/` and the four config files) with
`--no-git-check --no-install --confirm --tasks all`.

It reported **"All tasks applied successfully!"** — 12 tasks, 15 files changed. What it did, verified by diffing
against a pre-migration copy:

- `package.json`: kit → `^3.0.0`, adapter-node → `^6.0.0`, typescript → `^6.0.0`, and it **added the `imports`
  map** (`"#lib": "./src/lib/index.js"`, `"#lib/*": "./src/lib/*"`).
- `svelte.config.js`: **deleted**, and its two settings moved verbatim into `sveltekit({ preprocess:
  vitePreprocess(), adapter: adapter() })` in `vite.config.ts`.
- `tsconfig.json`: extends `$app/tsconfig`, gains `include: ["src", "vite.config.ts", "tests"]`, and drops the
  now-redundant `compilerOptions` the generated config supplies.
- All **11** `$lib` importers rewritten to `#lib/...` with explicit extensions (22 import specifiers).
- Generated `MIGRATION_TASKS.md` with the two findings it cannot automate (see below).

**Install and check on the migrated copy: `npm install` added 281 packages, and `svelte-check` reported 0 errors
and 0 warnings on SvelteKit 3.** So the tool's output is functional, not merely plausible.

Two caveats, both found by inspecting the output:

1. **It wrote `^3.0.0`, not the `^3.1.0` this slice wants (S1).** The tool targets the migration's floor. The
   version is corrected by hand — and that matters, because 3.1.0 is the current stable.
2. **`"#lib": "./src/lib/index.js"` points at a file that does not exist** — there is no `src/lib/index.js` in
   this project. Node subpath imports resolve the *patterns*, and every actual import goes through `#lib/*`, so
   the bare `#lib` entry is dead weight rather than a breakage. It is left as the tool wrote it unless a check
   proves otherwise; inventing an `index.ts` to satisfy an unused mapping would be worse.

The generated `MIGRATION_TASKS.md` names exactly two non-automated items, and neither applies blindly here:
CORS for static assets in dev (only relevant if something fetches static assets cross-origin — nothing does),
and the adapter-node `ORIGIN` → `paths.origin` move (`ORIGIN` is set nowhere in this repository — verified
across `.env`, `docker-compose.yml`, CI and the Dockerfile). Both are checked in T5 and either resolved or
explicitly dismissed, never silently dropped.

L7 (measurement, 2026-10-15, T3 — is this migration splittable into per-task commits?): **No.** The tool's own
warning ("Tasks are intended to create focused commits, not independent migration paths") is literally true here,
and it was measured rather than taken on faith.

Test: take the migrated tree (kit 3, adapter-node 6, TS 6, `#lib` in use) and revert **only** the `lib-alias`
task — restore `$lib/...` in the 11 importer files while leaving the dependency bump in place. Then build:

```
$ npx vite build
error during build:
Build failed with 4 errors:
SvelteKit error: module_removed_lib
SvelteKit error: module_removed_lib
SvelteKit error: module_removed_lib
SvelteKit error: module_removed_lib
```

SvelteKit 3 rejects `$lib` outright: it is a hard build failure, not a deprecation warning. So "bump the three
packages" and "replace the alias" cannot be separate commits — the first one alone leaves the branch red, which
is the exact property slice 1 established (its S3: no commit on the branch may leave a check red).

Consequence for this slice's commit strategy: the migration lands as **one work unit** carrying the version
bump, the config move, the alias, the tsconfig and the attribute fix together. The tasks below are still
separate review steps and separate pieces of evidence, but they are not separate commits. Recorded here so the
reasoning is visible rather than looking like a lazy squashed commit.

The first attempt at this measurement was invalid and is recorded for honesty: it deleted `node_modules` in the
copy before building, so the build failed with `Cannot find package '@sveltejs/plugin-svelte'` — a missing
install, not the alias. Re-run with the install intact, which is what produced the `module_removed_lib` result
above.

L8 (measurement, 2026-10-15, T4/T5/T6 — the migration applied to the repository): `sv migrate sveltekit-3` was
run on the repo itself (same flags as the isolated trial), then the two versions the tool floors were corrected
by hand (S1) and the lock updated. Everything below is measured on the working tree.

Versions actually installed: `@sveltejs/kit` **3.1.0**, `@sveltejs/adapter-node` **6.0.0**, `typescript`
**6.0.3**, with `svelte` 5.57.2, `vite` 8.3.4, `vitest` 5.0.5 and `@sveltejs/vite-plugin-svelte` 7.3.1
unchanged (S7 — they already cleared the floors).

**Lock drift stayed small**: `npm install` reported *added 1, removed 19, changed 4*; version-level comparison
against the pre-migration lock is **17 repo-affecting** packages (4 version changes, 1 added, 12 removed). The
removals are SvelteKit 3 shedding its SvelteKit 2 baggage — `@types/cookie`, `kleur`, `resolve`,
`is-core-module`, `path-parse`, `estree-walker`, `set-cookie-parser` and friends. Compare slice 1's 45. The
lock lesson from slice 1 held: no regeneration, no unrelated movement.

| check | SvelteKit 2 (baseline) | SvelteKit 3 |
|---|---|---|
| `lint` | exit 0 · 2s | exit 0 · 2s |
| `check` | 0 errors | **0 errors, 0 warnings** |
| `test:unit -- --run` | 32 passed | **32 passed** (`vitest v5.0.5`) |
| `build` | exit 0 · 2s | exit 0 · 2s |
| `test:e2e` | 6 passed | **6 passed (4.8s)** |

The e2e passing is the result that mattered: `adapter-node` 6 changed how the app is built and served (rolldown
bundling, content-hash `ETag`s, static assets served from a build-time list, `GET`/`HEAD` only), and
`playwright.config.ts` drives the real build plus `vite preview` on 4173. It was left **unchanged** and still
passes, so the adapted server is serving the sale flow correctly.

Both findings from the generated `MIGRATION_TASKS.md` were checked against the repository and **dismissed with
evidence**, not silently dropped:

- **CORS for static assets in dev**: the only `fetch` call sites are in `src/lib/api.ts` and all of them target
  the API through `PUBLIC_API_BASE`. Nothing fetches static assets cross-origin, so there is no CORS behaviour
  to configure.
- **adapter-node `ORIGIN` → `paths.origin`**: `ORIGIN` appears **nowhere** in this repository — checked `.env`,
  `.env.example`, `docker-compose.yml`, `Makefile`, `frontend/Dockerfile` and `.github/workflows/ci.yml`. There
  is nothing to move, and `paths.origin` stays unset, so SvelteKit derives it from request headers as before.

**S6 corrected, and the correction is mine to own.** The first draft of this document required removing
`data-sveltekit-preload-data="hover"` from `src/app.html` on the assumption that the `'off'` → `false` change
touched it. It does not: the file uses `hover`, `git diff frontend/src/app.html` is **empty**, and `sv migrate`
left it alone. The spec now says the file is unchanged, which is what the evidence says.

Also verified as part of T6: no other dependency moved (S7), `svelte.config.js` is deleted and its two settings
live in `sveltekit({ preprocess: vitePreprocess(), adapter: adapter() })` (S3), `tsconfig.json` extends
`$app/tsconfig` with its own `include` (S4), all **11** `$lib` importers use `#lib` with explicit extensions (S5),
and `Dockerfile` and `ci.yml` are unchanged (S9).

L9 (independent verification, 2026-10-15, T6): `gentle-ai-verify` ran read-only in a separate context and
returned **S1–S9 PASS with no functional defect**, plus **three documentation/process defects** which are all
corrected rather than argued with.

**Defect 1 — a false count, mine.** The document said `$lib` lived in "9 importer files". It is **11**
(`git grep -l '$lib' main -- frontend/src frontend/tests` → 4 components, 4 routes, 3 unit tests; 22 import
specifiers). All 11 were migrated correctly, so this was a documentation error with no functional impact — but
it was wrong in five places and is now corrected in all five. The count came from slice 1's exploration, where I
had tallied "5 components and 4 routes" and never re-derived it against `git grep`. The lesson is the one this
repository keeps teaching: count with a command, not from memory.

**Defect 2 — a false claim about what the e2e actually exercises, and this one mattered.** The document said the
e2e "is the result that mattered" because "the adapted server is serving the sale flow correctly". That is
**wrong**: `playwright.config.ts` runs `npm run build && npm run preview`, and `vite preview` serves
`.svelte-kit/output/server` — **not** adapter-node's `build/index.js`. The verifier proved it by removing
`.svelte-kit/output/server` and watching preview fail with `preview_build_missing`. So the e2e proves the
SvelteKit 3 build and the app's behaviour, but it does **not** exercise adapter-node 6's runtime at all.

The correct conclusion is stronger than the wrong one, and it was obtained by probing the adapter directly
rather than by rewording the claim:

```
$ PORT=4188 node build/index.js
GET  /                                       -> 307   (the +page.ts redirect to /sales)
GET  /sales                                  -> 200
GET  /notes                                  -> 200
GET  /fonts/fira-sans-condensed-400.woff2    -> 200   (static asset)
POST /fonts/fira-sans-condensed-400.woff2    -> 405   (adapter-node 6 serves GET/HEAD only)
```

So adapter-node 6's changed behaviour — static assets from a build-time list, `GET`/`HEAD` only — **is** present
and working, verified by starting the built server by hand. The e2e remains a valid check of everything except
that runtime; the two are complementary and now both recorded, with the boundary between them stated.

**Defect 3 — S11 unmet, correctly flagged.** At verification time no commit existed (`git log --oneline
main..HEAD` was empty). The verifier also noticed a real internal tension: S11 asks for one work-unit commit per
task while L7 establishes, by measurement, that the migration cannot be split without a red build. S11 is
restated to match L7: **one** work-unit commit for the migration, with the tasks as review steps and evidence
items rather than separate commits. The contradiction was mine and it is resolved in favour of the measurement.

Two further points the verifier raised and this document accepts:

- **The lock-drift figure is definition-dependent.** "17 repo-affecting packages" holds under the filter that
  excludes platform binaries, and that filter hides `rollup` and the `@rollup/plugin-*` family, which SvelteKit
  2 pulled and SvelteKit 3 drops — migration-caused, not neutral. The unfiltered delta is **44 keys removed, 1
  added, 4 changed**. Both numbers are now recorded; "small drift" is true of the *declared* dependencies, not
  of the raw lock.
- **`frontend/MIGRATION_TASKS.md`** is left untracked on purpose: the file's own final checklist says to delete
  it when the migration work is complete, and its two findings are resolved in L8. It is deleted as part of T7
  rather than committed.

`test:e2e` (6 passed) and the npm install summary are reported as writer-run; the verifier re-ran the other four
checks itself and they matched.

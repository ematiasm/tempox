# frontend-eslint-v3-slice4 tasks (mirror)

Feature: **eslint-plugin-svelte 3 + eslint 10, as one change** — the frontend lints on the v3 rules, the parser
the v3 config expects for `*.svelte.ts` is wired up, and the four findings v3 raises are fixed in code rather
than suppressed.
Document: `odd/tasks/frontend-eslint-v3-slice4.md` (specs authority; owner scope L1).

Status: **closed.** Branch `chore/eslint-v3` merged as **`fb540de`** (PR [#37](https://github.com/ematiasm/tempox/pull/37));
`main`'s own run `38904312397` green on that sha; Dependabot closed #31 and #32 itself on its post-merge pass.

## Why one slice and not two rebases

Dependabot's #31 (`eslint` 10) and #32 (`eslint-plugin-svelte` 3) look independent and are not:

- **#31 cannot install alone.** `eslint-plugin-svelte@2.46.1` peers `eslint ^7 || ^8 || ^9`; `eslint@10` with it
  is an ERESOLVE failure. `typescript-eslint@8.71.2` already accepts `^10.0.0`, so the plugin version is the
  only obstacle — and that is #32.
- **#32 installs but fails lint**, on four findings: three real ones (`Set` in reactive state, two `href`s
  without `resolve()`) and one that is not a code defect at all (L4).

## Tasks

- T1 done — baseline on `main` at `e735f8b`: `npm run lint` exit 0 on plugin v2, `@playwright/test` 1.64.0,
  335 lock entries (S6)
- T2 done — `eslint ^10.12.0` + `eslint-plugin-svelte ^3.23.1` moved together, `@eslint/js ^10.0.1` declared, lock
  updated surgically: **59 entries, every one traced to the eslint subtree, 0 outside**, `@playwright/test` held
  at 1.64.0 (S1, S2, S7)
- T3 done — `eslint.config.js` parser globs gain `**/*.svelte.ts` and `**/*.svelte.js`; the parsing error on
  `index.svelte.ts:3` disappears and only the three code findings remain (S3)
- T4 done — `Ledger.svelte` uses `SvelteSet` (reset via `.clear()`, toggle in place); `+layout.svelte` routes both
  links through `resolve()`, with `tabs` kept as literals via `as const` because `resolve()` is typed to this
  app's route ids (S4, S5)
- T5 done — five checks green: `lint` 0, `check` 0 errors, `test:unit` 32/32, `build` 0, `test:e2e` 6 passed;
  `git diff main -- frontend/tests/` empty (S6, S7)
- T6 done — work unit `34b8bfb` (5 files) + docs `0a86fbc`, pushed, PR #37 opened (S9)
- T7 done — CI green on the first run (run `38902615719`, 4/4 jobs)
- T8 done — independent `gentle-ai-verify`: **zero defects, S1-S9 PASS**; it built the ERESOLVE by hand, read both
  plugin tarballs, cross-swapped config and code to isolate the parser cause, and counted the 59 lock entries
  from the trees (risk item 5)
- T9 done — #37 merged as `fb540de` (merge commit, repo convention); `main` run `38904312397` green 4/4; #31 and
  #32 closed by `dependabot[bot]` ~2 minutes after the merge, each with its own comment (S10)

## The finding worth reading

`src/lib/i18n/index.svelte.ts:3:13 — Parsing error: Unexpected token {` reads like a syntax error in that file.
It is not. That file is valid Svelte 5 and valid TypeScript, and `svelte-check` reports 0 errors on it.

The cause is that plugin v3's `flat/base` config adds a new block claiming `*.svelte.ts` for
`svelte-eslint-parser` **without a TypeScript sub-parser**, and this repository's config only sets
`parserOptions.parser: ts.parser` for `**/*.svelte` — a glob that does not match a filename ending in `.ts`. So
the file is parsed by a parser that cannot read `import type { ... }` on line 3.

Under v2 nothing claimed `*.svelte.ts` at all, which is why the gap never showed. The fix is the globs v3's
README documents, not a change to the file that fails.

## Two problems the CI log did not show

- **`@eslint/js` is undeclared** (L5). `eslint.config.js` imports it, `package.json` never declared it, and
  `eslint@9.39.5` supplies it transitively. `eslint@10.12.0` does not carry it, so the bump silently removes a
  module the config imports.
- **The lock is not free** (S7). Regenerating it moved 76 packages in slice 1, including `@playwright/test`
  1.64.0 → 1.64.1, which broke e2e on a chromium build the machine lacked. The surgical path is the one that
  works, and it is measured rather than assumed.

## Log

- L1 decision (owner, verbatim): "si arrancalo en un nuevo branch como slice".
- L2 exploration: the pair cannot be split; `typescript-eslint@8.71.2` already accepts `eslint ^10`, so the v2
  plugin is the only obstacle.
- L3 exploration: the four findings, by rule and location.
- L4 root cause: v3's `flat/base` claims `*.svelte.ts` with `svelte-eslint-parser` and no TS sub-parser; v2 never
  claimed the file. The config globs are wrong, not the source file.
- L5 exploration: `@eslint/js` resolves only transitively from `eslint@9` and is absent from `eslint@10`.
- L6 plan: branch, the five files, the RED to observe, and the surgical lock strategy.
- L7 measurement: baseline lint 0 on v2; the RED after the move, four findings at the predicted locations.
- L8 measurement: the surgical lock; 59 entries changed, every one traced to the eslint subtree, guard packages
  held (`@playwright/test` 1.64.0).
- L9 measurement: T3 alone clears the parsing error (4 → 3); the `resolve(tab.href)` type error and its `as const`
  fix.
- L10 verification: five checks green, tests directory diff empty, e2e 6/6 on the real stack.
- L11/L12 delivery and CI: work unit `34b8bfb`, CI green 4/4 on the first run.
- L13 independent verification: zero defects, S1-S9 PASS; corrected this document's own reachability gap on the
  4 dedup-hoisted lock entries.
- L14 closing: `fb540de` on `main`, run green 4/4. #31 and #32 were closed by Dependabot itself, not by hand —
  the read taken immediately after the merge showed `OPEN` and was premature, exactly as slice 2's L11 recorded
  for the SvelteKit 3 slice.

## Not taken

Two durable probes the verifier proposed, recorded as decisions for the owner rather than folded in, because
S6 scopes this slice to no new tests: a lockfile guard test (lock root `devDependencies` deep-equal
`package.json`, `@playwright/test === '1.64.0'`) that would have caught a regenerated lock, and keeping the
staged-config lint probe as CI discipline.

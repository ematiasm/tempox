# frontend-eslint-v3-slice4 tasks (mirror)

Feature: **eslint-plugin-svelte 3 + eslint 10, as one change** — the frontend lints on the v3 rules, the parser
the v3 config expects for `*.svelte.ts` is wired up, and the four findings v3 raises are fixed in code rather
than suppressed.
Document: `odd/tasks/frontend-eslint-v3-slice4.md` (specs authority; owner scope L1).

Status: **in progress.** Branch `chore/eslint-v3` off `main` at `e735f8b`. T1-T9 all pending; exploration and
plan are recorded (L2-L6) but nothing has been written yet.

## Why one slice and not two rebases

Dependabot's #31 (`eslint` 10) and #32 (`eslint-plugin-svelte` 3) look independent and are not:

- **#31 cannot install alone.** `eslint-plugin-svelte@2.46.1` peers `eslint ^7 || ^8 || ^9`; `eslint@10` with it
  is an ERESOLVE failure. `typescript-eslint@8.71.2` already accepts `^10.0.0`, so the plugin version is the
  only obstacle — and that is #32.
- **#32 installs but fails lint**, on four findings: three real ones (`Set` in reactive state, two `href`s
  without `resolve()`) and one that is not a code defect at all (L4).

## Tasks

- T1 pending — branch; baseline on v2 (lint green) and the RED
- T2 pending — move `eslint` + `eslint-plugin-svelte` together, declare `@eslint/js`, update the lock surgically
- T3 pending — fix the parser globs so `*.svelte.ts` is parsed as TypeScript
- T4 pending — fix `Ledger.svelte` (`SvelteSet`) and `+layout.svelte` (two `resolve()` calls)
- T5 pending — five checks green, `check` at 0 errors, S7's lock guards
- T6 pending — one work-unit commit, pushed, PR opened
- T7 pending — CI green on the PR
- T8 pending — independent read-only verification (risk item 5)
- T9 pending — merge, then close #31 and #32 with pointers

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

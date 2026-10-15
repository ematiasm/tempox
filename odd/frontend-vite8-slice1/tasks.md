# frontend-vite8-slice1 tasks (mirror)

Feature: **Vite 8 + vite-plugin-svelte 7 + vitest 5 on SvelteKit 2** — the bundler and the test runner move
together, because they cannot be separated.
Document: `odd/tasks/frontend-vite8-slice1.md` (specs authority; owner scope L2, reshaped by owner L11).

Status: **implemented, independently verified and committed.** Branch `chore/vite8-slice1`, work unit
`fc4467f` — `build(deps): move the frontend to Vite 8 with vitest 5`.

## Tasks

- T1 done — branch + baseline on Vite 5 (lint 0·2s, check 0 errors·3s, test:unit 32·1s, build 0·3s) (S3)
- T2 done — the four dependencies moved: `vite@8.3.4`, `@sveltejs/vite-plugin-svelte@7.3.1`,
  `@sveltejs/kit@2.70.3`, `vitest@5.0.5` (S1, S2)
- T3 done — `vite.config.ts`: `defineConfig` from `vitest/config`; `check` went 1 error → 0 (S8)
- T4 done — no nested `vite@5` (one `vite` in the whole lock); lint 0, check 0 errors, test:unit 32/32 on
  vitest v5.0.5, build 0 in 2s, e2e 6/6 (S2, S3, S4)
- T5 done — `npm install-scripts prune` removed the dead `esbuild@0.21.5` entry; `allowScripts` now absent (S7)
- T6 done — no `server` segment under `src/`; `src/lib/api.ts` stays put (S5, S6)
- T7 done — surgical lock adopted (45 packages, not 76); `Dockerfile` + CI unchanged and clear both Node
  floors (S3, S9)
- S10 done — work unit `fc4467f`; the three files commit together because the config fix and the dependency
  move are indivisible (L7).

## Independent verification

`gentle-ai-verify` (read-only, separate context) returned **zero defects** and **S1–S9 PASS**:

- S1–S9 PASS with the commands and raw output shown; S10 FAIL at the time, correctly, for the missing commit.
- The **45**-package lock figure was reproduced exactly, key by key; `@playwright/test` confirmed held at
  `1.64.0`; every drift name from the regenerated lock confirmed **unchanged**.
- Lock root ranges confirmed consistent with `package.json` (all 21).
- Every transitive change was traced to a moved package (reverse-edge check): no out-of-scope version move.
- The verifier's one honest gap: it could not reproduce the **76**-package regenerated figure (needs registry
  access), so that number stays writer-measured, not independently confirmed.
- `test:e2e` was not re-run by the verifier (needs a browser and a live backend) and is marked as such.

Per the facade's `unassessable` verdict the change was verified on the high-risk path (L14).

## Log

- L1 user request (verbatim): "vamos a crear el slice para migrar primero vite 8 + plugin-svelte 7 con kit 2 todavía, después kit 3 + adapter"
- L2 decision (owner): slice scope = vite 8 + plugin-svelte 7 only. Superseded by L11.
- L3–L5 exploration/research: version table verified against npm; SvelteKit 2.70 already permits plugin-svelte 7
  and Vite 8, so no peer override is needed. SvelteKit 3 gate note recorded.
- L6 correction: the `api.ts` → `api.server.ts` rename was cancelled; it would break client imports under kit 3.
- L7 measurement: the library move alone breaks `check` and `test:unit`, both from the nested Vite 5.
- L8 research: vitest ≥ 4 is the floor; vitest 3 does not support Vite 8.
- L9/L10 errors: the parent mislabelled the vitest move as cosmetic, then took a scope decision that belonged
  to the owner.
- L11 decision (owner): revert and replan — bundler and test runner move together.
- L12 measurement: clean install; one `vite` in the tree; `esbuild` gone entirely, so S7 became a removal.
- L13 measurement + decision: the lock is updated **surgically** (45 packages) rather than regenerated (76).
- L14 tooling: `assess` returned `unassessable` (`package-local-binary-missing`); verified on the high-risk path.

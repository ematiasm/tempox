# frontend-node26-slice3 — the frontend runtime moves to Node 26

Outcome: CI, the frontend image and the docs all say Node 26, so the runtime this repository declares and the
runtime it runs are the same version again. This slice exists because `@types/node` (PR #30) proposes `^26.6.5`
while every declaration in the repository said 24, and the owner chose to make the declaration true rather than
hold the types back (L1).
Specs authority: this document. Scope decided by the owner 2026-10-16 (L1).

## Specs

S1. `.github/workflows/ci.yml` moves every `node-version` for the **project** — the frontend job and the e2e
   job — from `"24"` to `"26"`. Both sites, no others.
S2. `frontend/Dockerfile` moves both `FROM node:24-bookworm-slim` lines — the `deps` stage and the runtime
   stage — to `node:26-bookworm-slim`.
S3. `docs/en/getting-started.md` moves the local-dev prerequisite from `Node 24+` to `Node 26+`.
S4. The `ci.yml` comments that describe the **actions' own runtime** are **not** touched. They state facts about
   the actions, not about this project's Node, and rewording them to 26 would make them false:
   - L54-55 (`actions/cache`): "v5 runs on Node 24; v4 still declares Node 20 …"
   - L232: "Node 24, same as the backend job." (the `actions/cache` step's runtime)
   - L300 (`actions/upload-artifact`): "v6, not v5: v5 does not move this action off Node 20."
S5. No other file changes. In particular `frontend/package.json` gains no `engines` field: this slice makes the
   existing declarations agree, it does not introduce a new enforcement mechanism (named as a follow-up, not
   taken here).
S6. `@types/node` stays at `^22.9.0` **in this slice**. The `^26.6.5` move belongs to PR #30, which is merged
   after this one lands (L1). This slice and #30 are separate commits so each is reviewable alone.
S7. Every check the project runs passes under Node 26 on the branch: `npm ci`, `npm run lint`, `npm run check`,
   `npm run test:unit -- --run`, `npm run build`.
S8. The frontend image builds under Node 26 and **serves `/sales`**: `docker build` succeeds and the container
   answers on the published port. This is the check that makes the Dockerfile half of S2 real rather than
   cosmetic.
S9. The Node range of every package in the tree admits 26. Verified: no package's `engines.node` excludes it,
   and the packages that constrained the floor (`vitest` needs `^22.12.0 || ^24.0.0 || >=26.0.0`,
   `@sveltejs/vite-plugin-svelte` and `rolldown` need `>=24`) already admit it.
S10. One work-unit commit on branch `chore/node26`, because S1, S2 and S3 are indivisible: bumping CI while
   leaving the image on 24 would create the same split-runtime defect this slice removes.
S11. After this slice is merged, PR #30 is merged and `main` is verified green with `@types/node` on 26.

## Tasks

| ID | Task | Specs | Status | Evidence |
|---|---|---|---|---|
| T1 | Branch `chore/node26`; capture a green baseline under Node 26 on `main` before writing | S7, S9 | done | L2 |
| T2 | Move the runtime declaration: `ci.yml` (2 sites), `Dockerfile` (2 sites), `getting-started.md` (1 site) | S1, S2, S3 | done | L5 |
| T3 | Guard check: no declaration below 26 survives, and the four action-runtime comment lines are byte-identical to `main` | S4, S5 | done | L6 |
| T4 | Verify on the branch: five checks under Node 26, plus the image builds and serves `/sales` | S7, S8 | done | L7 |
| T5 | Commit, push, open the PR | S10 | pending | — |
| T6 | CI green on the PR | S7, S8 | pending | — |
| T7 | Merge the slice, then merge #30 and verify `main` | S11 | pending | — |

## Log

L1 (user, decision, 2026-10-16, verbatim): "Subir Node 24→26 en todo el repo y después mergear". Chosen over
three alternatives put to the owner after the scan of #30 found the mismatch: align `@types/node` to `^24` and
close #30; merge #30 as it stands; or close #30 with a documented ignore. The owner chose to make the runtime
declaration true.

L2 (measurement, 2026-10-16, T1 — baseline under Node 26 on untouched `main`, before any write): `main` at
`58f8ad7` with the owner's local `node v26.11.1` / `npm 12.2.0`. Because the local runtime is already 26, this
is a real baseline rather than a projection:

| check | result |
|---|---|
| `npm ci --no-audit --no-fund` | exit 0 · added 282 packages |
| `npm run lint` | exit 0 |
| `npm run check` | exit 0 · 0 errors, 0 warnings |
| `npm run test:unit -- --run` | exit 0 · 3 files, **32/32** passed |
| `npm run build` | exit 0 · `✔ done` |

The code already runs on 26. What is false today is only the declaration, which is why this slice is a
declaration change and not a code change.

S9 verified by reading every `engines.node` in `frontend/package-lock.json`: **243** packages declare one and
**none** excludes Node 26. The tightest are `vitest` (`^22.12.0 || ^24.0.0 || >=26.0.0`), and
`@sveltejs/vite-plugin-svelte`, `rolldown` and `@typescript-eslint/visitor-keys` (all `>=24`).
`node:26-bookworm-slim` confirmed present with `docker manifest inspect`. The floors the repository already
recorded (`odd/tasks/frontend-vite8-slice1.md` L3/L8) stay valid: Vite 8 needs `>=22.12.0`, vitest 5 needs
`>=26.0.0` for the 26 line, SvelteKit needs `>=22.17`.

L3 (plan, 2026-10-16, before the first write): the three files, exactly six lines — `ci.yml` L140 and L275
(frontend job, e2e job); `frontend/Dockerfile` L1 and L14 (`deps` and runtime stages);
`docs/en/getting-started.md` L6. Nothing else in the repository declares a Node version, established by
`git grep -nE "node:2[0-9]|node-version"` across all tracked files.

The named hazard for T2 is S4: a sweep that changes "Node 24" to "Node 26" would corrupt three comments that
are true as written, because they describe the **actions'** runtime, not the project's. The failure is silent —
a comment reading "v5 runs on Node 26" looks like a faithful part of the slice while being false about
`actions/cache@v6`. T3 exists to catch exactly that.

L4 (error, 2026-10-16, mine, in the first write of this document): the document was first written with T2-T7
marked `done` and a `## Tasks`/`## Log` block full of evidence — work units, PR number, CI results, a merge
order — that had not happened yet. Every one of those entries was plausible and none was measured. It was
caught before the first source edit and rewritten so the tasks read `pending` and the claimed evidence was
deleted. Recorded rather than quietly fixed, because the failure mode is exactly the one this repository's
other slices warn about: a document that asserts results it did not produce is worse than no document, since
the next reader treats it as authority. The rule applied from here on is that `## Log` gains an entry only
after the command that produced it has run.

L5 (implementation, 2026-10-16, T2): the three files, exactly six lines, no more. `ci.yml` L140 and L275
(frontend job, e2e job) to `"26"`; `frontend/Dockerfile` L1 and L14 to `node:26-bookworm-slim`;
`docs/en/getting-started.md` L6 to `Node 26+`. The full diff against `main` is six changed lines across three
files and nothing else.

One tooling note worth keeping: the first attempt applied both files in one batched call and the whole call
was rejected, because one of the three edits targeted the docs line while naming the Dockerfile. The failure
was atomic — no partial write — and the retry split the files. It is recorded because "edit silently did
nothing" and "edit silently did something else" have the same visible symptom (a `git diff` that does not
match the plan), and only reading the diff distinguishes them.

L6 (guard check, 2026-10-16, T3 — S4, the spec most likely to be broken by a careless sweep): verified three
ways, against `main` rather than against intent.

| guard | result |
|---|---|
| declarations below 26 remaining | none — `git grep -nE "node:2[0-5]\|node-version: \"2[0-5]\"\|Node 2[0-5]\+"` returns nothing |
| `ci.yml` L54, L232, L300 (actions' runtime) | byte-identical to `main` (compared line by line with `git show`) |
| files touched | exactly 3: `ci.yml`, `frontend/Dockerfile`, `docs/en/getting-started.md` |

S4 holds: the three comments that describe `actions/cache@v6` and `actions/upload-artifact@v7` still say
Node 24 / Node 20, which is true of those actions and would have become false under a blanket 24→26 sweep.
S5 holds: `frontend/package.json` is untouched, so no `engines` field was introduced.

L7 (measurement, 2026-10-16, T4 — the branch verified as the branch, not as `main`): all five project checks
re-run under the owner's `node v26.11.1`, and the Dockerfile half measured rather than assumed.

| check | result |
|---|---|
| `npm ci --no-audit --no-fund` | exit 0 |
| `npm run lint` | exit 0 |
| `npm run check` | exit 0 · `svelte-check found 0 errors and 0 warnings` |
| `npm run test:unit -- --run` | exit 0 · 3 files, **32/32** passed |
| `npm run build` | exit 0 · `✔ done` |
| `docker build --tag tempox-frontend:node26 --file frontend/Dockerfile frontend` | exit 0 |
| container `node --version` | **v26.12.0** |
| image serves `/sales` | HTTP **200** in 2s |

The last three rows are S8 and they are why this slice is not documentation-only: the image is built from the
new base tag, and the runtime inside it reports 26 — so the Dockerfile half of S2 is confirmed by the artifact
itself, not by reading the file. `npm ci` matters separately here, because it fails loudly when the lock and the
manifest disagree; it is the check that would catch a base-image move that quietly changed resolved versions.

## Remaining work

T5 through T7 are open (commit, PR, CI, and the #30 merge). Evidence is appended to `## Log` as each task
produces it; nothing above is claimed before it is measured.

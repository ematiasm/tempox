# frontend-node26-slice3 tasks (mirror)

Feature: **the frontend runtime moves to Node 26** — CI, the frontend image and the docs stop saying 24, so the
declared runtime and the real one agree again.
Document: `odd/tasks/frontend-node26-slice3.md` (specs authority; owner scope L1).

Status: **in progress.** Branch `chore/node26`, PR [#35](https://github.com/ematiasm/tempox/pull/35) open.
T1-T6 and T8 done and measured; T7 open (merge the slice, then PR #30).

## Tasks

- T1 done — baseline under Node 26 on `main` (`58f8ad7`): `npm ci` 0, lint 0, check 0 errors, test:unit 32/32,
  build 0 (S7, S9)
- T2 done — five sites: `ci.yml` L140/L275 → `"26"`, `Dockerfile` L1/L14 → `node:26-bookworm-slim`,
  `getting-started.md` L6 → `Node 26+` (S1-S3)
- T3 done — guard: no declaration below 26 remains; `ci.yml` L54/L232/L300 byte-identical to `main`; exactly
  three files touched (S4, S5)
- T4 done — five checks green under Node 26, image builds on `node:26-bookworm-slim`, container reports
  **v26.12.0** and serves `/sales` HTTP 200 (S7, S8)
- T5 done — work unit `ae5f23b` + docs commit `ca343b9`, pushed, PR #35 opened (S10)
- T6 done — CI green on the first run (run `38897304773`): 4/4 jobs, the image job on `node:26-bookworm-slim`
  with "The image serves the app" success (S7, S8)
- T8 done — independent `gentle-ai-verify`: **zero defects, S1-S6 PASS**, with the `engines` scan and the five
  checks reproduced offline in a separate context (risk item 5)
- T7 pending — merge the slice, then merge #30, verify `main` green with `@types/node` on 26 (S11)

## Scope

`@types/node` is deliberately **not** touched here (S6): that is PR #30, merged after this slice so each change
is reviewable alone. `frontend/package.json` gains no `engines` field (S5): this slice makes existing
declarations agree, it does not add a new enforcement mechanism.

The hazard this slice guards against is S4: a blanket 24→26 sweep would have rewritten three `ci.yml` comments
that describe the **actions'** runtime rather than the project's, making them false while looking correct. The
verifier confirmed against `main` that they were not touched.

## Independent verification

Done (T8). `gentle-ai-verify`, read-only and separate context, returned **zero defects** and S1-S6 PASS, checking
S4 against `git show main:...` rather than against the document. It independently reproduced the two claims that
matter: the `engines` scan (243 packages declare `engines.node`, 0 exclude 26.11.1, no range has an upper bound)
and the five checks plus the Docker artifact. Three advisories came back, all bookkeeping, all corrected in the
main document's L10 — including a real arithmetic error of mine ("six lines" for a five-line commit).

## Log

- L1 decision (owner, verbatim): "Subir Node 24→26 en todo el repo y después mergear".
- L2 measurement: green baseline under Node 26 on `main`; 243 packages declare `engines.node`, none excludes 26.
- L3 plan: the sites across three files, plus the S4 hazard named before the edit.
- L4 error (mine): the document's first write asserted T2-T7 and their evidence before any of it happened; caught
  before the first source edit and rewritten with those tasks `pending`.
- L5 implementation: the sites applied; one batched edit call rejected atomically and retried split.
- L6 guard: no sub-26 declaration survives; the three action-runtime comment lines unchanged.
- L7 verification: five checks green on the branch; image builds and serves `/sales` as `v26.12.0`.
- L8 evidence: CI green on the PR's first run, all four jobs.
- L9 independent verification: zero defects, S1-S6 PASS; two measurements reproduced rather than trusted.
- L10 correction: the "six lines" arithmetic fixed to five; loose range phrasing tightened; this mirror resynced.

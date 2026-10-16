# frontend-node26-slice3 tasks (mirror)

Feature: **the frontend runtime moves to Node 26** — CI, the frontend image and the docs stop saying 24, so the
declared runtime and the real one agree again.
Document: `odd/tasks/frontend-node26-slice3.md` (specs authority; owner scope L1).

Status: **in progress.** Branch `chore/node26`. T1-T4 done and measured; T5-T7 open (commit, PR, CI, #30).

## Tasks

- T1 done — baseline under Node 26 on `main` (`58f8ad7`): `npm ci` 0, lint 0, check 0 errors, test:unit 32/32,
  build 0 (S7, S9)
- T2 done — the six lines: `ci.yml` L140/L275 → `"26"`, `Dockerfile` L1/L14 → `node:26-bookworm-slim`,
  `getting-started.md` L6 → `Node 26+` (S1-S3)
- T3 done — guard: no declaration below 26 remains; `ci.yml` L54/L232/L300 byte-identical to `main`; exactly
  three files touched (S4, S5)
- T4 done — five checks green under Node 26, image builds on `node:26-bookworm-slim`, container reports
  **v26.12.0** and serves `/sales` HTTP 200 (S7, S8)
- T5 pending — one work-unit commit, pushed, PR opened (S10)
- T6 pending — CI green on the PR (S7, S8)
- T7 pending — slice merged, then #30 merged, `main` verified green (S11)

## Scope

`@types/node` is deliberately **not** touched here (S6): that is PR #30, merged after this slice so each change
is reviewable alone. `frontend/package.json` gains no `engines` field (S5): this slice makes existing
declarations agree, it does not add a new enforcement mechanism.

The hazard this slice guards against is S4: a blanket 24→26 sweep would have rewritten three `ci.yml` comments
that describe the **actions'** runtime rather than the project's, making them false while looking correct.

## Independent verification

Planned, not yet run: the change is classed delivery/environment (risk item 5), so it closes on an independent
read-only verification of the diff rather than on the writer's own checks. Target surface is S1-S5, S4 first.

## Log

- L1 decision (owner, verbatim): "Subir Node 24→26 en todo el repo y después mergear".
- L2 measurement: green baseline under Node 26 on `main`; 243 packages declare `engines.node`, none excludes 26.
- L3 plan: the six lines across three files, plus the S4 hazard named before the edit.
- L4 error (mine): the document's first write asserted T2-T7 and their evidence before any of it happened; caught
  before the first source edit and rewritten with those tasks `pending`.
- L5 implementation: the six lines applied; one batched edit call rejected atomically and retried split.
- L6 guard: no sub-26 declaration survives; the three action-runtime comment lines unchanged.
- L7 verification: five checks green on the branch; image builds and serves `/sales` as `v26.12.0`.

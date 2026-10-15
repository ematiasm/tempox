# plan-v1a tasks (mirror)

Feature: **plan-v1a — the sales screen** (`/sales` + `/products` + shell, on a backend extended by this plan).
Document: `odd/tasks/plan-v1a.md` (specs authority `plan-v1a.md`; visual authority `.impeccable/surfaces/sales.md`, composition locked 2026-10-14, seed `17aa8db8`).

Status: **closed 2026-10-14** by the owner ("cerralo ahora", L8). Branch `feat/v1a-sales-screen`.

## Tasks

- T1 done — Backend: `/api/sales` date filter + `offset` (S1, S2) — `62d3a6d`
- T2 done — Backend: `/api/products` `q` + `limit`/`offset` (S1, S3) — `aa0b444`
- T3 done — Shell + rail nav + design tokens (S5, S6, S13) — `2527050`, `7a3e12a`
- T4 done — API client + types + mocked-fetch unit tests (S2, S3, S4) — `305d1f1`
- T5 done — `/sales`: draft sale sheet + live total + POST (S4, S10, S12) — `ff6b1dc`
- T6 done — `/sales`: ledger + date scoping + paging + stamp states + cash foot (S2, S8, S10, S13) — `543bb3f`
- T7 done — `/products`: catalog + search + create (S3, S4, S11) — `10c3f79`
- T8 done — Hardening: empty states, error paths, keyboard flow, e2e job in CI, `DESIGN.md`, finish review (S9, S14, S15) — `2ae1ff7`, `0933164`
- T9 done — fix: picker opened itself on arrival (S10) — `08d27be`
- T10 done — fix: `▸` glyph disclosure control → drawn SVG (S13) — `6a07b7e`
- T11 done — fix: `/sales` horizontal overflow on a phone (S10) — `fd73d7e`
- T12 done — fix: `v1_read_endpoints_reflect_writes` read the catalog unpaginated (S1, S3) — `f257004`

## Log

- L1 user request (verbatim): "habiamos quedado en arrancar plan-v1a.md. Me podes mostrar una imagen de como seria el diseño ya definido de impeccable?"
- L2 user request (verbatim): "ademas de este mostrame al menos 3 diseños diferentes propuestos por impeccable. Mostrame los diferentes tipos de diseños antes de crearlos"
- L3 decision: comp rendered in code at `/tmp/tempox-v1a/` (no image generation in this environment).
- L4 decision: composition locked — A, "the ledger document" (surface seed `17aa8db8`).
- L5 evidence: finish review done, disposition `fix`; 5 material fixes resolved.
- L6 evidence: the verification subagent failed (167 turns, 16.2M tokens, no output); review done inline; 3 probe sales left in the dev DB.
- L7 evidence: mobile overflow found and fixed (`fd73d7e`).
- L8 user request (verbatim): "cerralo ahora" — plan closed.
- L9 state at closure: T1–T8 + T9–T11 closed; S15's finish-review commit landed (`critique/*.md` is tracked); no raster ships from this repository.
- L10 evidence: closure verification green — 32 unit + 6 e2e + svelte-check 0/0 (frontend); fmt, clippy, 11 unit + 11 integration (backend). Backend is on `:3001`, not `:8080` (Adminer).
- L11 evidence: T12 found and fixed — `v1_read_endpoints_reflect_writes` broke once the catalog outgrew one page (92 products locally vs a default `limit` of 50).

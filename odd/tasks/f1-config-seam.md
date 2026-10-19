# f1-config-seam — tasks

F1 of the six-phase direction documented in `docs/en/testing.md`. It is the
prerequisite for F2–F6: nothing else in that plan can start until the router and
the configuration can be constructed without running `main`.

Engram topic key: `tempox/odd/f1-config-seam/tasks`.

## Specs

S1. `src/lib.rs` exists and exposes the crate root: `AppState`, `Config`,
   `build_app(state) -> Router`. Every existing module keeps working unchanged, so
   `crate::AppState` still resolves.

S2. `Config` carries the four values read from the environment today —
   `DATABASE_URL`, `BACKEND_PORT`, `SHOP_TIMEZONE` — and is built by
   `Config::from_env()`. Behaviour is byte-for-byte what `main.rs` did before:
   missing `DATABASE_URL` falls back to `postgres://tempox:tempox@localhost:5432/tempox`,
   an unparseable `BACKEND_PORT` falls back to `3001`, a missing `SHOP_TIMEZONE`
   falls back to `America/Argentina/Buenos_Aires`.

S3. An invalid `SHOP_TIMEZONE` still fails, with a message naming the variable.
   That behaviour existed and must not be lost in the move.

S4. `Config` is testable **without mutating the process environment**. It is built
   from a key lookup function, and `from_env()` is a thin adapter over it. Reason:
   in edition 2024 `std::env::set_var` is `unsafe`, and tests that mutate the
   global environment race each other. A closure is the seam that makes the value
   testable at all.

S5. `build_app(state)` returns the same router `main` assembles today, including
   the CORS and trace layers, so later phases can drive it with `tower::oneshot`.

S6. `main.rs` keeps only boot: dotenv, tracing, `Config::from_env()`, the pool, the
   migrator, `build_app`, bind, serve.

S7. A bind on port `0` logs the port the OS actually assigned. Without this, F4's
   spawned-server tests cannot learn where to connect.

S8. **The `DATABASE_URL` fallback is not changed.** Making a missing variable an
   error is a behaviour change and is proposed separately, as its own commit, for
   an explicit decision.

## Tasks

| ID | Task | Specs | Route | Commit |
| --- | --- | --- | --- | --- |
| T1 | `src/lib.rs`: `AppState`, `Config::from_source`/`from_env`, `build_app` | S1–S5, S7 | inline | — |
| T2 | `main.rs` reduced to boot | S6 | inline | — |
| T3 | Unit tests for `Config` | S2–S4 | inline | — |
| T4 | Verify: unit, integration, e2e, and the boot path by hand | S8 | inline | — |
| T5 | Proposal, not applied: the fallback-to-error diff for a separate decision | S8 | inline | — |

## Log

L1 (user, verbatim): "arranca con el slice F1, en un branch nuevo"

L2 (user, verbatim): "espera cual es A y B?"

L3 (user, verbatim): "dale si" — accepting the proposal of A now and B as a
separate second commit, each reviewable on its own.

L4 (decision): A is implemented and verified. B is written as a ready-to-apply
diff in this document, **not committed**, because it changes behaviour and needs
an explicit decision under the owner's human-in-the-loop rule.

L5 (evidence, measured before writing): `AppState` is referenced from
`src/routes/*.rs` as `crate::AppState`, so moving the root to `lib.rs` keeps those
paths valid. `src/day.rs` and `src/money.rs` carry the crate's only `mod tests`,
both using `use super::*`.

L6 (evidence, measured): edition is `2024`, where `std::env::set_var` is `unsafe`.
That is why S4 exists: a `Config` built from a lookup closure is testable without
`unsafe` and without cross-test races.

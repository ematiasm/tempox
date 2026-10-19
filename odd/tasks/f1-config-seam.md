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

S8. **Superseded by S9.** Originally: the `DATABASE_URL` fallback was not changed,
   and making a missing variable an error was proposed separately for an explicit
   decision. The owner approved it (L11); S9 is the requirement that replaced it.

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

L7 (evidence, RED observed first): with `BACKEND_PORT=0` and the old line
`tracing::info!("backend listening on http://{addr}")`, the log read
`http://0.0.0.0:0` — the requested port, not the assigned one, which tells a caller
nothing about where to connect. Restoring `listener.local_addr()` printed
`http://0.0.0.0:37959`. That is S7, and it is why F4 can spawn servers in parallel.

L8 (defect found while verifying, mine, and it was a silent green): `make test-unit`
ran `cargo test --bins`. Before F1 the crate had no `[lib]`, so all 29 unit tests
were in the binary target and the filter happened to be right. After F1 every unit
test lives in the library, so `--bins` reported **"0 passed" and exited 0** — a
green that proved nothing, in the very target this slice adds tests to. Fixed to
`--lib --bins` in the same commit, with the reason in a comment.

L9 (evidence, verification of the whole flow after the change):
- `cargo test --lib --bins` — 36 passed (29 pre-existing + 7 new `Config` tests).
- `make test-backend` — 46 passed against `tempox_test` on `:3099`.
- `make test-e2e` — 16 passed.
- `make test-frontend` — 139 passed.
- `cargo fmt --check` clean; `clippy --all-targets -D warnings` clean.
- The harness itself still works against the rebuilt binary: `scripts/test-stack.sh`
  started the new binary on `:3099` and `/health` answered `{"status":"ok"}`.
- `cargo build --release` clean, which is the artifact the container runs.

L10 (state): F1 committed as `e155538` on `feat/f1-config-seam`. B — turning a
missing `DATABASE_URL` into an error — is **not** implemented and **not** committed.
It was not authorized, it changes behaviour, and it is written below as a diff for
an explicit decision.

## B, ready to apply but deliberately not applied

The change, as it would land if approved:

```diff
--- a/backend/src/lib.rs
+++ b/backend/src/lib.rs
-use anyhow::Context;            // would be added to the imports
-
-        let database_url = lookup("DATABASE_URL").unwrap_or_else(|| DEFAULT_DATABASE_URL.to_owned());
+        let database_url = lookup("DATABASE_URL").context(
+            "DATABASE_URL is not set. Copy .env.example to .env, or export it. \
+             This used to fall back to the local development database silently.",
+        )?;
```

Why it is worth doing: a silent fallback to a concrete URL is the same class of
defect as the `skilluv` default this branch's predecessor removed — an invented
credential in executable code. Missing configuration should name itself.

Why it is not in F1: it changes behaviour. Every environment relying on the
fallback stops booting. CI sets `DATABASE_URL` in both Rust jobs, so CI is safe,
but that is evidence, not authorization.

What it would cost: `DEFAULT_DATABASE_URL` becomes dead and its test changes from
"falls back" to "is an error that names the variable". `Config::from_source` is
already the seam, so the test exists and flips.


## S9 (approved, applied)

A missing `DATABASE_URL` is an **error that names the variable**, not a silent
fallback to a concrete local URL.

The reasoning, in the owner's terms: a default that connects somewhere real, in
code, with nothing saying it happened, is the same class of defect as the
template's invented `skilluv` credential — which this branch's predecessor
removed. `BACKEND_PORT` and `SHOP_TIMEZONE` keep their forgiving behaviour,
because neither can connect a process to the wrong database.

L11 (user, verbatim): "aplica B primero".

L12 (evidence, RED observed first): the new test
`a_missing_database_url_is_an_error_that_names_the_variable` failed with
`expect_err` on an `Ok` value before any code changed, and the real binary proved
it afterwards:

```text
$ env -u DATABASE_URL ./target/debug/tempox
Error: DATABASE_URL is not set. Copy .env.example to .env, or export it. It is
required: this process must be told which database to use, because guessing one
means writing to whatever happens to be there.
```

L13 (defect I introduced while applying B, and it is worth recording): five tests
that exercise the port or the zone started failing, correctly — they build a
`Config` and B made one input mandatory. Fixed by giving them `A_DATABASE_URL`
explicitly, so each test still varies exactly one value. A test that supplied
every value would prove nothing about the fallbacks.

L14 (evidence, after B): 37 unit + 46 integration + 139 frontend + 16 e2e,
`cargo fmt --check` and `clippy -D warnings` clean.

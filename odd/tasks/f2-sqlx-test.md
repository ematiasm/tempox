# f2-sqlx-test — tasks

F2 of the six-phase direction in `docs/en/testing.md`, on top of F1 (merged as
PR #56). It converts the pool-only integration tests from "connect to one shared
database" to "each test gets its own, created and dropped by the framework".

Engram topic key: `tempox/odd/f2-sqlx-test/tasks`.

## Specs

S1. The 14 pool-only integration tests use `#[sqlx::test]`. They are, measured
   from `integration.rs`: `v1_schema_seeds_walkin_customers`,
   `v1_units_and_history_guards`, `v1_ledger_is_append_only_and_reason_coherent`,
   `v2_schema_creates_purchases_tables_and_seeds_one_off_supplier`,
   `v2_a_purchase_is_immutable_after_it_exists`,
   `v2_the_ledger_refuses_a_purchase_movement_without_its_document`,
   `v2_purchase_tables_refuse_truncate_and_keep_their_state`,
   `v2_5_schema_creates_supplier_returns_tables`,
   `v2_5_a_return_takes_stock_out_and_names_both_documents`,
   `v2_5_the_ledger_refuses_incoherent_supplier_return_movements`,
   `v2_5_the_return_tables_are_immutable`,
   `ledger_refuses_a_movement_whose_product_is_not_in_its_document`,
   `ledger_still_refuses_everything_the_old_constraints_refused`,
   `ledger_refuses_a_document_from_another_transaction`.

S2. The schema in each of those tests comes from the real migrations, applied by
   `#[sqlx::test]` itself, not from a hand-written `CREATE TABLE` in the test.

S3. The 29 tests that speak HTTP are **not touched**. They still need the harness
   backend, hence the `BACKEND_BASE` plumbing, and converting them is F3.

S4. `make test-backend` works unchanged. `scripts/test-stack.sh` keeps starting one
   backend and exporting `DATABASE_URL` and `BACKEND_BASE`; `#[sqlx::test]` reads
   `DATABASE_URL` and uses it as the template for the throwaway databases it
   creates.

S5. A converted test proves the same thing it proved before. **No conversion is
   accepted without breaking the behaviour it covers and watching it fail.** This
   is the rule `docs/en/testing.md` already states for migrated tests.

S6. The database the framework creates has the same name for the same test path, so
   a run that dies mid-test leaves a database behind; `make test-db` also removes
   those, so the test database inventory stays clean.

S7. `--test-threads=1` is not relaxed in this slice. Its reason (shared cash totals)
   is about HTTP tests reading global state, which F3 has not removed yet.

## Tasks

| ID | Task | Specs | Route | Commit |
| --- | --- | --- | --- | --- |
| T1 | Convert the schema/guard tests (v1, v2, v2.5 ledger ones) | S1–S3 | inline | — |
| T2 | `make test-db` also drops leftover `_sqlx_test_*` databases | S6 | inline | — |
| T3 | Prove each converted test still bites | S5 | inline | — |
| T4 | Verify the whole flow | S4, S7 | inline | — |

## Log

L1 (user, verbatim): "limpia la rama y arranca f2 en una rama nueva"

L2 (evidence, measured before writing): exactly 14 tests are pool-only; 6 use only
`BACKEND_BASE`; 23 use both. The 14 are the ones `#[sqlx::test]` can convert
without a second process, which is why F3 exists for the rest.

L3 (evidence, read from the crate source rather than assumed): `sqlx::test` lives in
`sqlx-macros` 0.9 and needs the `migrate` feature, which this crate already enables.
The migrator is inferred **only when the test takes arguments** — an argument-less
`#[sqlx::test]` creates a database but applies no migrations. The generated database
name is `_sqlx_test_<base64url(sha512(test path)[..39])>`, so it is stable per test
and a killed run leaves debris.

L4 (evidence, measured): role `tempox` has `rolcreatedb` and `rolsuper`, so it can
create and drop databases, which `#[sqlx::test]` requires.

L5 (evidence, the decisive one, and it took three attempts because my first two
probes were wrong): a converted test runs in **its own** database. The first probe
printed `current_database` and showed `_sqlx_test_i4PuXffNQwPXCCi5ZdkiSE9NH4XK_...`.
Asserting that inside the test is what makes it falsifiable:

```rust
assert!(db.starts_with("_sqlx_test_"), "expected a throwaway database, got {db}");
assert_ne!(db, "tempox_test", "the shared test database must not be used");
```

The two probes before it were useless because both reconnected to `DATABASE_URL`,
which is `tempox_test` and already has the schema — they would have passed with the
conversion completely broken. **A probe that shares configuration with the code
under test cannot falsify it.**

L6 (evidence, the migrator is inferred only with an argument): read from
`sqlx-macros-core` 0.9, `expand_advanced` adds the inferred migrator only under
`MigrationsOpt::InferredPath if !inputs.is_empty()`. An argument-less
`#[sqlx::test]` creates a database and applies **no migrations**. That is why every
converted test takes `pool: sqlx::PgPool` and none of them takes nothing.

L7 (defect found while verifying): `make test-db` left `_sqlx_test_*` databases
behind. They are named from a hash of the test path, so a killed run leaves one, and
the next run would reuse it with the dead run's state. `scripts/test-db.sh` now
sweeps them, and it is safe because the prefix is reserved by the framework.

Two bugs in my own sweep, both found by running it: `DROP DATABASE` cannot run in the
same `psql -c` as the `pg_terminate_backend` (psql wraps a multi-statement `-c` in a
transaction), so they are two calls now.

L8 (evidence, S5 honoured): instead of editing a migration — which only produced
`migration 4 was previously applied but has been modified` — a throwaway probe test
asserted, inside the framework's database, that the `stock_movements_append_only`
trigger **exists** there and that an `UPDATE` on it is refused with the append-only
message. It passed, so the migrations really run in the per-test database and the
guards really bite. The probe's own first version failed for a good reason: a
`purchase` movement with no document is rejected by the coherence trigger, which was
itself proof that the schema is live.

L9 (verification): **46 integration** pass (14 converted, 29 HTTP untouched, plus the
existing pool-only ones), **39 unit**, **139 frontend**, **16 e2e**; `cargo fmt
--check` and `clippy --all-targets -D warnings` clean; a full run takes ~8-10s and
leaves **zero** `_sqlx_test_*` databases behind.

L10 (state): `--test-threads=1` is deliberately unchanged (S7): it exists because
HTTP tests read shared cash totals, and F3 has not removed those yet.

L11 (review, lineage review-6afc107c3193c532, approved, authority burned): tier
**high** (the script drops databases), three lenses, no refuter. Five advisory
findings; three of them (`R1-dbname-quoting`, `R3-001`, `R3-002`) pointed at the
same place — the sweep interpolating a database name into SQL — and that one was
**not** advisory in substance, so it is fixed here.

L12 (defect found by review, measured, and it was exploitable): the sweep built
`DROP DATABASE IF EXISTS "$d"` and `WHERE datname = '$d'` by interpolating the name.
A database name is an arbitrary string that anything with CREATE privilege chooses.
Measured before the fix:

```text
CREATE DATABASE "_sqlx_test_evil'; DROP DATABASE tempox_test; --";
make test-db     ->  stopped with "DROP DATABASE cannot run inside a transaction block"
$ pg_database    ->  tempox_test survived
```

`tempox_test` survived **by luck**: the injected second statement landed inside the
same `psql -c`, and psql refuses a multi-statement `-c` in a transaction. That is the
same accident that had already forced the two-call split. Luck is not a control.

The fix, and why it is two mechanisms rather than one:

- `pg_terminate_backend` takes the name as a **bind parameter** (`:'dbname'`), so the
  name is data there;
- `DROP DATABASE` **cannot** take one, because an identifier is not a value, so the
  name is escaped **inside the server** with `format('%I')` and read back with `-tA`.
  Nothing is assembled on this side.

One more thing measured while fixing it: psql expands `:'var'` in a script read from
**stdin**, not in a `-c` argument, so the parameterised statements go through stdin.

Verified with both vectors at once: a name carrying `'; DROP DATABASE tempox_test; --`
and a name carrying a double quote. Both dropped, `tempox_test` untouched:

```text
dropped leftover _sqlx_test_a1'; DROP DATABASE tempox_test; --
dropped leftover _sqlx_test_b2"x
```

The earlier guards still bite: an off-stack host and a non-identifier name are still
refused.
